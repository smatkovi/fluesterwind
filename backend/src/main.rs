//! Der Signal-Dienst von Fluesterwind.
//!
//! Er spricht dieselbe HTTP-Schnittstelle wie das Go-Backend des
//! WhatsApp-Ports, damit die Oberflaeche dort wiederverwendet werden kann:
//! 127.0.0.1, ein Port aus einer festen Reihe, JSON.
//!
//! Warum ueberhaupt HTTP zwischen zwei Teilen derselben Anwendung: die
//! Oberflaeche ist Qt 4.7 und C++, der Unterbau Rust. Die beiden ueber eine
//! Fremdsprachenschnittstelle zu verbinden hiesse, die Rust-Seite als
//! Bibliothek gegen Harmattans uralte libstdc++ zu binden -- genau das,
//! was der statische musl-Bau vermeidet. Ein lokaler Socket kostet nichts
//! und haelt die beiden Welten getrennt.
//!
//! ## Warum ein einziger Faden
//!
//! presage und libsignal arbeiten mit `Rc`, ihre Futures sind also nicht
//! `Send` und lassen sich nicht auf einen Thread-Pool schieben --
//! `tokio::spawn` lehnt sie ab. Deshalb laeuft alles Signal-Nahe auf einem
//! `LocalSet` des Hauptfadens; Whisperfish macht es genauso.
//!
//! Der HTTP-Server bekommt dafuer einen eigenen Betriebssystemfaden (er
//! blockiert ohnehin beim Warten auf Anfragen) und verstaendigt sich mit
//! der Signal-Seite ueber zwei Wege, die beide `Send` sind: einen Kanal
//! fuer Befehle und einen geteilten Zustand hinter einem Mutex.

use std::sync::{Arc, Mutex};

use futures::channel::oneshot;
use presage::libsignal_service::configuration::SignalServers;
use presage::manager::{Manager, Registered};
use presage_store_sqlite::{OnNewIdentity, SqliteStore};
use tokio::sync::mpsc;

mod chats;
mod http;
mod zustand;

use chats::Schnappschuss;
use zustand::Lage;

/// Die Ports, die der Reihe nach probiert werden. Dieselben wie beim
/// WhatsApp-Backend, nur um zehn verschoben, damit beide nebeneinander
/// laufen koennen.
const PORTS: [u16; 5] = [8095, 8096, 8097, 8098, 8099];

const GERAETENAME: &str = "Fluesterwind (N9)";

/// Was die HTTP-Seite der Signal-Seite auftragen kann.
///
/// Mehr als das geht nicht ueber die Fadengrenze: presage ist nicht
/// `Send`, also darf die HTTP-Seite es nicht anfassen. Sie schickt einen
/// Auftrag und liest das Ergebnis spaeter aus dem Schnappschuss.
pub enum Befehl {
    Verknuepfen,
    Senden { an: String, text: String },
}

pub type GeteilteLage = Arc<Mutex<Lage>>;
pub type GeteilteChats = Arc<Mutex<Schnappschuss>>;

fn datenverzeichnis() -> std::path::PathBuf {
    let heim = std::env::var("HOME").unwrap_or_else(|_| "/home/user".into());
    std::path::PathBuf::from(heim).join(".local/share/harbour/fluesterwind")
}

fn main() {
    let verzeichnis = datenverzeichnis();
    if let Err(e) = std::fs::create_dir_all(&verzeichnis) {
        eprintln!("Datenverzeichnis nicht anlegbar: {e}");
        std::process::exit(1);
    }

    let laufzeit = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Laufzeit");
    let lokal = tokio::task::LocalSet::new();

    laufzeit.block_on(lokal.run_until(async move {
        // sqlite:// erwartet einen Pfad; mode=rwc legt die Datei an, wenn
        // sie fehlt.
        let db = format!("sqlite://{}/signal.db?mode=rwc", verzeichnis.display());
        let speicher = match SqliteStore::open(&db, OnNewIdentity::Trust).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Speicher nicht zu oeffnen: {e}");
                std::process::exit(1);
            }
        };

        let lage: GeteilteLage = Arc::new(Mutex::new(Lage::neu()));
        let chats: GeteilteChats = Arc::new(Mutex::new(Schnappschuss::default()));
        let (sender, mut empfaenger) = mpsc::unbounded_channel::<Befehl>();

        http::starten(&PORTS, lage.clone(), chats.clone(), sender);

        // Schon verknuepft? Dann gleich weitermachen.
        // Der angemeldete Manager, sobald es einen gibt. Er bleibt hier
        // auf dem Faden liegen; die HTTP-Seite kommt nur ueber Befehle an
        // ihn heran.
        let mut manager: Option<Manager<SqliteStore, Registered>> =
            match Manager::load_registered(speicher.clone()).await {
                Ok(m) => {
                    println!("✅ Bereits verknuepft");
                    Some(m)
                }
                Err(_) => {
                    println!("ℹ Noch nicht verknuepft - /pair anfordern");
                    None
                }
            };

        if let Some(m) = manager.as_mut() {
            uebernehmen(m, &lage).await;
            empfangen_starten(m.clone(), lage.clone(), chats.clone());
        }

        while let Some(befehl) = empfaenger.recv().await {
            match befehl {
                Befehl::Verknuepfen => {
                    match verknuepfen(speicher.clone(), lage.clone()).await {
                        Ok(mut m) => {
                            uebernehmen(&mut m, &lage).await;
                            empfangen_starten(m.clone(), lage.clone(), chats.clone());
                            manager = Some(m);
                        }
                        Err(e) => eprintln!("Verknuepfen gescheitert: {e}"),
                    }
                }
                Befehl::Senden { an, text } => {
                    let Some(m) = manager.as_mut() else {
                        eprintln!("Senden ohne Verknuepfung");
                        continue;
                    };
                    if let Err(e) = senden(m, &an, &text, &chats).await {
                        eprintln!("Senden gescheitert: {e}");
                        if let Ok(mut l) = lage.lock() {
                            l.fehler_setzen(e);
                        }
                    }
                }
            }
        }
    }));
}

/// Uebernimmt einen angemeldeten Manager in den Zustand.
async fn uebernehmen(manager: &mut Manager<SqliteStore, Registered>, lage: &GeteilteLage) {
    let nummer = manager.whoami().await.ok().map(|w| w.number.to_string());
    if let Ok(mut l) = lage.lock() {
        l.verknuepft(nummer);
        l.verbunden_setzen(true);
    }
}

/// Startet das Verknuepfen als Zweitgeraet und legt die Adresse ab, die
/// das Haupttelefon abscannen muss.
///
/// Signal kennt keinen Nummerncode wie WhatsApp: es gibt ausschliesslich
/// den Weg ueber eine `sgnl://linkdevice?...`-Adresse. Das N950 hat keine
/// Kamera-Anbindung, die daraus einen QR-Code machen koennte, also wird
/// die Adresse als Text herausgereicht -- zum Anzeigen mit einer QR-App
/// oder zum Einfuegen.
async fn verknuepfen(
    speicher: SqliteStore,
    lage: GeteilteLage,
) -> Result<Manager<SqliteStore, Registered>, String> {
    let (sender, empfaenger) = oneshot::channel();
    let l = lage.clone();

    tokio::task::spawn_local(async move {
        match empfaenger.await {
            Ok(adresse) => {
                let text = format!("{adresse}");
                println!("🔗 Verknuepfungsadresse: {text}");
                if let Ok(mut z) = l.lock() {
                    z.adresse_setzen(text);
                }
            }
            Err(_) => {
                if let Ok(mut z) = l.lock() {
                    z.fehler_setzen("Verknuepfung abgebrochen".into());
                }
            }
        }
    });

    match Manager::link_secondary_device(
        speicher,
        SignalServers::Production,
        GERAETENAME.to_string(),
        sender,
    )
    .await
    {
        Ok(m) => {
            println!("✅ Verknuepft");
            Ok(m)
        }
        Err(e) => {
            let text = format!("{e}");
            if let Ok(mut z) = lage.lock() {
                z.fehler_setzen(text.clone());
            }
            Err(text)
        }
    }
}

/// Haengt die Empfangsschleife an den Nachrichtenstrom.
///
/// Sie laeuft als eigene lokale Aufgabe weiter, solange der Dienst lebt.
/// Reisst der Strom ab, wird nach kurzer Pause neu aufgesetzt -- ohne das
/// bliebe der Dienst still, bis ihn jemand neu startet.
fn empfangen_starten(
    manager: Manager<SqliteStore, Registered>,
    lage: GeteilteLage,
    chats: GeteilteChats,
) {
    tokio::task::spawn_local(async move {
        let mut m = manager;
        loop {
            match m.receive_messages().await {
                Ok(strom) => {
                    if let Ok(mut l) = lage.lock() {
                        l.verbunden_setzen(true);
                    }
                    futures::pin_mut!(strom);
                    use futures::StreamExt;
                    while let Some(empfangen) = strom.next().await {
                        verarbeiten(&mut m, empfangen, &chats).await;
                    }
                }
                Err(e) => eprintln!("Empfang: {e}"),
            }
            if let Ok(mut l) = lage.lock() {
                l.verbunden_setzen(false);
            }
            eprintln!("Strom abgerissen - neuer Versuch in 10 s");
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
}

/// Macht aus einem empfangenen Stueck einen Eintrag im Schnappschuss.
async fn verarbeiten(
    manager: &mut Manager<SqliteStore, Registered>,
    empfangen: presage::model::messages::Received,
    chats: &GeteilteChats,
) {
    use presage::libsignal_service::content::ContentBody;
    use presage::libsignal_service::proto::sync_message::Content as SyncContent;
    use presage::model::messages::Received;
    use presage::store::Thread;

    let inhalt = match empfangen {
        Received::Content(c) => c,
        Received::QueueEmpty => {
            println!("📭 Warteschlange leer");
            return;
        }
        Received::Contacts => {
            println!("📇 Kontakte erhalten");
            return;
        }
        Received::DecryptionError(wer) => {
            eprintln!("🔒 nicht entschluesselbar von {}", wer.service_id_string());
            return;
        }
    };

    // Nur was Text traegt. Anhaenge kommen spaeter dazu; eine Nachricht
    // ohne Text jetzt schon einzutragen hiesse, leere Blasen zu zeigen.
    // Der Zeitstempel der Nachricht selbst, sonst der des Umschlags.
    // Metadata fuehrt ihn als DateTime, nicht als Zahl.
    let umschlag = inhalt.metadata.client_timestamp.timestamp_millis() as u64;

    let (text, zeit) = match &inhalt.body {
        ContentBody::DataMessage(d) => (
            d.body.clone().unwrap_or_default(),
            d.timestamp.unwrap_or(umschlag),
        ),
        // Was man selbst vom Haupttelefon aus geschrieben hat, kommt als
        // Synchronisierung herein -- sonst fehlte im Verlauf die eigene
        // Haelfte des Gespraechs.
        ContentBody::SynchronizeMessage(s) => match &s.content {
            Some(SyncContent::Sent(sent)) => match &sent.message {
                Some(d) => (
                    d.body.clone().unwrap_or_default(),
                    d.timestamp.unwrap_or(umschlag),
                ),
                None => return,
            },
            _ => return,
        },
        _ => return,
    };
    if text.is_empty() {
        return;
    }

    let Ok(thread) = Thread::try_from(&*inhalt) else {
        return;
    };
    let jid = match &thread {
        Thread::Contact(id) => chats::kennung_person(&id.service_id_string()),
        Thread::Group(schluessel) => chats::kennung_gruppe(schluessel),
    };
    let titel = manager.thread_title(&thread).await.unwrap_or_default();

    // Eine eigene Nachricht erkennt man daran, dass der Absender die
    // eigene Kennung traegt -- bei einer Synchronisierung vom
    // Haupttelefon ist das immer so.
    let eigene = manager.registration_data().service_ids.aci;
    let absender = inhalt.metadata.sender.raw_uuid();
    let von_mir = absender == eigene;

    let n = chats::Nachricht {
        // Der Zeitstempel ist bei Signal zugleich die Kennung.
        id: zeit.to_string(),
        chat_jid: jid,
        sender: inhalt.metadata.sender.service_id_string(),
        text,
        from_me: von_mir,
        timestamp: zeit / 1000,
    };
    if let Ok(mut s) = chats.lock() {
        s.eintragen(n, &titel);
    }
}

/// Schickt eine Nachricht -- an eine Person oder in eine Gruppe.
async fn senden(
    manager: &mut Manager<SqliteStore, Registered>,
    an: &str,
    text: &str,
    chats: &GeteilteChats,
) -> Result<(), String> {
    use presage::libsignal_service::content::ContentBody;
    use presage::libsignal_service::proto::DataMessage;
    use presage::libsignal_service::protocol::ServiceId;

    let zeit = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64;

    let nachricht = DataMessage {
        body: Some(text.to_string()),
        timestamp: Some(zeit),
        ..Default::default()
    };

    if chats::ist_gruppe(an) {
        let schluessel = chats::gruppenschluessel(an)
            .ok_or_else(|| "Gruppenkennung unlesbar".to_string())?;
        manager
            .send_message_to_group(&schluessel, ContentBody::DataMessage(nachricht), zeit)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        let roh = an.strip_prefix("c:").unwrap_or(an);
        let ziel = ServiceId::parse_from_service_id_string(roh)
            .ok_or_else(|| format!("Kennung unlesbar: {roh}"))?;
        manager
            .send_message(ziel, ContentBody::DataMessage(nachricht), zeit)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Die eigene Nachricht gleich zeigen, statt auf ihre Rueckmeldung zu
    // warten: sonst verschwindet sie beim Absenden, und genau das war beim
    // WhatsApp-Port der erste Fehlerbericht.
    if let Ok(mut s) = chats.lock() {
        let eigene = manager.registration_data().service_ids.aci.to_string();
        s.eintragen(
            chats::Nachricht {
                id: zeit.to_string(),
                chat_jid: an.to_string(),
                sender: eigene,
                text: text.to_string(),
                from_me: true,
                timestamp: zeit / 1000,
            },
            "",
        );
    }
    Ok(())
}

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
mod dbusname;
mod http;
mod medien;
mod sperre;
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
    /// Einen Anhang aufs Geraet holen. Nicht von selbst beim Laden des
    /// Verlaufs: auf 2G will man nicht jedes Bild eines Chats ungefragt
    /// herunterladen.
    MedienLaden { chat: String, id: String },
    DateiSenden { an: String, pfad: String, beschriftung: String },
    /// Lokal abmelden: Anmeldedaten und Datenbank loeschen. Vom Konto
    /// nehmen kann sich ein Zweitgeraet nicht selbst -- das geht nur am
    /// Hauptgeraet.
    Abmelden,
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

    // Vor allem anderen: nur eine Instanz. Zwei Dienste mit derselben
    // Geraetekennung werfen sich bei Signal gegenseitig vom Websocket.
    if let Err(e) = sperre::nehmen(&verzeichnis) {
        eprintln!("↩ {e} - dieser Start endet hier");
        std::process::exit(0);
    }

    let laufzeit = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Laufzeit");
    let lokal = tokio::task::LocalSet::new();

    let verzeichnis_fuer_block = verzeichnis.clone();
    laufzeit.block_on(lokal.run_until(async move {
        let verzeichnis = verzeichnis_fuer_block;
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

        // Vor allem Netzkram: den Dienstnamen holen, damit eine
        // D-Bus-Aktivierung als gelungen gilt.
        dbusname::beanspruchen().await;

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
            schnappschuss_fuellen(m, &chats).await;
            // Die Kontaktliste liegt beim Hauptgeraet; ein Zweitgeraet
            // muss sie sich schicken lassen. Ohne das stehen in der
            // Chatliste nur Kennungen statt Namen.
            if let Err(e) = m.request_contacts().await {
                eprintln!("Kontakte nicht angefordert: {e}");
            }
            empfangen_starten(m.clone(), lage.clone(), chats.clone());
        }

        while let Some(befehl) = empfaenger.recv().await {
            match befehl {
                Befehl::Verknuepfen => {
                    match verknuepfen(speicher.clone(), lage.clone()).await {
                        Ok(mut m) => {
                            uebernehmen(&mut m, &lage).await;
                            if let Err(e) = m.request_contacts().await {
                                eprintln!("Kontakte nicht angefordert: {e}");
                            }
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
                    if let Err(e) = senden(m, &an, &text, None, &chats).await {
                        eprintln!("Senden gescheitert: {e}");
                        if let Ok(mut l) = lage.lock() {
                            l.fehler_setzen(e);
                        }
                    }
                }
                Befehl::MedienLaden { chat, id } => {
                    let Some(m) = manager.as_mut() else { continue };
                    if let Err(e) = medien_laden(m, &chat, &id, &chats).await {
                        eprintln!("Anhang nicht geladen: {e}");
                        if let Ok(mut l) = lage.lock() {
                            l.fehler_setzen(e);
                        }
                    }
                }
                Befehl::Abmelden => {
                    if let Some(m) = manager.as_mut() {
                        abmelden(m, &verzeichnis).await;
                    }
                }
                Befehl::DateiSenden { an, pfad, beschriftung } => {
                    let Some(m) = manager.as_mut() else { continue };
                    if let Err(e) =
                        datei_senden(m, &an, &pfad, &beschriftung, &chats).await
                    {
                        eprintln!("Datei nicht verschickt: {e}");
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
    let eigenes: u32 = manager.device_id().into();

    // Was der Server fuehrt, nicht was wir glauben. Steht das eigene
    // Geraet nicht in dieser Liste, ist die Verknuepfung nicht zustande
    // gekommen -- egal wie zuversichtlich der eigene Zustand klingt.
    let liste = match manager.devices().await {
        Ok(g) => g
            .into_iter()
            .map(|d| {
                let id: u32 = d.id.into();
                format!(
                    "{}: {} (seit {})",
                    id,
                    d.name.unwrap_or_else(|| "ohne Namen".into()),
                    d.created_at.format("%d.%m. %H:%M")
                )
            })
            .collect(),
        Err(e) => {
            eprintln!("Geraeteliste nicht abrufbar: {e}");
            Vec::new()
        }
    };
    println!("📱 eigenes Geraet {eigenes}, {} am Konto", liste.len());
    for g in &liste {
        println!("   {g}");
    }

    if let Ok(mut l) = lage.lock() {
        l.verknuepft(nummer);
        l.verbunden_setzen(true);
        l.geraete_setzen(eigenes, liste);
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
        // Beide sind der Anlass, den Schnappschuss neu aufzubauen.
        //
        // Die Kontaktliste kommt vom Hauptgeraet und trifft erst Sekunden
        // nach dem Verbinden ein -- wer nur beim Start liest, sieht sie
        // nie und zeigt eine leere Chatliste, obwohl die Datenbank voll
        // ist. Genau das war beim ersten Lauf zu sehen: "0 Unterhaltungen
        // im Speicher", und gleich darauf "Kontakte erhalten".
        //
        // Ein zweiter Durchlauf richtet keinen Schaden an: Nachrichten
        // werden ueber ihren Zeitstempel erkannt, vorhandene Chats nicht
        // noch einmal angelegt.
        Received::QueueEmpty => {
            println!("📭 Warteschlange leer");
            schnappschuss_fuellen(manager, chats).await;
            return;
        }
        Received::Contacts => {
            println!("📇 Kontakte erhalten");
            schnappschuss_fuellen(manager, chats).await;
            return;
        }
        Received::DecryptionError(wer) => {
            eprintln!("🔒 nicht entschluesselbar von {}", wer.service_id_string());
            return;
        }
    };

    let Ok(thread) = Thread::try_from(&*inhalt) else {
        return;
    };
    let jid = match &thread {
        Thread::Contact(id) => chats::kennung_person(&id.service_id_string()),
        Thread::Group(schluessel) => chats::kennung_gruppe(schluessel),
    };
    let titel = manager.thread_title(&thread).await.unwrap_or_default();
    let eigene = manager.registration_data().service_ids.aci;

    // Jede Nachricht traegt den Profilschluessel ihres Absenders bei
    // sich. Das ist nach den Gruppenmitgliedern die zweite Quelle dafuer
    // -- und fuer jemanden, mit dem man keine Gruppe teilt, die einzige.
    // Ohne den Schluessel gibt Signal kein Profilbild heraus.
    if let presage::libsignal_service::content::ContentBody::DataMessage(d) = &inhalt.body {
        if let Some(pk) = &d.profile_key {
            if let Ok(mut c) = chats.lock() {
                c.profilschluessel_setzen(
                    &inhalt.metadata.sender.service_id_string(),
                    pk.clone(),
                );
            }
        }
    }

    // Reaktionen kommen als eigene Nachrichten herein, die weder Text
    // noch Anhang tragen -- vor dieser Abfrage fielen sie durch, und
    // Michaels Daumen war nirgends zu sehen. Sie zeigen ueber den
    // Zeitstempel auf die Nachricht, der sie gelten.
    if let presage::libsignal_service::content::ContentBody::DataMessage(d) = &inhalt.body {
        if let Some(r) = &d.reaction {
            let ziel = r.target_sent_timestamp.unwrap_or(0);
            if ziel != 0 {
                let wer = inhalt.metadata.sender.service_id_string();
                if let Ok(mut c) = chats.lock() {
                    let anzeige = c.name_zu(&wer);
                    c.reaktion_setzen(
                        &jid,
                        &ziel.to_string(),
                        r.emoji.as_deref().unwrap_or(""),
                        &anzeige,
                        r.remove.unwrap_or(false),
                    );
                }
                println!("😀 Reaktion {} auf {ziel}", r.emoji.as_deref().unwrap_or("-"));
            }
            return;
        }
    }

    // Derselbe Bauer wie beim Lesen aus dem Speicher -- zwei Fassungen
    // davon waeren zwei Gelegenheiten, sie auseinanderlaufen zu lassen.
    let Some(n) = nachricht_aus(&inhalt, &jid, eigene) else {
        return;
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
    anhang: Option<presage::libsignal_service::proto::AttachmentPointer>,
    chats: &GeteilteChats,
) -> Result<(), String> {
    use presage::libsignal_service::content::ContentBody;
    use presage::libsignal_service::proto::DataMessage;
    use presage::libsignal_service::protocol::ServiceId;

    let zeit = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64;

    let anhaenge = anhang.clone().map(|a| vec![a]).unwrap_or_default();
    let nachricht = DataMessage {
        // Ein Anhang ohne Begleittext ist der Normalfall -- dann bleibt
        // body leer statt eine leere Zeichenkette zu schicken.
        body: if text.is_empty() { None } else { Some(text.to_string()) },
        timestamp: Some(zeit),
        attachments: anhaenge,
        ..Default::default()
    };

    if chats::ist_gruppe(an) {
        let schluessel = chats::gruppenschluessel(an)
            .ok_or_else(|| "Gruppenkennung unlesbar".to_string())?;
        println!("➡ an Gruppe {} ({} Byte Schluessel)", &an[..14.min(an.len())],
                 schluessel.len());
        manager
            .send_message_to_group(&schluessel, ContentBody::DataMessage(nachricht), zeit)
            .await
            .map_err(|e| e.to_string())?;
        println!("➡ Gruppennachricht abgeschickt");
    } else {
        let roh = an.strip_prefix("c:").unwrap_or(an);
        let ziel = ServiceId::parse_from_service_id_string(roh)
            .ok_or_else(|| format!("Kennung unlesbar: {roh}"))?;
        println!("➡ an {}", &roh[..12.min(roh.len())]);
        manager
            .send_message(ziel, ContentBody::DataMessage(nachricht), zeit)
            .await
            .map_err(|e| e.to_string())?;
        println!("➡ abgeschickt");
    }

    // Die eigene Nachricht gleich zeigen, statt auf ihre Rueckmeldung zu
    // warten: sonst verschwindet sie beim Absenden, und genau das war beim
    // WhatsApp-Port der erste Fehlerbericht.
    if let Ok(mut s) = chats.lock() {
        let eigene = manager.registration_data().service_ids.aci.to_string();
        let mut n = chats::Nachricht::text(
            zeit.to_string(),
            an.to_string(),
            eigene,
            text.to_string(),
            true,
            zeit / 1000,
        );
        if let Some(a) = &anhang {
            let (mime, name, groesse) = medien::beschreibung(a);
            n.media_type = medien::art(&mime).to_string();
            n.file_name = name;
            n.size = groesse;
        }
        s.eintragen(n, "");
    }
    Ok(())
}

/// Baut den Schnappschuss aus dem, was schon im Speicher liegt.
///
/// Die Empfangsschleife sieht nur, was waehrend ihrer Laufzeit
/// hereinkommt. Nach einem Neustart -- und direkt nach dem Verknuepfen --
/// waere die Chatliste deshalb leer, obwohl die Datenbank voll ist. Also
/// einmal durch Kontakte und Gruppen gehen und ihre Unterhaltungen holen.
async fn schnappschuss_fuellen(
    manager: &mut Manager<SqliteStore, Registered>,
    chats: &GeteilteChats,
) {
    use presage::libsignal_service::protocol::{Aci, ServiceId};
    use presage::store::{ContentsStore, Thread};

    let mut faeden: Vec<(Thread, String)> = Vec::new();
    // Kennung -> Name. Ohne diese Karte steht in Gruppen eine rohe UUID
    // ueber jeder fremden Nachricht.
    let mut namen: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    match manager.store().contacts().await {
        Ok(liste) => {
            for k in liste.flatten() {
                let id = ServiceId::from(Aci::from(k.uuid));
                if !k.name.is_empty() {
                    namen.insert(id.service_id_string(), k.name.clone());
                }
                faeden.push((Thread::Contact(id), k.name));
            }
        }
        Err(e) => eprintln!("Kontakte nicht lesbar: {e}"),
    }
    if let Ok(mut s) = chats.lock() {
        s.namen_setzen(namen.clone());
    }
    // Profilschluessel, eingesammelt aus den Gruppen.
    let mut schluessel: Vec<(String, Vec<u8>)> = Vec::new();

    match manager.store().groups().await {
        Ok(liste) => {
            for (schluessel_bytes, g) in liste.flatten() {
                let jid = chats::kennung_gruppe(&schluessel_bytes);
                // Die Mitglieder gleich mit ablegen: die Gruppendaten
                // liegen ohnehin gerade offen, und die Oberflaeche kommt
                // an den Signal-Faden nicht heran.
                let mitglieder: Vec<chats::Mitglied> = g
                    .members
                    .iter()
                    .map(|m| {
                        let kennung = ServiceId::from(m.aci).service_id_string();
                        // Der Profilschluessel dieses Mitglieds ist oft
                        // der einzige, den wir zu dieser Person haben.
                        schluessel.push((
                            kennung.clone(),
                            m.profile_key.get_bytes().to_vec(),
                        ));
                        chats::Mitglied {
                            name: namen
                                .get(&kennung)
                                .cloned()
                                .unwrap_or_else(|| kennung.clone()),
                            jid: chats::kennung_person(&kennung),
                            // Role kommt aus groups_v2, nicht aus den
                            // Protobuf-Definitionen -- gleiche Namen,
                            // andere Herkunft.
                            is_admin: matches!(
                                m.role,
                                presage::libsignal_service::groups_v2::Role::Administrator
                            ),
                        }
                    })
                    .collect();
                if let Ok(mut c) = chats.lock() {
                    c.mitglieder_setzen(&jid, mitglieder);
                }
                faeden.push((Thread::Group(schluessel_bytes), g.title));
            }
        }
        Err(e) => eprintln!("Gruppen nicht lesbar: {e}"),
    }

    if let Ok(mut c) = chats.lock() {
        for (kennung, k) in &schluessel {
            c.profilschluessel_setzen(kennung, k.clone());
        }
    }
    println!("📇 {} Unterhaltungen im Speicher, {} Profilschluessel",
             faeden.len(), schluessel.len());

    let eigene = manager.registration_data().service_ids.aci;
    let mut gefunden = 0usize;
    for (thread, titel) in faeden {
        let Ok(verlauf) = manager.store().messages(&thread, ..).await else {
            continue;
        };
        let jid = match &thread {
            Thread::Contact(id) => chats::kennung_person(&id.service_id_string()),
            Thread::Group(s) => chats::kennung_gruppe(s),
        };
        for inhalt in verlauf.flatten() {
            // Auch die gespeicherten Nachrichten tragen Schluessel.
            if let presage::libsignal_service::content::ContentBody::DataMessage(d) =
                &inhalt.body
            {
                if let Some(pk) = &d.profile_key {
                    if let Ok(mut c) = chats.lock() {
                        c.profilschluessel_setzen(
                            &inhalt.metadata.sender.service_id_string(),
                            pk.clone(),
                        );
                    }
                }
            }
            if let Some(n) = nachricht_aus(&inhalt, &jid, eigene) {
                gefunden += 1;
                if let Ok(mut s) = chats.lock() {
                    s.eintragen(n, &titel);
                }
            }
        }
        // Auch eine Unterhaltung ohne Text gehoert in die Liste: sonst
        // verschwindet ein Kontakt, mit dem man noch nicht geschrieben hat,
        // und man kann ihn nicht anschreiben.
        if let Ok(mut s) = chats.lock() {
            s.anlegen(&jid, &titel);
        }
    }
    println!("📜 {gefunden} Nachrichten aus dem Speicher");
    avatare_holen(manager, chats).await;
}

/// Macht aus einem gespeicherten Inhalt eine Nachricht -- oder nichts,
/// wenn kein Text daran haengt.
fn nachricht_aus(
    inhalt: &presage::libsignal_service::content::Content,
    jid: &str,
    // Die eigene Kennung als Uuid: service_ids.aci ist das Feld (Uuid),
    // service_ids.aci() die Methode (Aci). Hier genuegt die Uuid, und der
    // Absender liefert mit raw_uuid() dasselbe.
    eigene: presage::libsignal_service::prelude::Uuid,
) -> Option<chats::Nachricht> {
    use presage::libsignal_service::content::ContentBody;
    use presage::libsignal_service::proto::sync_message::Content as SyncContent;

    let umschlag = inhalt.metadata.client_timestamp.timestamp_millis() as u64;
    let daten = match &inhalt.body {
        ContentBody::DataMessage(d) => d,
        // Was man selbst vom Hauptgeraet aus geschrieben hat, kommt als
        // Synchronisierung herein -- sonst fehlte im Verlauf die eigene
        // Haelfte des Gespraechs.
        ContentBody::SynchronizeMessage(s) => match &s.content {
            Some(SyncContent::Sent(sent)) => sent.message.as_ref()?,
            _ => return None,
        },
        _ => return None,
    };
    let zeit = daten.timestamp.unwrap_or(umschlag);
    let text = daten.body.clone().unwrap_or_default();

    // Der erste Anhang bestimmt, wie die Nachricht aussieht. Mehrere in
    // einer Nachricht kommen vor, sind aber selten -- und in einer Blase
    // waeren sie ohnehin nicht unterzubringen.
    let (art, name, groesse) = match daten.attachments.first() {
        Some(a) => {
            let (mime, name, groesse) = medien::beschreibung(a);
            (medien::art(&mime).to_string(), name, groesse)
        }
        None => (String::new(), String::new(), 0),
    };

    // Antwortbezug: wer auf eine Nachricht antwortet, schickt einen
    // Ausschnitt davon mit. Ohne ihn steht die Antwort ohne Zusammenhang
    // da -- im Feld war genau das zu sehen.
    let (zitat_text, zitat_von) = match &daten.quote {
        Some(q) => (
            q.text.clone().unwrap_or_default(),
            q.author_aci.clone().unwrap_or_default(),
        ),
        None => (String::new(), String::new()),
    };

    // Ohne Text und ohne Anhang gibt es nichts zu zeigen; eine leere
    // Blase waere schlimmer als gar keine.
    if text.is_empty() && art.is_empty() {
        return None;
    }

    Some(chats::Nachricht {
        id: zeit.to_string(),
        chat_jid: jid.to_string(),
        sender: inhalt.metadata.sender.service_id_string(),
        text,
        // raw_uuid() gibt eine Uuid; Aci ist ein eigener Typ darueber.
        // Ueber die Uuid vergleichen, dann passt es auf beiden Seiten.
        from_me: inhalt.metadata.sender.raw_uuid() == eigene,
        timestamp: zeit / 1000,
        media_type: art,
        file_name: name,
        size: groesse,
        local_path: String::new(),
        reaktionen: String::new(),
        quoted_text: zitat_text,
        quoted_sender: zitat_von,
    })
}

/// Der Thread zu einer Chat-Kennung.
fn thread_aus(jid: &str) -> Option<presage::store::Thread> {
    use presage::libsignal_service::protocol::ServiceId;
    use presage::store::Thread;

    if let Some(hex) = jid.strip_prefix("g:") {
        let bytes = chats::gruppenschluessel(&format!("g:{hex}"))?;
        let feld: [u8; 32] = bytes.try_into().ok()?;
        return Some(Thread::Group(feld));
    }
    let roh = jid.strip_prefix("c:").unwrap_or(jid);
    Some(Thread::Contact(ServiceId::parse_from_service_id_string(roh)?))
}

/// Holt den Anhang einer Nachricht aufs Geraet.
///
/// Der Anhang wird nicht zwischengespeichert: die Nachricht liegt samt
/// ihrem Zeiger im Speicher, und der Zeitstempel ist bei Signal zugleich
/// ihre Kennung. Also nachschlagen statt mitschleppen.
async fn medien_laden(
    manager: &mut Manager<SqliteStore, Registered>,
    chat: &str,
    id: &str,
    chats: &GeteilteChats,
) -> Result<(), String> {
    use presage::libsignal_service::content::ContentBody;
    use presage::libsignal_service::proto::sync_message::Content as SyncContent;
    use presage::store::ContentsStore;

    let thread = thread_aus(chat).ok_or_else(|| format!("Kennung unlesbar: {chat}"))?;
    let zeit: u64 = id.parse().map_err(|_| format!("Kennung unlesbar: {id}"))?;

    let inhalt = manager
        .store()
        .message(&thread, zeit)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Nachricht nicht im Speicher".to_string())?;

    let daten = match &inhalt.body {
        ContentBody::DataMessage(d) => d,
        ContentBody::SynchronizeMessage(s) => match &s.content {
            Some(SyncContent::Sent(sent)) => sent
                .message
                .as_ref()
                .ok_or_else(|| "keine Nachricht darin".to_string())?,
            _ => return Err("kein Anhang".into()),
        },
        _ => return Err("kein Anhang".into()),
    };
    let zeiger = daten
        .attachments
        .first()
        .ok_or_else(|| "kein Anhang".to_string())?;

    let (mime, name, _) = medien::beschreibung(zeiger);
    println!("📎 hole Anhang ({mime}) aus {chat}");
    let roh = manager
        .get_attachment(zeiger)
        .await
        .map_err(|e| e.to_string())?;
    let pfad = medien::ablegen(&roh, &mime, &name, zeit)?;
    println!("📎 abgelegt: {}", pfad.display());

    if let Ok(mut s) = chats.lock() {
        s.pfad_setzen(chat, id, &pfad.to_string_lossy());
    }
    Ok(())
}

/// Verschickt eine Datei als Anhang.
async fn datei_senden(
    manager: &mut Manager<SqliteStore, Registered>,
    an: &str,
    pfad: &str,
    beschriftung: &str,
    chats: &GeteilteChats,
) -> Result<(), String> {
    use presage::libsignal_service::sender::AttachmentSpec;

    let p = std::path::Path::new(pfad);
    let roh = std::fs::read(p).map_err(|e| format!("{pfad}: {e}"))?;
    let mime = medien::mime_aus_pfad(p);
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "datei".into());

    let spec = AttachmentSpec {
        content_type: mime.clone(),
        length: roh.len(),
        file_name: Some(name.clone()),
        preview: None,
        voice_note: None,
        borderless: None,
        width: None,
        height: None,
        caption: if beschriftung.is_empty() {
            None
        } else {
            Some(beschriftung.to_string())
        },
        blur_hash: None,
    };

    println!("📎 lade hoch: {name} ({} B, {mime})", roh.len());
    let zeiger = manager
        .upload_attachment(spec, roh)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    senden(manager, an, beschriftung, Some(zeiger), chats).await
}

/// Holt die Profilbilder, die noch fehlen.
///
/// Nicht alle auf einmal und nicht bei jedem Durchlauf: bei hundert
/// Kontakten waeren das hundert Abrufe, und auf 2G legt das den Dienst
/// fuer Minuten lahm. Ein paar je Durchlauf genuegen -- nach wenigen
/// Runden sind die haeufigen Chats versorgt, und die stehen oben.
async fn avatare_holen(
    manager: &mut Manager<SqliteStore, Registered>,
    chats: &GeteilteChats,
) {
    use presage::libsignal_service::protocol::ServiceId;
    use presage::libsignal_service::zkgroup::profiles::ProfileKey;
    use presage::store::ContentsStore;

    const JE_DURCHLAUF: usize = 8;

    let offen: Vec<String> = match chats.lock() {
        Ok(c) => c.ohne_avatar(),
        Err(_) => return,
    };
    if offen.is_empty() {
        return;
    }

    let mut geholt = 0usize;
    let mut ohne_kontakt = 0usize;
    let mut ohne_schluessel = 0usize;
    let mut schon_da = 0usize;
    for jid in offen {
        if geholt >= JE_DURCHLAUF {
            break;
        }
        // Schon einmal geholt? Dann nur den Pfad nachtragen, nicht das
        // Netz bemuehen.
        let pfad = medien::avatar_pfad(&jid);
        if pfad.exists() {
            schon_da += 1;
            // Eine leere Datei heisst "nachgefragt, nichts hinterlegt" --
            // die darf nicht als Bild eingetragen werden, sonst zeigt die
            // Oberflaeche ein kaputtes Bild statt des Buchstabenkreises.
            if std::fs::metadata(&pfad).map(|m| m.len() > 0).unwrap_or(false) {
                if let Ok(mut c) = chats.lock() {
                    c.avatar_setzen(&jid, &pfad.to_string_lossy());
                }
            }
            continue;
        }

        let bild: Option<Vec<u8>> = if chats::ist_gruppe(&jid) {
            // Gruppenbilder brauchen den Hauptschluessel in einem
            // GroupContextV2 -- mehr als den kennt retrieve_group_avatar
            // nicht.
            let Some(schluessel) = chats::gruppenschluessel(&jid) else {
                continue;
            };
            let kontext = presage::libsignal_service::proto::GroupContextV2 {
                master_key: Some(schluessel),
                revision: None,
                group_change: None,
            };
            manager.retrieve_group_avatar(kontext).await.ok().flatten()
        } else {
            let roh = jid.strip_prefix("c:").unwrap_or(&jid);
            let Some(id) = ServiceId::parse_from_service_id_string(roh) else {
                continue;
            };
            // Ohne den Profilschluessel geht es nicht -- er steht im
            // Kontakt, und ohne Kontakt gibt es kein Bild.
            let Ok(Some(kontakt)) = manager.store().contact_by_id(&id).await else {
                ohne_kontakt += 1;
                continue;
            };
            // Erst der Kontakt, dann die Karte aus den Gruppen.
            let roh_schluessel = if kontakt.profile_key.len() == 32 {
                Some(kontakt.profile_key.clone())
            } else {
                chats.lock().ok().and_then(|c| c.profilschluessel(roh))
            };
            let Some(roh_schluessel) = roh_schluessel else {
                ohne_schluessel += 1;
                continue;
            };
            let Ok(feld) = <[u8; 32]>::try_from(roh_schluessel.as_slice()) else {
                ohne_schluessel += 1;
                continue;
            };
            manager
                .retrieve_profile_avatar_by_uuid(
                    kontakt.uuid,
                    ProfileKey::create(feld),
                )
                .await
                .ok()
                .flatten()
        };

        geholt += 1;
        let Some(daten) = bild else {
            // Kein Bild hinterlegt. Eine leere Datei anlegen, damit nicht
            // bei jedem Durchlauf erneut gefragt wird.
            let _ = medien::avatar_ablegen(&jid, &[]);
            continue;
        };
        match medien::avatar_ablegen(&jid, &daten) {
            Ok(p) => {
                if let Ok(mut c) = chats.lock() {
                    c.avatar_setzen(&jid, &p.to_string_lossy());
                }
            }
            Err(e) => eprintln!("Profilbild nicht ablegbar: {e}"),
        }
    }
    println!(
        "🖼 Profilbilder: {geholt} abgefragt, {schon_da} lagen schon vor, \
         {ohne_kontakt} ohne Kontakt, {ohne_schluessel} ohne Profilschluessel"
    );
}

/// Meldet dieses Geraet lokal ab.
///
/// Was hier NICHT geht: sich selbst vom Konto nehmen. presage verweigert
/// unlink_secondary auf einem Zweitgeraet ausdruecklich -- "secondary
/// devices cannot unlink themselves or other devices, it will fail with an
/// unauthorized error". Das Entfernen gehoert ans Hauptgeraet, und die
/// Oberflaeche sagt das auch.
///
/// Was geht, ist der lokale Teil: Anmeldedaten und Datenbank loeschen.
/// Danach beendet sich der Dienst -- eine offene SQLite-Verbindung auf
/// eine geloeschte Datei ist kein Zustand, in dem man weiterarbeiten
/// moechte. Der Anstossjob oder die App holen ihn zurueck, dann ohne
/// Verknuepfung.
async fn abmelden(
    manager: &mut Manager<SqliteStore, Registered>,
    verzeichnis: &std::path::Path,
) {
    use presage::store::StateStore;

    println!("👋 Abmelden angefordert");
    if let Err(e) = manager.store().clone().clear_registration().await {
        eprintln!("Anmeldedaten nicht geloescht: {e}");
    }
    for name in ["signal.db", "signal.db-wal", "signal.db-shm"] {
        let p = verzeichnis.join(name);
        if p.exists() {
            if let Err(e) = std::fs::remove_file(&p) {
                eprintln!("{}: {e}", p.display());
            }
        }
    }
    // Auch die Profilbilder: sie gehoeren zu einem Konto, von dem wir uns
    // gerade trennen.
    let _ = std::fs::remove_dir_all(verzeichnis.join("avatare"));
    println!("👋 abgemeldet - Dienst beendet sich");
    std::process::exit(0);
}

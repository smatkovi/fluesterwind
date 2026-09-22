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

mod http;
mod zustand;

use zustand::Lage;

/// Die Ports, die der Reihe nach probiert werden. Dieselben wie beim
/// WhatsApp-Backend, nur um zehn verschoben, damit beide nebeneinander
/// laufen koennen.
const PORTS: [u16; 5] = [8095, 8096, 8097, 8098, 8099];

const GERAETENAME: &str = "Fluesterwind (N9)";

/// Was die HTTP-Seite der Signal-Seite auftragen kann.
pub enum Befehl {
    Verknuepfen,
}

pub type GeteilteLage = Arc<Mutex<Lage>>;

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
        let (sender, mut empfaenger) = mpsc::unbounded_channel::<Befehl>();

        http::starten(&PORTS, lage.clone(), sender);

        // Schon verknuepft? Dann gleich weitermachen.
        match Manager::load_registered(speicher.clone()).await {
            Ok(m) => {
                println!("✅ Bereits verknuepft");
                uebernehmen(m, &lage).await;
            }
            Err(_) => println!("ℹ Noch nicht verknuepft - /pair anfordern"),
        }

        while let Some(befehl) = empfaenger.recv().await {
            match befehl {
                Befehl::Verknuepfen => {
                    let s = speicher.clone();
                    let l = lage.clone();
                    // spawn_local, nicht spawn: die Futures sind !Send.
                    tokio::task::spawn_local(async move {
                        if let Err(e) = verknuepfen(s, l).await {
                            eprintln!("Verknuepfen gescheitert: {e}");
                        }
                    });
                }
            }
        }
    }));
}

/// Uebernimmt einen angemeldeten Manager in den Zustand.
async fn uebernehmen(manager: Manager<SqliteStore, Registered>, lage: &GeteilteLage) {
    let mut manager = manager;
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
async fn verknuepfen(speicher: SqliteStore, lage: GeteilteLage) -> Result<(), String> {
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
            uebernehmen(m, &lage).await;
            Ok(())
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

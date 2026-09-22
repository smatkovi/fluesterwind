//! Eine exklusive Sperre auf dem Datenverzeichnis.
//!
//! Der Anlass ist ein beobachteter Fehler, nicht eine Vorsichtsmassnahme:
//! die Oberflaeche startet den Dienst mit, ohne zu pruefen, ob schon einer
//! laeuft. Der zweite fand Port 8095 belegt, nahm 8096 -- und lief
//! weiter. Zwei Dienste mit derselben Geraetekennung an Signal warfen
//! sich daraufhin gegenseitig vom Websocket ("Strom abgerissen" im Takt),
//! Senden scheiterte mit "Websocket closing while waiting for a
//! response", und beide schrieben in dieselbe SQLite-Datei.
//!
//! Beim WhatsApp-Port endete dieselbe Konstellation damit, dass eine
//! Instanz ihren leeren Nachrichtenspeicher ueber den vollen schrieb.
//!
//! flock ist hier das richtige Mittel: es haengt am Dateideskriptor und
//! verschwindet mit dem Prozess, auch wenn er abstuerzt. Kein verwaister
//! Sperreintrag, um den sich jemand kuemmern muesste.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::Path;

/// Offen gehalten, solange der Prozess laeuft -- mit dem Deskriptor faellt
/// die Sperre.
static mut SPERRDATEI: Option<std::fs::File> = None;

/// Belegt das Datenverzeichnis. Gelingt es nicht, laeuft schon eine
/// andere Instanz und diese hier soll sich beenden.
pub fn nehmen(verzeichnis: &Path) -> Result<(), String> {
    let pfad = verzeichnis.join(".lock");
    let datei = match OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&pfad)
    {
        Ok(f) => f,
        Err(e) => {
            // Kein Verzeichnis, kein Platz -- lieber weiterlaufen als gar
            // nicht starten. Die Sperre ist eine Absicherung, keine
            // Voraussetzung.
            eprintln!("⚠ Sperre nicht anlegbar ({e}) - laufe ohne");
            return Ok(());
        }
    };

    // LOCK_EX | LOCK_NB
    let ergebnis = unsafe { flock(datei.as_raw_fd(), 2 | 4) };
    if ergebnis != 0 {
        return Err(format!("eine andere Instanz haelt {}", pfad.display()));
    }

    let mut datei = datei;
    let _ = writeln!(datei, "{}", std::process::id());
    unsafe {
        SPERRDATEI = Some(datei);
    }
    Ok(())
}

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

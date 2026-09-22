//! Der Dienstname am Sitzungsbus.
//!
//! Harmattan hat keinen brauchbaren Weg, einen Benutzerdienst beim Start
//! hochzuziehen: Jobs unter ~/.config/upstart liest niemand, und nach
//! /etc/init/xsession/ kommt ein unsigniertes Paket nicht -- Aegis
//! verweigert dort jede Datei ohne Referenz-Hash. Was bleibt, ist die
//! Aktivierung ueber den Sitzungs-D-Bus: ein darueber gestarteter Dienst
//! laeuft als Besitzer des Busses, also als "user", mit HOME und Zugriff
//! aufs Datenverzeichnis.
//!
//! Dafuer muss der Dienst den Namen aber auch beanspruchen. Ohne das gilt
//! die Aktivierung als gescheitert -- und im Feld hiess das: nach einem
//! Neustart kam der Dienst nicht von selbst zurueck, der Anstoss lief ins
//! Leere, und das Telefon war still, bis jemand die App oeffnete.

const NAME: &str = "org.smatkovi.Fluesterwind";

/// Meldet den Dienst am Sitzungsbus an.
///
/// Scheitert es, laeuft der Dienst trotzdem weiter: gestartet wurde er
/// dann eben von der App und nicht von D-Bus, und das ist kein Fehler.
pub async fn beanspruchen() {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            println!("⚠ Sitzungsbus nicht erreichbar ({e}) - laeuft ohne Dienstnamen");
            return;
        }
    };
    match conn
        .request_name_with_flags(NAME, zbus::fdo::RequestNameFlags::DoNotQueue.into())
        .await
    {
        Ok(zbus::fdo::RequestNameReply::PrimaryOwner) => {
            println!("🔌 Dienstname {NAME} beansprucht");
            // Die Verbindung muss leben bleiben, sonst faellt der Name
            // sofort wieder weg. Sie wird deshalb absichtlich vergessen.
            std::mem::forget(conn);
        }
        Ok(andere) => {
            // Jemand haelt ihn schon. Zusammen mit der Dateisperre ist das
            // der zweite Riegel gegen zwei Instanzen.
            println!("⚠ {NAME} gehoert bereits einer anderen Instanz ({andere:?})");
        }
        Err(e) => println!("⚠ Dienstname nicht zu haben ({e})"),
    }
}

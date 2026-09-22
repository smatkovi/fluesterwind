//! Die HTTP-Seite.
//!
//! Sie laeuft auf einem eigenen Betriebssystemfaden: tiny_http blockiert
//! beim Warten auf Anfragen, und der Faden, auf dem die Signal-Verbindung
//! liegt, darf nicht blockieren. Hinueber geht nur, was `Send` ist -- der
//! geteilte Zustand und ein Befehlskanal.

use std::sync::mpsc as std_mpsc;

use tokio::sync::mpsc::UnboundedSender;

use crate::{Befehl, GeteilteChats, GeteilteLage};

pub fn starten(
    ports: &[u16],
    lage: GeteilteLage,
    chats: GeteilteChats,
    befehle: UnboundedSender<Befehl>,
) {
    let mut server = None;
    for p in ports {
        match tiny_http::Server::http(("127.0.0.1", *p)) {
            Ok(s) => {
                println!("🌐 lausche auf 127.0.0.1:{p}");
                server = Some(s);
                break;
            }
            Err(e) => eprintln!("⚠ Port {p} nicht zu belegen ({e})"),
        }
    }
    let Some(server) = server else {
        eprintln!("kein freier Port - laeuft schon eine Instanz?");
        std::process::exit(1);
    };

    std::thread::spawn(move || {
        for anfrage in server.incoming_requests() {
            let pfad = anfrage.url().split('?').next().unwrap_or("").to_string();
            // /pair/qr liefert ein Bild, alles andere JSON.
            if pfad == "/pair/qr" {
                let adresse = lage.lock().ok().map(|l| l.pair_url.clone())
                    .unwrap_or_default();
                match (adresse.is_empty(), qr_bild(&adresse)) {
                    (false, Some(png)) => {
                        let antwort = tiny_http::Response::from_data(png)
                            .with_header(
                                tiny_http::Header::from_bytes(
                                    &b"Content-Type"[..], &b"image/png"[..],
                                )
                                .unwrap(),
                            );
                        let _ = anfrage.respond(antwort);
                    }
                    _ => {
                        let _ = anfrage.respond(
                            tiny_http::Response::from_string(
                                r#"{"error":"keine Adresse"}"#)
                                .with_status_code(404));
                    }
                }
                continue;
            }
            let abfrage = anfrage.url().split_once('?').map(|(_, q)| q.to_string())
                .unwrap_or_default();
            let (code, koerper) = beantworten(&pfad, &abfrage, &lage, &chats, &befehle);
            let antwort = tiny_http::Response::from_string(koerper)
                .with_header(
                    tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/json"[..],
                    )
                    .unwrap(),
                )
                .with_status_code(code);
            let _ = anfrage.respond(antwort);
        }
    });

    // Ungenutzt, aber dokumentiert die Absicht: der Server laeuft, solange
    // der Prozess laeuft. Beendet wird ueber /quit.
    let _ = std_mpsc::channel::<()>();
}

/// Holt einen Wert aus der Abfragezeichenkette, prozentkodierung
/// aufgeloest. Ein eigener kleiner Leser statt einer Kiste: es geht um
/// zwei Parameter, und beim WhatsApp-Port war doppelte Kodierung schon
/// einmal die Ursache dafuer, dass statt eines Kommas "%2C" ankam.
fn parameter(abfrage: &str, name: &str) -> String {
    for paar in abfrage.split('&') {
        let Some((k, v)) = paar.split_once('=') else { continue };
        if k != name {
            continue;
        }
        let bytes = v.as_bytes();
        let mut aus: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => {
                    aus.push(b' ');
                    i += 1;
                }
                b'%' if i + 2 < bytes.len() => {
                    let h = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                    match u8::from_str_radix(h, 16) {
                        Ok(b) => {
                            aus.push(b);
                            i += 3;
                        }
                        Err(_) => {
                            aus.push(bytes[i]);
                            i += 1;
                        }
                    }
                }
                c => {
                    aus.push(c);
                    i += 1;
                }
            }
        }
        return String::from_utf8_lossy(&aus).into_owned();
    }
    String::new()
}

fn beantworten(
    pfad: &str,
    abfrage: &str,
    lage: &GeteilteLage,
    chats: &GeteilteChats,
    befehle: &UnboundedSender<Befehl>,
) -> (u16, String) {
    match pfad {
        "/chats" => match chats.lock() {
            Ok(c) => (200, serde_json::to_string(&c.chats()).unwrap_or("[]".into())),
            Err(_) => (500, r#"{"error":"Chats gesperrt"}"#.into()),
        },
        "/messages" => {
            let jid = parameter(abfrage, "jid");
            match chats.lock() {
                Ok(c) => (
                    200,
                    serde_json::to_string(&c.verlauf(&jid)).unwrap_or("[]".into()),
                ),
                Err(_) => (500, r#"{"error":"Chats gesperrt"}"#.into()),
            }
        }
        "/events" => {
            // Die Oberflaeche fragt mit der zuletzt gesehenen Nummer und
            // bekommt die aktuelle zurueck. Anders als beim
            // WhatsApp-Backend wird hier nicht gewartet, sondern sofort
            // geantwortet: eine lange Abfrage braucht einen Weckruf von
            // der Signal-Seite, und die liegt hinter der Fadengrenze.
            // Fuer den Anfang genuegt das -- die Oberflaeche fragt ohnehin
            // im Takt.
            match chats.lock() {
                Ok(c) => (200, format!(r#"{{"seq":{}}}"#, c.folge)),
                Err(_) => (500, r#"{"error":"Chats gesperrt"}"#.into()),
            }
        }
        "/send" => {
            let an = parameter(abfrage, "to");
            let text = parameter(abfrage, "text");
            if an.is_empty() || text.is_empty() {
                return (400, r#"{"error":"to und text noetig"}"#.into());
            }
            if befehle.send(Befehl::Senden { an, text }).is_err() {
                return (500, r#"{"error":"Signal-Seite antwortet nicht"}"#.into());
            }
            (200, r#"{"ok":true}"#.into())
        }
        "/status" => match lage.lock() {
            Ok(l) => (200, serde_json::to_string(&*l).unwrap_or_else(|_| "{}".into())),
            Err(_) => (500, r#"{"error":"Zustand gesperrt"}"#.into()),
        },
        "/pair" => {
            {
                let Ok(mut l) = lage.lock() else {
                    return (500, r#"{"error":"Zustand gesperrt"}"#.into());
                };
                if l.paired {
                    return (409, r#"{"error":"schon verknuepft"}"#.into());
                }
                l.state = "pairing".into();
            }
            // Das Verknuepfen laeuft weiter, nachdem diese Antwort heraus
            // ist: die Adresse kommt Sekunden spaeter und wird ueber
            // /status abgeholt.
            if befehle.send(Befehl::Verknuepfen).is_err() {
                return (500, r#"{"error":"Signal-Seite antwortet nicht"}"#.into());
            }
            (200, r#"{"ok":true}"#.into())
        }
        "/quit" => {
            println!("👋 Beenden angefordert");
            std::process::exit(0);
        }
        _ => (404, r#"{"error":"unbekannt"}"#.into()),
    }
}

/// Malt die Verknuepfungsadresse als QR-Code.
///
/// Whisperfish auf dem Haupttelefon erwartet einen Code zum Abscannen,
/// nicht eine Zeichenkette zum Einfuegen. Das N950 hat keine Kamera-
/// Anbindung, um selbst zu scannen -- aber einen Bildschirm, um zu zeigen.
/// Also malt es, und das Haupttelefon schaut hin.
fn qr_bild(text: &str) -> Option<Vec<u8>> {
    use image::{ImageEncoder, ExtendedColorType};
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    // Vier Module Rand sind Vorschrift, sonst findet mancher Scanner den
    // Code nicht; die Modulgroesse ist so gewaehlt, dass das Bild auf die
    // 480 Punkte Breite des Geraets passt.
    let bild = code
        .render::<image::Luma<u8>>()
        .min_dimensions(420, 420)
        .quiet_zone(true)
        .build();
    let mut aus = Vec::new();
    image::codecs::png::PngEncoder::new(&mut aus)
        .write_image(bild.as_raw(), bild.width(), bild.height(),
                     ExtendedColorType::L8)
        .ok()?;
    Some(aus)
}

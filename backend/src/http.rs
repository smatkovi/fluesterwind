//! Die HTTP-Seite.
//!
//! Sie laeuft auf einem eigenen Betriebssystemfaden: tiny_http blockiert
//! beim Warten auf Anfragen, und der Faden, auf dem die Signal-Verbindung
//! liegt, darf nicht blockieren. Hinueber geht nur, was `Send` ist -- der
//! geteilte Zustand und ein Befehlskanal.

use std::sync::mpsc as std_mpsc;

use tokio::sync::mpsc::UnboundedSender;

use crate::{Befehl, GeteilteLage};

pub fn starten(ports: &[u16], lage: GeteilteLage, befehle: UnboundedSender<Befehl>) {
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
            let (code, koerper) = beantworten(&pfad, &lage, &befehle);
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

fn beantworten(
    pfad: &str,
    lage: &GeteilteLage,
    befehle: &UnboundedSender<Befehl>,
) -> (u16, String) {
    match pfad {
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

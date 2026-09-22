//! Anhaenge: herunterladen und verschicken.
//!
//! Wohin heruntergeladen wird, ist auf Harmattan keine Geschmacksfrage.
//! Der Indexdienst (tracker) durchsucht ausschliesslich MyDocs; was
//! daneben liegt, taucht in der Galerie und in der Dokumente-App nie auf.
//! Beim WhatsApp-Port landeten Dateien zuerst unter ~/Documents und waren
//! damit unsichtbar -- derselbe Fehler soll sich hier nicht wiederholen.

use std::path::{Path, PathBuf};

use presage::libsignal_service::proto::AttachmentPointer;

fn heim() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/home/user".into()))
}

/// Das Verzeichnis, in das ein Anhang dieses Typs gehoert.
pub fn zielordner(mime: &str) -> PathBuf {
    let wurzel = heim().join("MyDocs");
    let unter = if mime.starts_with("image/") {
        "Pictures/Fluesterwind"
    } else if mime.starts_with("video/") {
        "Videos/Fluesterwind"
    } else if mime.starts_with("audio/") {
        "Music/Fluesterwind"
    } else {
        "Documents/Fluesterwind"
    };
    wurzel.join(unter)
}

/// Die grobe Art eines Anhangs, wie die Oberflaeche sie braucht.
pub fn art(mime: &str) -> &'static str {
    if mime.starts_with("image/") {
        "image"
    } else if mime.starts_with("video/") {
        "video"
    } else if mime.starts_with("audio/") {
        "audio"
    } else {
        "document"
    }
}

/// Eine Endung zum MIME-Typ.
///
/// Der Typ kann Parameter tragen ("audio/ogg; codecs=opus"); die gehoeren
/// abgeschnitten, sonst kommt hinten ".bin" heraus. Genau das passierte im
/// WhatsApp-Port mit Sprachnachrichten.
pub fn endung(mime: &str) -> String {
    let rein = mime.split(';').next().unwrap_or("").trim().to_lowercase();
    let bekannt = [
        ("image/jpeg", "jpg"),
        ("image/png", "png"),
        ("image/gif", "gif"),
        ("image/webp", "webp"),
        ("video/mp4", "mp4"),
        ("video/3gpp", "3gp"),
        ("audio/mpeg", "mp3"),
        ("audio/aac", "aac"),
        ("audio/ogg", "ogg"),
        ("audio/wav", "wav"),
        ("application/pdf", "pdf"),
        ("text/plain", "txt"),
    ];
    for (m, e) in bekannt {
        if rein == m {
            return e.to_string();
        }
    }
    // Sonst der Untertyp, sofern er wie eine Endung aussieht.
    match rein.split('/').nth(1) {
        Some(u) if !u.is_empty() && u.chars().all(|c| c.is_ascii_alphanumeric()) => {
            u.to_string()
        }
        _ => "bin".to_string(),
    }
}

/// Ein Dateiname, der auf VFAT zulaessig ist.
///
/// MyDocs ist VFAT: Doppelpunkte, Fragezeichen und Schraegstriche
/// scheitern dort beim Anlegen, nicht erst beim Lesen.
pub fn sauberer_name(vorschlag: &str, mime: &str, zeit: u64) -> String {
    let mut n: String = vorschlag
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if (c as u32) < 32 => '_',
            c => c,
        })
        .collect();
    n = n.trim().to_string();
    if n.is_empty() {
        n = format!("signal-{zeit}.{}", endung(mime));
    } else if !n.contains('.') {
        n = format!("{n}.{}", endung(mime));
    }
    // VFAT kann lange Namen, aber uferlos wird es unhandlich.
    if n.len() > 120 {
        n.truncate(120);
    }
    n
}

/// Name und Art eines Anhangs, wie sie in die Nachricht gehoeren.
pub fn beschreibung(p: &AttachmentPointer) -> (String, String, u64) {
    let mime = p.content_type.clone().unwrap_or_default();
    let name = p.file_name.clone().unwrap_or_default();
    let groesse = p.size.unwrap_or(0) as u64;
    (mime, name, groesse)
}

/// Legt die Datei ab und gibt ihren Pfad zurueck.
pub fn ablegen(
    daten: &[u8],
    mime: &str,
    vorschlag: &str,
    zeit: u64,
) -> Result<PathBuf, String> {
    let ordner = zielordner(mime);
    std::fs::create_dir_all(&ordner).map_err(|e| e.to_string())?;
    let mut pfad = ordner.join(sauberer_name(vorschlag, mime, zeit));
    // Nicht ueberschreiben: zwei Bilder gleichen Namens sind haeufiger,
    // als man denkt.
    if pfad.exists() {
        let stamm = pfad
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let end = pfad
            .extension()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "bin".into());
        pfad = ordner.join(format!("{stamm}-{zeit}.{end}"));
    }
    std::fs::write(&pfad, daten).map_err(|e| e.to_string())?;
    Ok(pfad)
}

/// MIME-Typ einer zu verschickenden Datei, aus ihrer Endung geraten.
pub fn mime_aus_pfad(pfad: &Path) -> String {
    let e = pfad
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match e.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "3gp" => "video/3gpp",
        "mp3" => "audio/mpeg",
        "aac" | "m4a" => "audio/aac",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Wohin Profilbilder kommen.
///
/// Nicht nach MyDocs: die Galerie soll sich nicht mit hunderten
/// Kontaktbildern fuellen. Das Datenverzeichnis reicht -- nur die App
/// selbst liest sie.
pub fn avatar_pfad(jid: &str) -> PathBuf {
    let sicher: String = jid
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    heim()
        .join(".local/share/harbour/fluesterwind/avatare")
        .join(format!("{sicher}.jpg"))
}

pub fn avatar_ablegen(jid: &str, daten: &[u8]) -> Result<PathBuf, String> {
    let pfad = avatar_pfad(jid);
    if let Some(ordner) = pfad.parent() {
        std::fs::create_dir_all(ordner).map_err(|e| e.to_string())?;
    }
    std::fs::write(&pfad, daten).map_err(|e| e.to_string())?;
    Ok(pfad)
}

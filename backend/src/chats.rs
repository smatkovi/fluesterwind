//! Der Schnappschuss, den die Oberflaeche zu sehen bekommt.
//!
//! presage lebt auf einem einzigen Faden und ist nicht `Send`; der
//! HTTP-Server laeuft auf einem anderen. Zwischen beiden liegt deshalb
//! kein Zugriff auf presage, sondern dieser Schnappschuss: schlichte
//! Datensaetze hinter einem Mutex, die die Signal-Seite fortschreibt und
//! die HTTP-Seite ausliest.
//!
//! Die Feldnamen sind die des WhatsApp-Backends -- jid, name, isGroup,
//! lastMessage, lastTime; chatJid, sender, text, fromMe, timestamp. Nicht
//! aus Bequemlichkeit, sondern damit dieselbe Qt-Oberflaeche beide
//! bedienen kann.

use std::collections::HashMap;

use serde::Serialize;

/// Wie eine Unterhaltung nach aussen heisst.
///
/// Bei WhatsApp entschied die Laenge der Kennung, ob es eine Gruppe ist --
/// und das ging schief, als Signal-ferne Kennungen (LIDs) genau an der
/// Grenze lagen. Hier steht es vorne dran und ist nicht zu verwechseln:
/// "c:" fuer eine Person, "g:" fuer eine Gruppe.
pub fn kennung_person(service_id: &str) -> String {
    format!("c:{service_id}")
}

pub fn kennung_gruppe(master_key: &[u8]) -> String {
    let mut s = String::with_capacity(2 + master_key.len() * 2);
    s.push_str("g:");
    for b in master_key {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn ist_gruppe(kennung: &str) -> bool {
    kennung.starts_with("g:")
}

/// Der Hauptschluessel einer Gruppe aus ihrer Kennung.
pub fn gruppenschluessel(kennung: &str) -> Option<Vec<u8>> {
    let hex = kennung.strip_prefix("g:")?;
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

#[derive(Serialize, Clone)]
pub struct Chat {
    pub jid: String,
    pub name: String,
    #[serde(rename = "isGroup")]
    pub is_group: bool,
    #[serde(rename = "lastMessage")]
    pub last_message: String,
    #[serde(rename = "lastTime")]
    pub last_time: u64,
    #[serde(rename = "fromMe")]
    pub from_me: bool,
}

#[derive(Serialize, Clone)]
pub struct Nachricht {
    pub id: String,
    #[serde(rename = "chatJid")]
    pub chat_jid: String,
    pub sender: String,
    pub text: String,
    #[serde(rename = "fromMe")]
    pub from_me: bool,
    pub timestamp: u64,
    /// "image", "video", "audio", "document" -- leer, wenn kein Anhang.
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    pub size: u64,
    /// Gesetzt, sobald der Anhang auf dem Geraet liegt. Auf 2G will man
    /// nicht jeden Anhang eines Verlaufs ungefragt holen.
    #[serde(rename = "localPath")]
    pub local_path: String,
}

impl Nachricht {
    /// Ohne Anhang -- der haeufige Fall.
    pub fn text(id: String, chat_jid: String, sender: String, text: String,
                from_me: bool, timestamp: u64) -> Self {
        Nachricht {
            id, chat_jid, sender, text, from_me, timestamp,
            media_type: String::new(),
            file_name: String::new(),
            size: 0,
            local_path: String::new(),
        }
    }
}

#[derive(Default)]
pub struct Schnappschuss {
    chats: HashMap<String, Chat>,
    nachrichten: HashMap<String, Vec<Nachricht>>,
    /// Kennung -> Name, aus den Kontakten. In einer Gruppe steht sonst
    /// eine rohe UUID ueber der Nachricht statt eines Namens.
    namen: HashMap<String, String>,
    /// Zaehlt jede Aenderung. Die Oberflaeche haengt daran statt zu pollen
    /// -- dasselbe Verfahren wie beim WhatsApp-Backend.
    pub folge: u64,
}

impl Schnappschuss {
    /// Traegt eine Nachricht ein und schreibt den Chat fort.
    pub fn eintragen(&mut self, n: Nachricht, chatname: &str) {
        // Den Absender erst hier aufloesen: die Namenskarte fuellt sich
        // asynchron aus den Kontakten, und der Bauer der Nachricht kennt
        // sie nicht.
        let mut n = n;
        if !n.sender.is_empty() {
            n.sender = self.name_zu(&n.sender);
        }
        let chat = self.chats.entry(n.chat_jid.clone()).or_insert_with(|| Chat {
            jid: n.chat_jid.clone(),
            name: chatname.to_string(),
            is_group: ist_gruppe(&n.chat_jid),
            last_message: String::new(),
            last_time: 0,
            from_me: false,
        });
        if !chatname.is_empty() {
            chat.name = chatname.to_string();
        }
        // Nur fortschreiben, wenn die Nachricht wirklich neuer ist: beim
        // Nachholen alter Nachrichten waere es sonst die aelteste, die am
        // Ende in der Chatliste steht.
        if n.timestamp >= chat.last_time {
            chat.last_time = n.timestamp;
            chat.last_message = n.text.clone();
            chat.from_me = n.from_me;
        }

        let verlauf = self.nachrichten.entry(n.chat_jid.clone()).or_default();
        // Doppelte abweisen: derselbe Zeitstempel ist bei Signal die
        // Kennung einer Nachricht. Ein bereits heruntergeladener Anhang
        // darf dabei nicht verlorengehen -- der Speicher weiss nichts
        // davon, wohin wir die Datei gelegt haben.
        if let Some(vorhanden) = verlauf.iter_mut().find(|m| m.id == n.id) {
            if vorhanden.local_path.is_empty() && !n.local_path.is_empty() {
                vorhanden.local_path = n.local_path;
                self.folge += 1;
            }
            return;
        }
        verlauf.push(n);
        verlauf.sort_by_key(|m| m.timestamp);
        self.folge += 1;
    }

    /// Vermerkt, wo ein heruntergeladener Anhang liegt.
    pub fn pfad_setzen(&mut self, jid: &str, id: &str, pfad: &str) {
        if let Some(v) = self.nachrichten.get_mut(jid) {
            if let Some(m) = v.iter_mut().find(|m| m.id == id) {
                m.local_path = pfad.to_string();
                self.folge += 1;
            }
        }
    }

    pub fn namen_setzen(&mut self, namen: HashMap<String, String>) {
        self.namen = namen;
    }

    /// Der Anzeigename zu einer Kennung, oder die Kennung selbst.
    pub fn name_zu(&self, kennung: &str) -> String {
        self.namen
            .get(kennung)
            .cloned()
            .unwrap_or_else(|| kennung.to_string())
    }

    pub fn chatname_setzen(&mut self, jid: &str, name: &str) {
        if let Some(c) = self.chats.get_mut(jid) {
            if c.name != name {
                c.name = name.to_string();
                self.folge += 1;
            }
        }
    }

    /// Die Chatliste, neueste zuerst.
    pub fn chats(&self) -> Vec<Chat> {
        let mut v: Vec<Chat> = self.chats.values().cloned().collect();
        v.sort_by(|a, b| b.last_time.cmp(&a.last_time));
        v
    }

    pub fn verlauf(&self, jid: &str) -> Vec<Nachricht> {
        self.nachrichten.get(jid).cloned().unwrap_or_default()
    }
}

impl Schnappschuss {
    /// Legt einen Chat an, auch ohne Nachricht darin.
    ///
    /// Ein Kontakt, mit dem man noch nie geschrieben hat, taucht sonst
    /// nirgends auf -- und laesst sich dann auch nicht anschreiben.
    pub fn anlegen(&mut self, jid: &str, name: &str) {
        if self.chats.contains_key(jid) {
            return;
        }
        if name.is_empty() {
            return;
        }
        self.chats.insert(
            jid.to_string(),
            Chat {
                jid: jid.to_string(),
                name: name.to_string(),
                is_group: ist_gruppe(jid),
                last_message: String::new(),
                last_time: 0,
                from_me: false,
            },
        );
        self.folge += 1;
    }
}

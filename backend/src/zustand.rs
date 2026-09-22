//! Was die Oberflaeche ueber /status erfaehrt.

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Lage {
    /// Verknuepft, also ein Zweitgeraet dieses Kontos.
    pub paired: bool,
    /// Verbindung zum Server steht.
    pub connected: bool,
    /// Die Adresse zum Abscannen, solange das Verknuepfen laeuft.
    #[serde(rename = "pairUrl")]
    pub pair_url: String,
    /// Eigene Nummer, wenn bekannt.
    pub phone: String,
    #[serde(rename = "lastError")]
    pub last_error: String,
    pub state: String,
}

impl Lage {
    pub fn neu() -> Self {
        Lage {
            paired: false,
            connected: false,
            pair_url: String::new(),
            phone: String::new(),
            last_error: String::new(),
            state: "idle".into(),
        }
    }

    pub fn adresse_setzen(&mut self, adresse: String) {
        self.pair_url = adresse;
        self.state = "pairing".into();
        self.last_error.clear();
    }

    /// `nummer` ist die eigene Rufnummer in E.164, sofern der Server sie
    /// schon nennt -- direkt nach dem Verknuepfen kann sie fehlen.
    pub fn verknuepft(&mut self, nummer: Option<String>) {
        self.paired = true;
        self.state = "connected".into();
        // Ist das Verknuepfen durch, taugt die Adresse nichts mehr -- sie
        // stehen zu lassen hiesse, der Oberflaeche einen abgelaufenen Code
        // anzubieten.
        self.pair_url.clear();
        self.last_error.clear();
        if let Some(n) = nummer {
            self.phone = n;
        }
    }

    pub fn verbunden_setzen(&mut self, an: bool) {
        self.connected = an;
        if an {
            self.state = "connected".into();
        }
    }

    pub fn fehler_setzen(&mut self, text: String) {
        self.last_error = text;
        self.state = "error".into();
    }
}

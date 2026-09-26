# Fluesterwind

Ein Signal-Client für MeeGo Harmattan — das Nokia N9 und N950.

Der Name ist eine Verbeugung vor
[Whisperfish](https://gitlab.com/whisperfish/whisperfish), dem
Signal-Client für Sailfish: dort „whisper" plus Sailfish, hier „fluestern"
plus Harmattan, den Wind, nach dem das Betriebssystem benannt ist.

## Warum kein Port von Whisperfish

Whisperfishs Oberfläche ist Silica-QML, von Rust über `qmetaobject-rs`
gegen **Qt 5** getrieben. Harmattan hat Qt 4.7 und kein Silica. Übernommen
wird deshalb nicht die Anwendung, sondern ihr Unterbau: presage und
libsignal-service, beides Rust.

Der Aufbau entspricht dem des
[WhatsApp-Ports](https://github.com/smatkovi/harbour-whatsapp-meego) für
dasselbe Gerät, und zwar absichtlich:

    ┌────────────────────────────┐
    │ Oberfläche (Qt 4.7, QML 1) │  com.nokia.meego
    └─────────────┬──────────────┘
                  │ HTTP auf 127.0.0.1
    ┌─────────────┴──────────────┐
    │ Dienst (Rust, presage)     │  statisch gegen musl
    └────────────────────────────┘

Dieselbe HTTP-Schnittstelle wie dort bedeutet, dass Oberfläche,
Dateiwähler, Medienbehandlung und die Anbindung an die Nachrichten-App
weitgehend übernommen werden können.

## Anmelden

Signal kennt kein Verknüpfen per Nummerncode wie WhatsApp — es gibt nur
den Weg über ein Hauptgerät, das ein Zweitgerät aufnimmt.

Das N950 hat keine Kamera-Anbindung, um einen Code zu scannen, aber einen
Bildschirm, um einen zu zeigen. Also erzeugt der Dienst die
`sgnl://linkdevice?…`-Adresse und malt sie unter `/pair/qr` als QR-Code;
das Hauptgerät scannt ihn.

Als Hauptgerät taugt Signal für Android oder iOS ebenso wie ein
Whisperfish, das selbst als Hauptgerät registriert ist — dort unter
Einstellungen → „Linked devices" → Gerät hinzufügen.

## Bauen

    . tools/cross.env
    cargo build --release --target $ZIEL --manifest-path backend/Cargo.toml

Was dabei nicht offensichtlich ist, steht in `tools/cross.env`: warum musl
statt glibc, warum `rust-lld` statt des Systemlinkers, und warum clang
allein als Cross-Compiler nicht genügt.

## Stand

Läuft auf N9 und N950, Fassung 1.3. Gekoppelt, verbunden, 97 Chats.

* Oberfläche (Qt 4.7 / QML 1): Chatliste, Verlauf, Senden, Anhänge,
  Gruppen mit Mitgliedsnamen, Reaktionen, Antwortbezüge, Profilbilder.
* Kopplung per QR-Code unter `/pair/qr`, Abmelden über `/logout`.
* Anbindung an die Nachrichten-App über die Brücke: das Konto
  `bruecke/signal/signal0` spricht HTTP auf `127.0.0.1:8095`.

**Der Dienst ersetzt seit 26.09.2026 `/opt/pysignal/signal_daemon.py`.**
Der lag daneben noch im Speicher (3,8 MB, Elternprozess init) und hörte
auf gar nichts mehr — Port 8095 gehörte längst hier her. Die
Python-Vorlage liegt zum Nachschlagen unter `alt/pysignal/`.

Offen: der Gerätename beim Koppeln ist fest „Fluesterwind (N9)", auch
auf dem N950 — in der Geräteliste von Signal sehen beide gleich aus.

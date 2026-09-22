#!/usr/bin/python
# -*- coding: utf-8 -*-
"""Traegt Signal in den vorhandenen pybridge-Verbindungsmanager ein.

pybridge gehoert einem anderen Paket. Statt es zu ersetzen, wird es an
wenigen Stellen erweitert -- der Manager verzweigt nach Protokoll, und
unser Daemon spricht bewusst die Datenform des Telegram-Daemons. Aus jedem
Telegram-Zweig wird ein Zweig, der auch "signal" annimmt.

Der Unterschied zum gleichnamigen Skript des WhatsApp-Ports: dort war die
Datei im Auslieferungszustand, hier nicht unbedingt. Ist WhatsApp schon
eingetragen, steht an den Stellen bereits

    if protocol in ('telegram', 'whatsapp'):

statt

    if protocol == 'telegram':

Beide Formen muessen behandelt werden, und in beiden Anfuehrungsarten --
der WhatsApp-Eingriff hat an zwei Stellen doppelte uebernommen. Deshalb
wird hier mit Mustern gearbeitet statt mit festem Text, dafuer aber die
Zahl der Treffer geprueft: kommen nicht genau so viele vor wie erwartet,
bricht das Skript ab und ruehrt nichts an.

Laeuft unter Python 2.6 (dem Bordmittel dieses Geraets) wie unter 3.
"""

import io
import os
import re
import shutil
import sys

CM = '/opt/pybridge/pybridge_cm.py'
MANAGER = '/usr/share/telepathy/managers/pybridge.manager'

MARKE = "'signal': {"

# So viele Protokollverzweigungen erwartet dieses Skript. Weicht die Zahl
# ab, hat sich pybridge geaendert -- dann lieber gar nichts tun.
#
# Acht, nicht neun: der WhatsApp-Eingriff zaehlt neun Stellen, aber eine
# davon ist der Eintrag in BACKENDS und keine Verzweigung.
ERWARTETE_ZWEIGE = 8


def zweige_ersetzen(text):
    """Nimmt "signal" in jede Telegram-Verzweigung auf.

    Gibt (neuer Text, Anzahl) zurueck.
    """
    anzahl = [0]

    def ersetze(m):
        anzahl[0] += 1
        vorher = m.group('vor')          # "protocol" oder "self._protocol"
        q = m.group('q') or "'"          # verwendete Anfuehrungsart
        liste = m.group('liste')
        if liste is None:
            # == 'telegram'  ->  in ('telegram', 'signal')
            return "%s in (%stelegram%s, %ssignal%s)" % (vorher, q, q, q, q)
        # in ('telegram', ...)  ->  ... mit 'signal' dahinter
        if 'signal' in liste:
            return m.group(0)
        return "%s in (%s, %ssignal%s)" % (vorher, liste.rstrip(), q, q)

    muster = re.compile(
        r"(?P<vor>self\._protocol|protocol)\s*"
        r"(?:==\s*(?P<q>['\"])telegram(?P=q)"
        r"|in\s*\((?P<liste>[^)]*['\"]telegram['\"][^)]*)\))"
    )
    return muster.sub(ersetze, text), anzahl[0]


ALT_ZUSTELLUNG = '        ch = self._get_or_create_channel(chat_handle)\n        log("DELIVERING to ch=%s h=%s" % (ch._path, sender_handle))\n        ch.receive_message(sender_handle, text, timestamp)'

NEU_ZUSTELLUNG = '        ch = self._get_or_create_channel(chat_handle)\n        log("DELIVERING to ch=%s h=%s" % (ch._path, sender_handle))\n        ch.receive_message(sender_handle, text, timestamp)\n        # Auf einem BESTEHENDEN Kanal erfaehrt CommHistory von einer neuen\n        # Nachricht nur, solange es ihn noch beobachtet. Schliesst man die\n        # Unterhaltung in der Nachrichten-App, hoert das auf -- und danach\n        # stapeln sich die Nachrichten unsichtbar als "pending".\n        #\n        # Gemeldet wird nur beobachtend: die Unterhaltung soll im Verlauf\n        # auftauchen, sich aber nicht von selbst oeffnen.\n        #\n        # Und nur, wenn es noetig ist. Jede Meldung startet einen eigenen\n        # Python-Prozess, und auf diesem Geraet kostet das spuerbar: beim\n        # Nachholen vieler Nachrichten stieg die Last auf ueber sieben.\n        # Noetig ist es, wenn schon etwas Unbestaetigtes im Kanal liegt --\n        # dann hoert offenbar niemand mehr zu. Und einmal je Minute\n        # ohnehin, damit auch die erste verpasste Nachricht ankommt und\n        # nicht erst die zweite sie mitzieht.\n        _jetzt = time.time()\n        _offen = len(ch._pending) > 1\n        _lange_her = _jetzt - getattr(ch, "_zuletzt_gemeldet", 0) > 60\n        if _offen or _lange_her:\n            ch._zuletzt_gemeldet = _jetzt\n            gobject.idle_add(\n                lambda p=ch._path, c=ch: self._dispatch_channel(p, c, True) or False)'

ERSETZUNGEN = [

    # Der Dienst selbst. Haengt hinter dem letzten Eintrag von BACKENDS --
    # welcher das ist, haengt davon ab, ob WhatsApp schon drin steht.
    ("    }\n}\n\nLOG_FILE",
     """    },
    'signal': {
        'daemon_script': '/opt/pysignal/signal_daemon.py',
        'socket_path': os.path.expanduser('~/.pysignal/daemon.sock'),
        'data_dir': os.path.expanduser('~/.pysignal'),
    }
}

LOG_FILE"""),

    # Der Rueckfallpfad zum Konto. Ohne eigenen Zweig landeten
    # Signal-Nachrichten im Verlauf unter einem fremden Konto, sobald die
    # Abfrage beim Kontoverwalter einmal scheitert.
    #
    # Der Signal-Zweig kommt VOR den Telegram-Zweig, nicht dahinter. Die
    # Musterersetzung nimmt "signal" gleich darauf auch in den
    # Telegram-Zweig auf -- stuende unserer dahinter, waere er
    # unerreichbar, und Signal bekaeme den Telegram-Kontopfad.
    ("""        if self._protocol == 'telegram':
            return '/org/freedesktop/Telepathy/Account/pybridge/telegram/Telegram0'""",
     """        if self._protocol == 'signal':
            return '/org/freedesktop/Telepathy/Account/pybridge/signal/Signal0'
        if self._protocol == 'telegram':
            return '/org/freedesktop/Telepathy/Account/pybridge/telegram/Telegram0'"""),
]


PROTOKOLL_EINTRAG = """
[Protocol signal]
param-account=s required
ConnectionInterfaces=org.freedesktop.Telepathy.Connection.Interface.Requests;org.freedesktop.Telepathy.Connection.Interface.SimplePresence;org.freedesktop.Telepathy.Connection.Interface.Contacts;
RequestableChannelClasses=org.freedesktop.Telepathy.Channel.Type.Text
VCardField=x-signal
EnglishName=Signal
Icon=icon-m-service-signal
"""



def zustellung_ergaenzen(text):
    """Meldet einen bestehenden Kanal erneut, wenn eine Nachricht kommt.

    Diese Ergaenzung gilt fuer ALLE Protokolle -- Telegram, WhatsApp und
    Signal teilen sich die Stelle. Sie wird deshalb auch dann angewandt,
    wenn das eigene Protokoll schon eingetragen ist; sonst kaeme sie nie
    zur Anwendung, weil das Skript vorher abbricht.

    Gibt (Text, ob geaendert) zurueck.
    """
    if "_lange_her" in text:
        return text, False
    if "_zuletzt_gemeldet" in text:
        # Eine aeltere Fassung steht schon drin: sie meldete bei jeder
        # Nachricht und trieb die Last hoch. Ersetzen statt ueberspringen.
        anfang = text.find("        # Auf einem BESTEHENDEN Kanal")
        ende = text.find("or False)", anfang)
        if anfang < 0 or ende < 0:
            sys.stderr.write("pybridge: alte Fassung nicht ersetzbar\n")
            return text, False
        ende += len("or False)")
        marke = "        # Auf einem BESTEHENDEN Kanal"
        return text[:anfang] + NEU_ZUSTELLUNG[NEU_ZUSTELLUNG.find(marke):] + text[ende:], True
    if text.count(ALT_ZUSTELLUNG) != 1:
        sys.stderr.write(
            "pybridge: Zustellstelle nicht eindeutig gefunden - "
            "bleibt unveraendert\n")
        return text, False
    return text.replace(ALT_ZUSTELLUNG, NEU_ZUSTELLUNG, 1), True


def lies(pfad):
    f = io.open(pfad, encoding='utf-8')
    try:
        return f.read()
    finally:
        f.close()


def schreib(pfad, text):
    # Erst daneben, dann umbenennen: ein halb geschriebener
    # Verbindungsmanager legt auch Telegram, Matrix und WhatsApp lahm.
    tmp = pfad + '.neu'
    f = io.open(tmp, 'w', encoding='utf-8')
    try:
        f.write(text)
    finally:
        f.close()
    os.rename(tmp, pfad)


def sichern(pfad):
    ab = pfad + '.vor-signal'
    if not os.path.exists(ab):
        shutil.copy2(pfad, ab)


def patche_cm():
    if not os.path.exists(CM):
        sys.stderr.write('pybridge nicht gefunden: %s\n' % CM)
        return False
    text = lies(CM)

    # Zuerst die Ergaenzung, die allen Protokollen gilt. Sie muss auch
    # laufen, wenn das eigene Protokoll schon eingetragen ist.
    text, zustellung = zustellung_ergaenzen(text)
    if zustellung:
        sichern(CM)
        schreib(CM, text)
        print('pybridge: Kanaele werden bei neuen Nachrichten erneut gemeldet')

    if MARKE in text:
        print('pybridge: Signal bereits eingetragen')
        return True

    for alt, neu in ERSETZUNGEN:
        anzahl = text.count(alt)
        if anzahl != 1:
            sys.stderr.write(
                'pybridge hat sich geaendert: Stelle kommt %d-mal vor '
                '(erwartet: genau einmal)\n---\n%s\n---\n' % (anzahl, alt[:160]))
            return False

    # Zuerst die festen Ersetzungen -- sie erwarten den Text so, wie er
    # vorliegt. Danach erst die Muster, die daraus "in (...)" machen.
    neu_text = text
    for alt, neu in ERSETZUNGEN:
        neu_text = neu_text.replace(alt, neu, 1)

    neu_text, zweige = zweige_ersetzen(neu_text)
    if zweige != ERWARTETE_ZWEIGE:
        sys.stderr.write(
            'pybridge hat sich geaendert: %d Protokollzweige gefunden, '
            '%d erwartet\n' % (zweige, ERWARTETE_ZWEIGE))
        return False

    sichern(CM)
    schreib(CM, neu_text)
    print('pybridge: Signal eingetragen (%d Zweige)' % zweige)
    return True


def patche_manager():
    if not os.path.exists(MANAGER):
        sys.stderr.write('Manager-Datei fehlt: %s\n' % MANAGER)
        return False
    text = lies(MANAGER)
    if '[Protocol signal]' in text:
        print('pybridge.manager: bereits eingetragen')
        return True
    sichern(MANAGER)
    # Nicht blind anhaengen: die Datei auf diesem Geraet endete einmal mit
    # einer verirrten Heredoc-Marke. Was dahinter steht, sieht ein Parser,
    # der an dieser Zeile abbricht, nie.
    zeilen = text.split('\n')
    letzte = -1
    for i, z in enumerate(zeilen):
        k = z.strip()
        if k.startswith('[') or ('=' in k and not k.startswith('#')):
            letzte = i
    if letzte < 0:
        sys.stderr.write('Manager-Datei unverstaendlich: %s\n' % MANAGER)
        return False
    neu = (zeilen[:letzte + 1]
           + PROTOKOLL_EINTRAG.rstrip('\n').split('\n')
           + zeilen[letzte + 1:])
    schreib(MANAGER, '\n'.join(neu))
    print('pybridge.manager: Signal eingetragen')
    return True


def main():
    # Erst der Verbindungsmanager. Scheitert er, bleibt auch die
    # Manager-Datei unberuehrt: ein eingetragenes Protokoll, das der
    # Manager nicht kennt, waere schlimmer als gar keines.
    if not patche_cm():
        return 1
    return 0 if patche_manager() else 1


if __name__ == '__main__':
    sys.exit(main())

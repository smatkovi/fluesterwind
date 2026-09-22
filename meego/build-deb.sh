#!/bin/sh
# Packt Oberflaeche, Dienst und QML als Harmattan-.deb.
#
#   meego/build-deb.sh [version]
#
# Keine versionierten Abhaengigkeiten: "libqt4-gui (>= 4.7.4)" sieht
# harmlos aus und ist es nicht -- das installierte Qt ist
# 4.7.4~git20120327, und "~" sortiert in Debian UNTER der leeren
# Zeichenkette. Die Bedingung waere nie erfuellbar.
set -e
cd "$(dirname "$0")/.."

VERSION=${1:-0.1}
STAGE=build/stage
OUT=build
rm -rf "$STAGE"

[ -x "$OUT/fluesterwind" ]   || { echo "$OUT/fluesterwind fehlt -- erst tools/build.sh" >&2; exit 1; }
[ -x "$OUT/signal-backend" ] || { echo "$OUT/signal-backend fehlt -- erst tools/build.sh" >&2; exit 1; }

mkdir -p "$STAGE/opt/fluesterwind/bin" "$STAGE/opt/fluesterwind/qml" \
         "$STAGE/usr/share/applications" \
         "$STAGE/usr/share/icons/hicolor/80x80/apps" \
         "$STAGE/usr/share/themes/base/meegotouch/icons" \
         "$STAGE/DEBIAN"

cp "$OUT/fluesterwind" "$OUT/signal-backend" "$STAGE/opt/fluesterwind/bin/"
chmod 755 "$STAGE/opt/fluesterwind/bin/"*
cp meego/qml/*.qml "$STAGE/opt/fluesterwind/qml/"
cp meego/fluesterwind.desktop "$STAGE/usr/share/applications/"

# --- Hintergrunddienst -----------------------------------------------------
# Der Dienst soll laufen, auch wenn die App zu ist -- sonst kommen
# Nachrichten erst an, wenn man nachsieht. Jobs unter ~/.config/upstart
# liest auf diesem Geraet niemand, und nach /etc/init/xsession/ kommt ein
# unsigniertes Paket nicht (Aegis verweigert dort jede Datei ohne
# Referenz-Hash). Der Sitzungs-D-Bus ist der Weg, der bleibt.
mkdir -p "$STAGE/usr/share/dbus-1/services" "$STAGE/etc/init/apps"
cp meego/org.smatkovi.Fluesterwind.service "$STAGE/usr/share/dbus-1/services/"
cp meego/fluesterwind-trigger.conf "$STAGE/etc/init/apps/fluesterwind.conf"

# --- Nachrichten-App: der pybridge-Anschluss --------------------------------
# pybridge ist der Telepathy-Verbindungsmanager, der auf diesem Geraet
# schon Telegram, Matrix und WhatsApp in die Nachrichten-App traegt. Signal
# haengt sich daran: ein Daemon, der auf der einen Seite pybridges
# Zeilen-JSON spricht und auf der anderen unsere HTTP-Schnittstelle.
mkdir -p "$STAGE/opt/pysignal" \
         "$STAGE/usr/share/accounts/services" \
         "$STAGE/usr/share/accounts/providers" \
         "$STAGE/usr/share/themes/blanco/meegotouch/icons"
cp meego/pybridge/signal_daemon.py   "$STAGE/opt/pysignal/"
cp meego/pybridge/patch-pybridge.py  "$STAGE/opt/pysignal/"
cp meego/pybridge/signal-setup       "$STAGE/opt/pysignal/"
chmod 755 "$STAGE/opt/pysignal/"*.py "$STAGE/opt/pysignal/signal-setup"
cp meego/pybridge/signal.service  "$STAGE/usr/share/accounts/services/"
cp meego/pybridge/signal.provider "$STAGE/usr/share/accounts/providers/"
cp meego/pybridge/icon-m-service-signal.png \
   meego/pybridge/icon-s-service-signal.png \
   "$STAGE/usr/share/themes/blanco/meegotouch/icons/"

cp meego/postinst meego/prerm "$STAGE/DEBIAN/"
chmod 755 "$STAGE/DEBIAN/postinst" "$STAGE/DEBIAN/prerm"

# Das Symbol traegt die exakte Silhouette der Standard-Apps; ein rundes
# faellt im Raster des Startbildschirms sofort auf.
cp meego/icons/icon-80.png "$STAGE/usr/share/icons/hicolor/80x80/apps/fluesterwind.png"
cp meego/icons/icon-80.png "$STAGE/usr/share/themes/base/meegotouch/icons/fluesterwind-80.png"

VERSION="$VERSION" python3 - <<'PY'
import base64, io, os, textwrap
icon = base64.b64encode(open("meego/icons/icon-64.png", "rb").read()).decode("ascii")
text = io.open("meego/control.in", encoding="utf-8").read()
text = text.replace("@VERSION@", os.environ["VERSION"])
text = text.replace("@ICON@",
                    "\n".join(" " + line for line in textwrap.wrap(icon, 76)))
io.open("build/stage/DEBIAN/control", "w", encoding="utf-8").write(text)
PY

DEB="fluesterwind_${VERSION}_armel.deb"
python3 meego/mkdeb.py "$STAGE" "$DEB"

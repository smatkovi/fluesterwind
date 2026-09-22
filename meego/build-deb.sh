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

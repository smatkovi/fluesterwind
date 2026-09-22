#!/bin/sh
# Laeuft auf dem Build-Rechner: Dienst (Rust) und Oberflaeche (Qt 4.7).
set -e
SRC=/tmp/fluesterwind-src
. "$SRC/tools/cross.env"

# Toolchain vorhanden? /tmp ist ein tmpfs und nach einem Neustart leer.
if [ ! -x "$CARGO_HOME/bin/cargo" ] || [ ! -d "$MUSL" ]; then
    sh "$SRC/tools/toolchain.sh"
    . "$SRC/tools/cross.env"
fi

OUT=$SRC/build
mkdir -p "$OUT"

# --- Dienst ---------------------------------------------------------------
cd "$SRC/backend"
cargo build --release --target "$ZIEL"
cp "target/$ZIEL/release/signal-backend" "$OUT/"
echo "== signal-backend fertig ($(stat -c %s "$OUT/signal-backend") B)"

# --- Oberflaeche ----------------------------------------------------------
# Qt 4.7 kommt aus dem MADDE-Sysroot des Harmattan-SDK, uebersetzt wird mit
# dem GCC-14-Cross aus dem Snapszer-Port. Zwei Link-Einstellungen sind
# nicht kosmetisch:
#
#   --dynamic-linker=/lib/ld-linux.so.3  -- sonst verlangt das Binary den
#     armhf-Lader, den Harmattan nicht hat.
#   -static-libstdc++ -static-libgcc mit --exclude-libs,ALL -- die moderne
#     C++-Laufzeit bleibt im Binary, statt sie an Qt zu exportieren, das
#     gegen die von GCC 4.4 gebaut ist.
XGCC=${XGCC:-/tmp/xgcc-harmattan}
SYSROOT=${SYSROOT:-$HOME/QtSDK/Madde/sysroots/harmattan_sysroot_10.2011.34-1_slim}
SIMQT=${SIMQT:-$HOME/QtSDK/Simulator/Qt/gcc}

CXX=$XGCC/bin/arm-none-linux-gnueabi-g++
if [ ! -x "$CXX" ]; then
    echo "== Cross-C++ fehlt ($CXX) - nur der Dienst wurde gebaut" >&2
    exit 0
fi
MOC=$SIMQT/bin/moc
QTINC=$SYSROOT/usr/include/qt4

CXXFLAGS="--sysroot=$SYSROOT -std=gnu++17 -O2 -Wall -Wno-register \
 -Wno-deprecated-declarations -DQT_NO_DEBUG -I$QTINC -I$SRC/meego"
for m in QtCore QtGui QtNetwork QtScript QtDeclarative; do
    CXXFLAGS="$CXXFLAGS -I$QTINC/$m"
done
LDFLAGS="--sysroot=$SYSROOT -static-libstdc++ -static-libgcc -Wl,-O1 \
 -Wl,--as-needed -Wl,--exclude-libs,ALL -Wl,--dynamic-linker=/lib/ld-linux.so.3"
LIBS="-lQtDeclarative -lQtScript -lQtNetwork -lQtGui -lQtCore -lpthread"

cd "$OUT"
$MOC "$SRC/meego/src/Backend.h" -o moc_Backend.cpp
OBJS=""
$CXX $CXXFLAGS -c moc_Backend.cpp -o moc_Backend.o; OBJS="$OBJS moc_Backend.o"
for s in src/Json src/Backend; do
    n=$(basename "$s")
    $CXX $CXXFLAGS -c "$SRC/meego/$s.cpp" -o "$n.o"; OBJS="$OBJS $n.o"
done
$CXX $CXXFLAGS -c "$SRC/meego/main.cpp" -o main.o; OBJS="$OBJS main.o"
$CXX $LDFLAGS -o fluesterwind $OBJS $LIBS
echo "== fluesterwind fertig ($(stat -c %s fluesterwind) B)"

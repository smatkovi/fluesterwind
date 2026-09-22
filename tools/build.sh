#!/bin/sh
# Baut Dienst und Oberflaeche fuer das N9/N950.
#
#   tools/build.sh          # -> build/{signal-backend,fluesterwind}
#
# Gebaut wird auf dem Build-Rechner, nicht hier: dort liegen Rust, die
# musl-Toolchain und der Cargo-Zwischenspeicher. Die Wurzelpartition dort
# ist voll, deshalb alles unter /tmp (tmpfs) -- das ueberlebt keinen
# Neustart, und tools/toolchain.sh legt es bei Bedarf neu an.
set -e
cd "$(dirname "$0")/.."

FERN=/tmp/fluesterwind-src
HOST=$(sh "$HOME/ps/nfsshift-sfos/tools/buildhost.sh")
echo "== Build-Rechner: $HOST"

rsync -a --delete --exclude build --exclude target --exclude .git \
    ./ "$HOST:$FERN/"

ssh "$HOST" 'sh /tmp/fluesterwind-src/tools/remote-build.sh'

mkdir -p build
scp -q "$HOST:$FERN/build/signal-backend" build/
if ssh "$HOST" "test -f $FERN/build/fluesterwind"; then
    scp -q "$HOST:$FERN/build/fluesterwind" build/
    echo "== build/ bereit: signal-backend + fluesterwind"
else
    echo "== build/ bereit: nur signal-backend (Qt-Cross fehlt)"
fi

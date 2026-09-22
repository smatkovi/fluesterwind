#!/bin/sh
# Laeuft auf dem Build-Rechner.
set -e
SRC=/tmp/fluesterwind-src
. "$SRC/tools/cross.env"

# Toolchain vorhanden? /tmp ist ein tmpfs und nach einem Neustart leer.
if [ ! -x "$CARGO_HOME/bin/cargo" ] || [ ! -d "$MUSL" ]; then
    sh "$SRC/tools/toolchain.sh"
    . "$SRC/tools/cross.env"
fi

cd "$SRC/backend"
cargo build --release --target "$ZIEL"

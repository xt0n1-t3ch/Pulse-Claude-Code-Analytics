#!/bin/bash
# Build, deploy to releases/windows/, and launch
set -e
cd "$(dirname "$0")"

cargo build --release

SRC="releases/.cargo-target/release/cc-discord-presence.exe"
DST="releases/windows/cc-discord-presence.exe"

if [ -f "$SRC" ]; then
    mkdir -p releases/windows
    # Kill existing process if running (ignore errors)
    taskkill //F //IM cc-discord-presence.exe 2>/dev/null || true
    sleep 0.5
    cp "$SRC" "$DST"
    echo "Deployed to $DST"
fi

exec "$DST"

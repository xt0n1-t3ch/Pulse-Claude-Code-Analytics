#!/bin/bash
# Build cc-discord-presence release binary (Rust v2)
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Building cc-discord-presence (Rust release)..."

cd "$PROJECT_ROOT"

cargo build --release

echo ""
echo "Build complete!"
echo "Binary: target/release/cc-discord-presence$( [[ "$(uname -s)" == MINGW* ]] && echo '.exe' || true )"

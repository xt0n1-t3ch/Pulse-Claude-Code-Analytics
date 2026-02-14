#!/bin/bash
# Start cc-discord-presence daemon (Rust v2)
# The binary handles single-instance locking with auto-takeover internally.

set -e

CLAUDE_DIR="$HOME/.claude"
BIN_DIR="$CLAUDE_DIR/bin"
LOG_FILE="$CLAUDE_DIR/cc-discord-presence.log"
REPO="tsanva/cc-discord-presence"
VERSION="v2.0.0"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
IS_WINDOWS=false
case "$OS" in
    mingw*|msys*|cygwin*) IS_WINDOWS=true; OS="windows" ;;
esac

# Ensure directories exist
mkdir -p "$CLAUDE_DIR" "$BIN_DIR"

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64) ARCH="amd64" ;;
    aarch64|arm64) ARCH="arm64" ;;
esac

BINARY_NAME="cc-discord-presence-${OS}-${ARCH}"
if [[ "$OS" == "windows" ]]; then
    BINARY_NAME="${BINARY_NAME}.exe"
fi
BINARY="$BIN_DIR/$BINARY_NAME"

# Download binary if not present
if [[ ! -f "$BINARY" ]]; then
    echo "Downloading cc-discord-presence for ${OS}-${ARCH}..."

    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY_NAME}"

    if command -v curl &> /dev/null; then
        curl -fsSL "$DOWNLOAD_URL" -o "$BINARY"
    elif command -v wget &> /dev/null; then
        wget -q "$DOWNLOAD_URL" -O "$BINARY"
    else
        echo "Error: curl or wget required to download binary" >&2
        exit 1
    fi

    if ! $IS_WINDOWS; then
        chmod +x "$BINARY"
    fi
    echo "Downloaded successfully!"
fi

if [[ ! -f "$BINARY" ]]; then
    echo "Error: Binary not found at $BINARY" >&2
    exit 1
fi

# Start the daemon in background.
# The binary handles single-instance locking and auto-takeover of any existing instance.
if $IS_WINDOWS; then
    WIN_BINARY=$(cygpath -w "$BINARY" 2>/dev/null || echo "$BINARY")
    powershell.exe -NoProfile -WindowStyle Hidden -Command \
        "Start-Process -FilePath '$WIN_BINARY' -WindowStyle Hidden" 2>/dev/null
else
    nohup "$BINARY" > "$LOG_FILE" 2>&1 &
fi

echo "cc-discord-presence started"

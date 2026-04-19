#!/bin/bash
# Stop cc-discord-presence daemon (Rust v2)
# The binary uses file locks, so we just need to find and terminate the process.

CLAUDE_DIR="$HOME/.claude"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
IS_WINDOWS=false
case "$OS" in
    mingw*|msys*|cygwin*) IS_WINDOWS=true ;;
esac

# Read PID from instance metadata (Rust v2 writes this file)
META_FILE="$CLAUDE_DIR/cc-discord-presence.instance.json"

kill_by_metadata() {
    if [[ ! -f "$META_FILE" ]]; then
        return 1
    fi

    local pid
    pid=$(grep -o '"pid":[[:space:]]*[0-9]*' "$META_FILE" 2>/dev/null | grep -o '[0-9]*')
    if [[ -z "$pid" ]]; then
        return 1
    fi

    if $IS_WINDOWS; then
        taskkill //F //PID "$pid" >/dev/null 2>&1 && return 0
    else
        kill "$pid" 2>/dev/null && return 0
    fi
    return 1
}

kill_by_name() {
    if $IS_WINDOWS; then
        taskkill //F //IM "cc-discord-presence*.exe" >/dev/null 2>&1 || true
    else
        pkill -f cc-discord-presence 2>/dev/null || true
    fi
}

if kill_by_metadata; then
    echo "cc-discord-presence stopped"
else
    kill_by_name
    echo "cc-discord-presence stopped (fallback)"
fi

# Clean up legacy v1 files
rm -f "$CLAUDE_DIR/discord-presence.pid" 2>/dev/null
rm -f "$CLAUDE_DIR/discord-presence.refcount" 2>/dev/null
rm -rf "$CLAUDE_DIR/discord-presence-sessions" 2>/dev/null

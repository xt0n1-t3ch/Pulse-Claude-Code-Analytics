#!/usr/bin/env bash
set -euo pipefail

REPOSITORY="${1:-https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence.git}"
BRANCH="${2:-main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/src/codex/UPSTREAM.json"

if [[ ! -f "$MANIFEST" ]]; then
  echo "src/codex/UPSTREAM.json not found. Run scripts/update-codex-rich-presence.sh" >&2
  exit 1
fi

UPSTREAM_SHA="$(git ls-remote "$REPOSITORY" "refs/heads/$BRANCH" | awk '{print $1}')"
if [[ -z "$UPSTREAM_SHA" ]]; then
  echo "Unable to resolve $REPOSITORY branch $BRANCH" >&2
  exit 1
fi

LOCKED_SHA="$(python3 - <<PY
import json
from pathlib import Path
print(json.loads(Path(r"$MANIFEST").read_text()).get("commit", ""))
PY
)"

if [[ -z "$LOCKED_SHA" ]]; then
  echo "src/codex/UPSTREAM.json has no commit. Run scripts/update-codex-rich-presence.sh" >&2
  exit 1
fi

if [[ "$LOCKED_SHA" != "$UPSTREAM_SHA" ]]; then
  echo "Codex Discord Rich Presence source is stale. synced=$LOCKED_SHA upstream=$UPSTREAM_SHA. Run scripts/update-codex-rich-presence.sh" >&2
  exit 1
fi

echo "Codex Discord Rich Presence source is current: $LOCKED_SHA"

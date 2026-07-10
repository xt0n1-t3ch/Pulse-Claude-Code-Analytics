#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec pwsh -NoLogo -NoProfile -NonInteractive \
  -File "$ROOT/scripts/check-codex-rich-presence-upstream.ps1" \
  -Root "$ROOT" "$@"

#!/usr/bin/env bash
set -euo pipefail

REPOSITORY="${1:-https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence.git}"
BRANCH="${2:-main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/pulse-codex-rp-sync.XXXXXX")"
export ROOT WORK REPOSITORY BRANCH

cleanup() {
  rm -rf "$WORK"
}
trap cleanup EXIT

MANIFEST="$ROOT/src/codex/UPSTREAM.json"
export MANIFEST
BEFORE="not-synced"
if [[ -f "$MANIFEST" ]]; then
  BEFORE="$(python3 - <<'PY'
import json, os
path = os.environ.get('MANIFEST')
try:
    with open(path, encoding='utf-8') as fh:
        print(json.load(fh).get('commit') or 'not-synced')
except Exception:
    print('not-synced')
PY
)"
fi

git clone --depth 1 --branch "$BRANCH" "$REPOSITORY" "$WORK"
AFTER="$(git -C "$WORK" rev-parse HEAD)"
export AFTER

python3 - <<'PY'
import datetime
import json
import os
import pathlib

root = pathlib.Path(os.environ['ROOT'])
work = pathlib.Path(os.environ['WORK'])
branch = os.environ['BRANCH']
after = os.environ['AFTER']
codex_dir = root / 'src' / 'codex'
files = [
    'config.rs',
    'cost.rs',
    'discord.rs',
    'session.rs',
    'util.rs',
    'session/activity.rs',
    'session/parser.rs',
    'telemetry/limits.rs',
    'telemetry/plan.rs',
    'telemetry/service_tier.rs',
]
modules = ['config', 'cost', 'discord', 'metrics', 'process_guard', 'session', 'telemetry', 'util', 'opencode']

for relative in files:
    source = work / 'src' / relative
    target = codex_dir / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    text = source.read_text(encoding='utf-8')
    for module in modules:
        text = text.replace(f'crate::{module}', f'crate::codex::{module}')
    text = text.replace('fn presence_lines(', 'pub fn presence_lines(')
    if relative == 'session/parser.rs':
        text = text.replace('Command::new("git")', 'crate::util::silent_command("git")')
        text = text.replace('use std::process::Command;\n', '')
    target.write_text(text, encoding='utf-8')

(codex_dir / 'mod.rs').write_text('\n'.join([
    'pub mod config;',
    'pub mod cost;',
    'pub mod discord;',
    'pub mod process;',
    'pub mod session;',
    'pub mod telemetry {',
    '    pub mod limits;',
    '    pub mod plan;',
    '    pub mod service_tier;',
    '}',
    'pub mod util;',
    '',
]), encoding='utf-8')

(codex_dir / 'process.rs').write_text(r'''#[cfg(windows)]
use std::process::Stdio;

#[cfg(any(windows, test))]
const OPENCODE_PROCESS_NAMES: [&str; 3] = ["OpenCode.exe", "opencode.exe", "opencode-cli.exe"];

pub fn is_opencode_running() -> bool {
    is_opencode_running_impl()
}

#[cfg(windows)]
fn is_opencode_running_impl() -> bool {
    let output = crate::util::silent_command("tasklist")
        .arg("/FO")
        .arg("CSV")
        .arg("/NH")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    tasklist_has_opencode(&text)
}

#[cfg(not(windows))]
fn is_opencode_running_impl() -> bool {
    false
}

#[cfg(any(windows, test))]
pub(crate) fn tasklist_has_opencode(output: &str) -> bool {
    output.lines().any(|line| {
        let Some(name) = tasklist_image_name(line) else {
            return false;
        };
        OPENCODE_PROCESS_NAMES
            .iter()
            .any(|expected| name.eq_ignore_ascii_case(expected))
    })
}

#[cfg(any(windows, test))]
fn tasklist_image_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case(
            "INFO: No tasks are running which match the specified criteria.",
        )
    {
        return None;
    }

    let name = tasklist_csv_image_name(trimmed)
        .unwrap_or_else(|| trimmed.split_whitespace().next().unwrap_or(trimmed));
    if name.is_empty() || name.eq_ignore_ascii_case("Image Name") {
        None
    } else {
        Some(name)
    }
}

#[cfg(any(windows, test))]
fn tasklist_csv_image_name(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(&rest[..end]);
    }

    let mut fields = line.split(',');
    let first = fields.next()?.trim();
    let second = fields.next()?.trim();
    if fields.count() < 3 {
        return None;
    }

    if second.eq_ignore_ascii_case("PID") || second.parse::<u32>().is_ok() {
        Some(first)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasklist_parser_detects_opencode_names_case_insensitively() {
        let output = r#"
"Image Name","PID","Session Name","Session#","Mem Usage"
"OpenCode.exe","1234","Console","1","42,000 K"
"opencode-cli.exe","2345","Console","1","10,000 K"
"#;

        assert!(tasklist_has_opencode(output));
    }

    #[test]
    fn tasklist_parser_rejects_partial_names() {
        let output = r#"
"not-opencode.exe","1234","Console","1","42,000 K"
"opencode-helper.exe","2345","Console","1","10,000 K"
"#;

        assert!(!tasklist_has_opencode(output));
    }

    #[test]
    fn tasklist_parser_supports_table_output() {
        let output = r#"
Image Name                     PID Session Name        Session#    Mem Usage
========================= ======== ================ =========== ============
opencode.exe                  7777 Console                    1     12,000 K
"#;

        assert!(tasklist_has_opencode(output));
    }
}
''', encoding='utf-8')

cost_path = codex_dir / 'cost.rs'
cost_text = cost_path.read_text(encoding='utf-8')
cost_helpers = r'''

pub fn is_fast_capable(model_id: &str) -> bool {
    let key = normalize_model_key(model_id);
    key.starts_with("gpt-5.5") || key.starts_with("gpt-5.4")
}

pub fn speed_multiplier(model_id: &str, fast: bool) -> f64 {
    if !fast {
        return 1.0;
    }
    let key = normalize_model_key(model_id);
    if key.starts_with("gpt-5.5") {
        2.5
    } else if key.starts_with("gpt-5.4") {
        2.0
    } else {
        1.0
    }
}
'''
marker = '\n#[cfg(test)]\nmod tests {'
if marker not in cost_text:
    raise SystemExit('cost test marker not found')
cost_path.write_text(cost_text.replace(marker, cost_helpers + marker, 1), encoding='utf-8')

manifest = {
    'repository': 'https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence',
    'branch': branch,
    'commit': after,
    'synced_at': datetime.datetime.now(datetime.UTC).isoformat().replace('+00:00', 'Z'),
    'strategy': 'source-sync-with-pulse-compatibility-overlay',
}
(codex_dir / 'UPSTREAM.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
PY

cargo fmt --all
"$ROOT/scripts/check-codex-rich-presence-upstream.sh" "$REPOSITORY" "$BRANCH"
cargo test --workspace --test codex_upstream_contract

echo "Codex Discord Rich Presence source synced: $BEFORE -> $AFTER"


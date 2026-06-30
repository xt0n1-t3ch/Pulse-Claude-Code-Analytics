param(
  [string]$Repository = "https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence.git",
  [string]$Branch = "main"
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("pulse-codex-rp-sync-" + [guid]::NewGuid().ToString("N"))
$codexDir = Join-Path $root "src/codex"
$manifestPath = Join-Path $codexDir "UPSTREAM.json"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)

$before = "not-synced"
if (Test-Path $manifestPath) {
  $before = [string]((Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json).commit)
}

try {
  git clone --depth 1 --branch $Branch $Repository $work | Out-Host
  $after = git -C $work rev-parse HEAD

  $relativeFiles = @(
    "config.rs", "cost.rs", "discord.rs", "session.rs", "util.rs",
    "session/activity.rs", "session/parser.rs", "telemetry/limits.rs", "telemetry/plan.rs", "telemetry/service_tier.rs"
  )

  foreach ($relative in $relativeFiles) {
    $source = Join-Path (Join-Path $work "src") $relative
    $target = Join-Path $codexDir $relative
    New-Item -ItemType Directory -Force -Path (Split-Path $target) | Out-Null
    $text = [System.IO.File]::ReadAllText($source, [System.Text.Encoding]::UTF8)
    foreach ($name in @("config", "cost", "discord", "metrics", "process_guard", "session", "telemetry", "util", "opencode")) {
      $text = $text -replace "crate::$name", "crate::codex::$name"
    }
    $text = $text -replace "fn presence_lines\(", "pub fn presence_lines("
    if ($relative -eq "session/parser.rs") {
      $text = $text -replace 'Command::new\("git"\)', 'crate::util::silent_command("git")'
      $text = $text -replace "(?m)^use std::process::Command;`r?`n", ""
    }
    [System.IO.File]::WriteAllText($target, $text, $utf8NoBom)
  }

  [System.IO.File]::WriteAllText((Join-Path $codexDir "mod.rs"), (@(
    "pub mod config;",
    "pub mod cost;",
    "pub mod discord;",
    "pub mod process;",
    "pub mod session;",
    "pub mod telemetry {",
    "    pub mod limits;",
    "    pub mod plan;",
    "    pub mod service_tier;",
    "}",
    "pub mod util;"
  ) -join "`n") + "`n", $utf8NoBom)

  Set-Content -LiteralPath (Join-Path $codexDir "process.rs") -Encoding utf8 -Value @"
#[cfg(windows)]
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
"@

  $costPath = Join-Path $codexDir "cost.rs"
  $costText = [System.IO.File]::ReadAllText($costPath, [System.Text.Encoding]::UTF8)
  $costHelpers = @"

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
"@
  $costText = $costText -replace "`r?`n#\[cfg\(test\)\]`r?`nmod tests \{", ($costHelpers + "`n#[cfg(test)]`nmod tests {")
  [System.IO.File]::WriteAllText($costPath, $costText, $utf8NoBom)

  $manifest = @{
    repository = "https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence"
    branch = $Branch
    commit = $after
    synced_at = (Get-Date).ToUniversalTime().ToString("o")
    strategy = "source-sync-with-pulse-compatibility-overlay"
  } | ConvertTo-Json -Depth 3
  Set-Content -LiteralPath $manifestPath -Value $manifest -Encoding utf8

  cargo fmt --all
  & (Join-Path $PSScriptRoot "check-codex-rich-presence-upstream.ps1") -Repository $Repository -Branch $Branch
  cargo test --workspace --test codex_upstream_contract

  Write-Output "Codex Discord Rich Presence source synced: $before -> $after"
} finally {
  if (Test-Path $work) {
    Remove-Item -LiteralPath $work -Recurse -Force
  }
}

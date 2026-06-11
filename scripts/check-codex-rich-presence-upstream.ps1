param(
  [string]$Repository = "https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence.git",
  [string]$Branch = "main"
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifestPath = Join-Path $root "src/codex/UPSTREAM.json"
if (-not (Test-Path $manifestPath)) {
  throw "src/codex/UPSTREAM.json not found. Run scripts/update-codex-rich-presence.ps1"
}

$remote = git ls-remote $Repository "refs/heads/$Branch"
if (-not $remote) {
  throw "Unable to resolve $Repository branch $Branch"
}
$upstreamSha = ($remote -split "\s+")[0]
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$lockedSha = [string]$manifest.commit

if (-not $lockedSha) {
  throw "src/codex/UPSTREAM.json has no commit. Run scripts/update-codex-rich-presence.ps1"
}

if ($lockedSha -ne $upstreamSha) {
  throw "Codex Discord Rich Presence source is stale. synced=$lockedSha upstream=$upstreamSha. Run scripts/update-codex-rich-presence.ps1"
}

Write-Output "Codex Discord Rich Presence source is current: $lockedSha"

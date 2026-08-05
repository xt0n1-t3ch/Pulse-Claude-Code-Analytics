#requires -Version 5.1
<#
.SYNOPSIS
  Build and publish a Pulse release locally, without GitHub Actions.

.DESCRIPTION
  Builds the frontend + Tauri Windows installers and publishes a GitHub release
  with the installers and a SHA256SUMS manifest via the gh CLI. This keeps
  releases fast and off CI so Actions minutes are reserved for essentials.

  Requires: npm, cargo, cargo-tauri, and an authenticated gh CLI.

  Note: the in-app updater's signed `latest.json` is intentionally NOT produced
  locally (the minisign private key is a CI secret). Installers are attached for
  manual download; auto-update signing stays a CI-only concern.

.PARAMETER Tag
  Release tag, e.g. v1.7.1. Defaults to "v" + the version in package.json.

.PARAMETER Draft
  Create the GitHub release as a draft instead of publishing immediately.

.PARAMETER SkipBuild
  Reuse existing installers under target/release/bundle instead of rebuilding.
#>
param(
  [string]$Tag,
  [switch]$Draft,
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

if (-not $Tag) {
  $pkg = Get-Content -Raw -LiteralPath (Join-Path $root "package.json") | ConvertFrom-Json
  $Tag = "v$($pkg.version)"
}
Write-Host "Releasing $Tag ..." -ForegroundColor Cyan

Push-Location $root
try {
  if (-not $SkipBuild) {
    Write-Host "Building installers (npm run build)..." -ForegroundColor Cyan
    # `cargo tauri build` exits non-zero only on the optional updater-signing
    # step, which runs after the installers are produced. Verify the artifacts
    # exist rather than trusting the exit code.
    npm run build
  }

  $bundle = Join-Path $root "target/release/bundle"
  $assets = @()
  $assets += Get-ChildItem -Path (Join-Path $bundle "nsis") -Filter "Pulse_*_x64-setup.exe" -ErrorAction SilentlyContinue
  $assets += Get-ChildItem -Path (Join-Path $bundle "msi")  -Filter "Pulse_*_x64_en-US.msi"  -ErrorAction SilentlyContinue
  if (-not $assets) { throw "No installers found under $bundle. Run without -SkipBuild first." }

  $sums = Join-Path $bundle "SHA256SUMS.txt"
  Remove-Item -LiteralPath $sums -ErrorAction SilentlyContinue
  foreach ($asset in $assets) {
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $asset.FullName).Hash.ToLower()
    "$hash  $($asset.Name)" | Add-Content -LiteralPath $sums -Encoding ascii
  }
  $assets += Get-Item -LiteralPath $sums

  $notes = "Pulse $Tag — local build. Download the Windows installer below and run it (per-user, no admin)."
  $ghArgs = @("release", "create", $Tag, "--title", "Pulse $Tag", "--notes", $notes)
  if ($Draft) { $ghArgs += "--draft" }
  $ghArgs += ($assets | ForEach-Object { $_.FullName })

  Write-Host "Publishing GitHub release via gh ($($assets.Count) assets)..." -ForegroundColor Cyan
  gh @ghArgs
  Write-Host "Released $Tag" -ForegroundColor Green
}
finally {
  Pop-Location
}

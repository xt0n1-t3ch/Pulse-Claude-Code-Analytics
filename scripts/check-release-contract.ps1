[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Tag,
  [string]$Root = (Join-Path $PSScriptRoot "..")
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$tagPattern = '^v(?<version>(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*))?)$'

function Get-PackageVersion {
  param(
    [string]$Path,
    [string]$PackageName
  )

  $content = Get-Content -Raw -LiteralPath $Path
  $pattern = '(?ms)^\[\[package\]\]\s*\r?\nname = "' + [regex]::Escape($PackageName) + '"\s*\r?\nversion = "(?<version>[^"]+)"'
  $match = [regex]::Match($content, $pattern)
  if (-not $match.Success) {
    throw "Unable to find $PackageName in $Path"
  }
  $match.Groups["version"].Value
}

function Get-CargoManifestVersion {
  param([string]$Path)

  $content = Get-Content -Raw -LiteralPath $Path
  $match = [regex]::Match($content, '(?ms)^\[package\].*?^version\s*=\s*"(?<version>[^"]+)"')
  if (-not $match.Success) {
    throw "Unable to find package version in $Path"
  }
  $match.Groups["version"].Value
}

if ($Tag -notmatch $tagPattern) {
  throw "Tag must be a semantic version tag such as v1.5.2 or v1.5.3-rc.1"
}
$version = $Matches.version
$rootPath = (Resolve-Path -LiteralPath $Root).Path

$package = Get-Content -Raw -LiteralPath (Join-Path $rootPath "package.json") | ConvertFrom-Json
$frontendPackage = Get-Content -Raw -LiteralPath (Join-Path $rootPath "frontend/package.json") | ConvertFrom-Json
$frontendLock = Get-Content -Raw -LiteralPath (Join-Path $rootPath "frontend/package-lock.json") | ConvertFrom-Json -AsHashtable
$tauriConfig = Get-Content -Raw -LiteralPath (Join-Path $rootPath "src-tauri/tauri.conf.json") | ConvertFrom-Json
$cargoLockPath = Join-Path $rootPath "Cargo.lock"

$versionSurfaces = [ordered]@{
  "Cargo.toml" = Get-CargoManifestVersion (Join-Path $rootPath "Cargo.toml")
  "src-tauri/Cargo.toml" = Get-CargoManifestVersion (Join-Path $rootPath "src-tauri/Cargo.toml")
  "Cargo.lock:cc-discord-presence" = Get-PackageVersion $cargoLockPath "cc-discord-presence"
  "Cargo.lock:pulse" = Get-PackageVersion $cargoLockPath "pulse"
  "package.json" = [string]$package.version
  "frontend/package.json" = [string]$frontendPackage.version
  "frontend/package-lock.json" = [string]$frontendLock["version"]
  "frontend/package-lock.json:packages" = [string]$frontendLock["packages"][""]["version"]
  "src-tauri/tauri.conf.json" = [string]$tauriConfig.version
}

foreach ($surface in $versionSurfaces.GetEnumerator()) {
  if ($surface.Value -ne $version) {
    throw "Release version mismatch: tag=$version $($surface.Key)=$($surface.Value)"
  }
}

$changelogPath = Join-Path $rootPath "CHANGELOG.md"
$changelog = Get-Content -Raw -LiteralPath $changelogPath
$sectionPattern = '(?ms)^## \[' + [regex]::Escape($version) + '\][^\r\n]*\r?\n(?<body>.*?)(?=^## \[|\z)'
$section = [regex]::Match($changelog, $sectionPattern)
if (-not $section.Success) {
  throw "CHANGELOG.md has no section for $version"
}
if ($section.Groups["body"].Value.Trim().Length -lt 80) {
  throw "CHANGELOG.md section for $version is too short to publish"
}

Write-Output "Release contract verified: tag=$Tag version=$version surfaces=$($versionSurfaces.Count)"

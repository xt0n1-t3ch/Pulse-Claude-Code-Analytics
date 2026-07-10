[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$Tag,
  [Parameter(Mandatory)] [string]$ExpectedCommit,
  [string]$Root = (Join-Path $PSScriptRoot ".."),
  [switch]$TestMode
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$tagPattern = '^v(?<version>(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*))?)$'

function Invoke-Git {
  param(
    [Parameter(Mandatory)] [string]$Directory,
    [Parameter(Mandatory)] [string[]]$Arguments,
    [switch]$AllowFailure
  )

  $output = @(& git -C $Directory @Arguments 2>&1)
  $exitCode = $LASTEXITCODE
  if (-not $AllowFailure -and $exitCode -ne 0) {
    throw "git $($Arguments -join ' ') failed: $($output -join [Environment]::NewLine)"
  }
  [pscustomobject]@{
    ExitCode = $exitCode
    Output = @($output | ForEach-Object { [string]$_ })
  }
}

function Get-PackageVersion {
  param(
    [Parameter(Mandatory)] [string]$Path,
    [Parameter(Mandatory)] [string]$PackageName
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
  param([Parameter(Mandatory)] [string]$Path)

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
if ($ExpectedCommit -notmatch '^[0-9a-fA-F]{40}$') {
  throw "ExpectedCommit must be a full 40-character Git SHA"
}
$ExpectedCommit = $ExpectedCommit.ToLowerInvariant()
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

foreach ($documentationSurface in @("README.md", "docs/index.md")) {
  $content = Get-Content -Raw -LiteralPath (Join-Path $rootPath $documentationSurface)
  if (-not $content.Contains("v$version")) {
    throw "$documentationSurface does not identify release v$version"
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

$tagType = Invoke-Git $rootPath @("cat-file", "-t", "refs/tags/$Tag") -AllowFailure
if ($tagType.ExitCode -ne 0 -or ($tagType.Output | Select-Object -First 1) -ne "tag") {
  throw "Release tag $Tag must be an annotated tag"
}
$taggedCommit = (Invoke-Git $rootPath @("rev-parse", "$Tag^{commit}")).Output[0].Trim().ToLowerInvariant()
if ($taggedCommit -ne $ExpectedCommit) {
  throw "Release tag $Tag resolves to $taggedCommit instead of expected commit $ExpectedCommit"
}

$reachable = Invoke-Git $rootPath @("merge-base", "--is-ancestor", $ExpectedCommit, "origin/main") -AllowFailure
if ($reachable.ExitCode -ne 0) {
  throw "Release commit $ExpectedCommit is not reachable from origin/main"
}

if (-not $TestMode) {
  $headCommit = (Invoke-Git $rootPath @("rev-parse", "HEAD")).Output[0].Trim().ToLowerInvariant()
  if ($headCommit -ne $ExpectedCommit) {
    throw "Release checkout HEAD $headCommit does not match expected commit $ExpectedCommit"
  }
}

Write-Output "Release contract verified: tag=$Tag version=$version commit=$ExpectedCommit surfaces=$($versionSurfaces.Count)"

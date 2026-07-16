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

function Assert-RepositoryReleaseContract {
  param(
    [Parameter(Mandatory)] [string]$RootPath,
    [Parameter(Mandatory)] [string]$Version
  )

  $contractPath = Join-Path $RootPath "scripts/release-contract.json"
  if (-not (Test-Path -LiteralPath $contractPath -PathType Leaf)) { return }
  try { $contract = Get-Content -Raw -LiteralPath $contractPath | ConvertFrom-Json }
  catch { throw "scripts/release-contract.json is not valid JSON: $($_.Exception.Message)" }
  if ([int]$contract.schema_version -ne 1) { throw "scripts/release-contract.json schema_version must be 1" }
  if ([string]$contract.product.version -ne $Version) {
    throw "Release contract product version '$($contract.product.version)' does not match '$Version'"
  }

  $cargoSource = Get-Content -Raw -LiteralPath (Join-Path $RootPath "Cargo.toml")
  $dependencyPattern = '(?m)^codex-presence-core\s*=\s*\{(?<body>[^\r\n]+)\}'
  $dependency = [regex]::Match($cargoSource, $dependencyPattern)
  if (-not $dependency.Success) { throw "Cargo.toml does not declare codex-presence-core" }
  $body = $dependency.Groups["body"].Value
  if ($body -match '\bpath\s*=' -or $body -notmatch '\bgit\s*=') {
    throw "Pulse releases require codex-presence-core as an immutable Git dependency, not a path dependency"
  }
  $revision = [regex]::Match($body, '\brev\s*=\s*"(?<sha>[0-9a-fA-F]{40})"')
  if (-not $revision.Success) { throw "codex-presence-core must use a full 40-character Git rev" }
  if ($body -notmatch ('\bversion\s*=\s*"' + [regex]::Escape([string]$contract.core.version) + '"')) {
    throw "codex-presence-core dependency version does not match $($contract.core.version)"
  }

  $canonicalManifest = Get-Content -Raw -LiteralPath (Join-Path $RootPath ([string]$contract.core.manifest)) | ConvertFrom-Json
  $manifestCommit = if ($canonicalManifest.PSObject.Properties.Name -contains "canonical_commit") {
    [string]$canonicalManifest.canonical_commit
  } elseif ($canonicalManifest.PSObject.Properties.Name -contains "core" -and $null -ne $canonicalManifest.core.commit) {
    [string]$canonicalManifest.core.commit
  } else {
    [string]$canonicalManifest.commit
  }
  $manifestVersion = if ($canonicalManifest.PSObject.Properties.Name -contains "integration") {
    [string]$canonicalManifest.integration.version
  } elseif ($canonicalManifest.PSObject.Properties.Name -contains "core" -and $null -ne $canonicalManifest.core.version) {
    [string]$canonicalManifest.core.version
  } elseif ($canonicalManifest.PSObject.Properties.Name -contains "core_version") {
    [string]$canonicalManifest.core_version
  } else { $null }
  if ($manifestCommit.ToLowerInvariant() -ne $revision.Groups["sha"].Value.ToLowerInvariant()) {
    throw "Canonical core manifest SHA does not match the Cargo Git rev"
  }
  if ([string]::IsNullOrWhiteSpace($manifestVersion) -or $manifestVersion -ne [string]$contract.core.version) {
    throw "Canonical core manifest version does not match $($contract.core.version)"
  }

  foreach ($schema in @($contract.configuration, $contract.database)) {
    $source = Get-Content -Raw -LiteralPath (Join-Path $RootPath ([string]$schema.source))
    $pattern = '(?:CONFIG_)?SCHEMA_VERSION\s*:\s*(?:u32|i64)\s*=\s*' + [regex]::Escape([string]$schema.schema_version) + '\s*;'
    if ($source -notmatch $pattern) { throw "$($schema.source) does not declare schema $($schema.schema_version)" }
  }
  if ([string]$contract.release.windows_sbom -notmatch '\.spdx\.json$' -or [string]$contract.release.checksum_manifest -ne 'SHA256SUMS.txt') {
    throw "Release integrity contract must require an SPDX JSON SBOM and SHA256SUMS.txt"
  }
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
Assert-RepositoryReleaseContract -RootPath $rootPath -Version $version

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

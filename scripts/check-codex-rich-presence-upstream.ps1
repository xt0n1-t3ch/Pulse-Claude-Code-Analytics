[CmdletBinding()]
param(
  [string]$Root = (Join-Path $PSScriptRoot ".."),
  [switch]$TestMode
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-RequiredString {
  param(
    [Parameter(Mandatory)] [object]$Object,
    [Parameter(Mandatory)] [string]$Name,
    [Parameter(Mandatory)] [string]$Pattern
  )

  $property = $Object.PSObject.Properties[$Name]
  $value = if ($null -eq $property) { "" } else { [string]$property.Value }
  if ([string]::IsNullOrWhiteSpace($value) -or $value -notmatch $Pattern) {
    throw "src/codex/UPSTREAM.json has an invalid $Name"
  }
  $value
}

function Get-ContainedPath {
  param(
    [Parameter(Mandatory)] [string]$RootPath,
    [Parameter(Mandatory)] [string]$RelativePath,
    [Parameter(Mandatory)] [string]$RequiredPrefix
  )

  $normalized = $RelativePath.Replace("\", "/")
  $segments = $normalized.Split("/", [System.StringSplitOptions]::RemoveEmptyEntries)
  if (
    [System.IO.Path]::IsPathRooted($RelativePath) -or
    $segments -contains ".." -or
    -not $normalized.StartsWith($RequiredPrefix, [System.StringComparison]::Ordinal)
  ) {
    throw "Vendored path escapes its allowed prefix: $RelativePath"
  }

  $canonicalRoot = [System.IO.Path]::GetFullPath($RootPath)
  $fullPath = [System.IO.Path]::GetFullPath((Join-Path $canonicalRoot $normalized))
  $relativeToRoot = [System.IO.Path]::GetRelativePath($canonicalRoot, $fullPath).Replace("\", "/")
  if (
    [System.IO.Path]::IsPathRooted($relativeToRoot) -or
    $relativeToRoot -eq ".." -or
    $relativeToRoot.StartsWith("../", [System.StringComparison]::Ordinal)
  ) {
    throw "Vendored path escapes the repository root: $RelativePath"
  }
  $fullPath
}

function Get-ExactStringSet {
  param(
    [Parameter(Mandatory)] [object[]]$Values,
    [Parameter(Mandatory)] [string]$Label
  )

  $strings = @($Values | ForEach-Object { [string]$_ })
  if ($strings.Count -ne (@($strings | Sort-Object -Unique)).Count) {
    throw "$Label contains duplicate entries"
  }
  @($strings | Sort-Object)
}

function Get-TargetSha256 {
  param(
    [Parameter(Mandatory)] [string]$Path,
    [Parameter(Mandatory)] [string]$Mode
  )

  if ($Mode -eq "byte-copy") {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
  }

  $text = [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
  $normalized = $text.Replace("`r`n", "`n").Replace("`r", "`n")
  $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($normalized)
  [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}

$rootPath = (Resolve-Path -LiteralPath $Root).Path
$contractPath = Join-Path $PSScriptRoot "codex-vendor-contract.json"
$manifestPath = Join-Path $rootPath "src/codex/UPSTREAM.json"
if (-not (Test-Path -LiteralPath $contractPath -PathType Leaf)) {
  throw "scripts/codex-vendor-contract.json not found"
}
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
  throw "src/codex/UPSTREAM.json not found. Run scripts/update-codex-rich-presence.ps1"
}

try {
  $contract = Get-Content -Raw -LiteralPath $contractPath | ConvertFrom-Json
} catch {
  throw "scripts/codex-vendor-contract.json is not valid JSON: $($_.Exception.Message)"
}
try {
  $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
} catch {
  throw "src/codex/UPSTREAM.json is not valid JSON: $($_.Exception.Message)"
}

$schemaVersionProperty = $manifest.PSObject.Properties["schema_version"]
$syncVersionProperty = $manifest.PSObject.Properties["sync_version"]
if (
  $null -eq $schemaVersionProperty -or
  $null -eq $syncVersionProperty -or
  $schemaVersionProperty.Value -ne 2 -or
  $syncVersionProperty.Value -ne $contract.sync_version
) {
  throw "src/codex/UPSTREAM.json must use schema_version 2 and sync_version $($contract.sync_version)"
}

$officialRepository = Get-RequiredString $contract "official_repository" '^https://github\.com/'
$repository = Get-RequiredString $manifest "repository" '^https://github\.com/'
if ($repository -ne $officialRepository) {
  throw "src/codex/UPSTREAM.json repository must be $officialRepository"
}
$null = Get-RequiredString $manifest "ref" '^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$'
$commit = Get-RequiredString $manifest "commit" '^[0-9a-fA-F]{40}$'
$inventoryName = Get-RequiredString $manifest "inventory" '^[a-z0-9][a-z0-9-]*$'
$strategy = Get-RequiredString $manifest "strategy" '^[a-z0-9][a-z0-9-]*$'
$provenance = Get-RequiredString $manifest "provenance" '^(official|test)$'
if ($provenance -eq "test" -and -not $TestMode) {
  throw "Test provenance is accepted only with -TestMode"
}

$upstreamCommittedAt = [System.DateTimeOffset]::MinValue
if (-not [System.DateTimeOffset]::TryParse([string]$manifest.upstream_committed_at, [ref]$upstreamCommittedAt)) {
  throw "src/codex/UPSTREAM.json has an invalid upstream_committed_at"
}

$inventoryProperty = $contract.inventories.PSObject.Properties[$inventoryName]
if ($null -eq $inventoryProperty) {
  throw "src/codex/UPSTREAM.json names an unknown expected inventory: $inventoryName"
}
$inventory = $inventoryProperty.Value
if ($strategy -ne [string]$inventory.strategy) {
  throw "src/codex/UPSTREAM.json strategy does not match expected inventory $inventoryName"
}

$expectedAdapters = Get-ExactStringSet @($contract.local_adapters) "Vendoring contract adapters"
$actualAdapters = Get-ExactStringSet @($manifest.local_adapters) "Manifest adapters"
if (($expectedAdapters -join "|") -ne ($actualAdapters -join "|")) {
  throw "src/codex/UPSTREAM.json adapters do not match the expected inventory contract"
}

$expectedFiles = @($inventory.files)
$actualFiles = @($manifest.files)
if ($actualFiles.Count -ne $expectedFiles.Count) {
  throw "src/codex/UPSTREAM.json expected inventory '$inventoryName' requires $($expectedFiles.Count) files, found $($actualFiles.Count)"
}

$actualByTarget = @{}
foreach ($file in $actualFiles) {
  $target = Get-RequiredString $file "target" '^src/codex/.+\.(?:rs|json)$'
  if ($actualByTarget.ContainsKey($target)) {
    throw "Duplicate vendored target in src/codex/UPSTREAM.json: $target"
  }
  $actualByTarget[$target] = $file
}

foreach ($expected in $expectedFiles) {
  $source = [string]$expected.source
  $target = [string]$expected.target
  $mode = [string]$expected.mode
  if (-not $actualByTarget.ContainsKey($target)) {
    throw "src/codex/UPSTREAM.json expected inventory '$inventoryName' is missing $target"
  }

  $actual = $actualByTarget[$target]
  if (
    (Get-RequiredString $actual "source" '^src/.+\.(?:rs|json)$') -ne $source -or
    (Get-RequiredString $actual "mode" '^[a-z][a-z-]*$') -ne $mode
  ) {
    throw "Vendored mapping differs from expected inventory '$inventoryName': $target"
  }
  $null = Get-RequiredString $actual "source_sha256" '^[0-9a-fA-F]{64}$'
  $expectedHash = Get-RequiredString $actual "target_sha256" '^[0-9a-fA-F]{64}$'

  $null = Get-ContainedPath $rootPath $source "src/"
  $targetPath = Get-ContainedPath $rootPath $target "src/codex/"
  if ($actualAdapters -contains $target) {
    throw "Pulse-owned adapter cannot be listed as a mirrored file: $target"
  }
  if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
    throw "Vendored file is missing: $target"
  }

  $actualHash = Get-TargetSha256 $targetPath $mode
  if ($actualHash -ne $expectedHash.ToLowerInvariant()) {
    throw "Vendored file hash mismatch: $target expected=$expectedHash actual=$actualHash"
  }
}

$modulePath = Join-Path $rootPath "src/codex/mod.rs"
if (-not (Test-Path -LiteralPath $modulePath -PathType Leaf)) {
  throw "Pulse-owned adapter is missing: src/codex/mod.rs"
}
$moduleSource = Get-Content -Raw -LiteralPath $modulePath
foreach ($declaration in @($inventory.required_module_declarations)) {
  if (-not $moduleSource.Contains([string]$declaration)) {
    throw "src/codex/mod.rs is missing required declaration: $declaration"
  }
}

Write-Output "Codex Rich Presence integrity verified: inventory=$inventoryName commit=$commit files=$($actualFiles.Count)"

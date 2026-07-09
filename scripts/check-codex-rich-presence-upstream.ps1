[CmdletBinding()]
param(
  [string]$Root = (Join-Path $PSScriptRoot "..")
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-RequiredString {
  param(
    [object]$Object,
    [string]$Name,
    [string]$Pattern
  )

  $property = $Object.PSObject.Properties[$Name]
  $value = if ($null -eq $property) { "" } else { [string]$property.Value }
  if ([string]::IsNullOrWhiteSpace($value) -or ($Pattern -and $value -notmatch $Pattern)) {
    throw "src/codex/UPSTREAM.json has an invalid $Name"
  }
  $value
}

function Get-ContainedPath {
  param(
    [string]$RootPath,
    [string]$RelativePath,
    [string]$RequiredPrefix
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

  $rootPrefix = $RootPath.TrimEnd(
    [System.IO.Path]::DirectorySeparatorChar,
    [System.IO.Path]::AltDirectorySeparatorChar
  ) + [System.IO.Path]::DirectorySeparatorChar
  $fullPath = [System.IO.Path]::GetFullPath((Join-Path $RootPath $normalized))
  if (-not $fullPath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Vendored path escapes the repository root: $RelativePath"
  }
  $fullPath
}

$rootPath = (Resolve-Path -LiteralPath $Root).Path
$manifestPath = Join-Path $rootPath "src/codex/UPSTREAM.json"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
  throw "src/codex/UPSTREAM.json not found. Run scripts/update-codex-rich-presence.ps1"
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
  $syncVersionProperty.Value -ne 2
) {
  throw "src/codex/UPSTREAM.json must use schema_version 2 and sync_version 2"
}

$null = Get-RequiredString $manifest "repository" '^https://github\.com/xt0n1-t3ch/Codex-Discord-Rich-Presence$'
$null = Get-RequiredString $manifest "ref" '^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*))?$'
$commit = Get-RequiredString $manifest "commit" '^[0-9a-fA-F]{40}$'
$upstreamCommittedAtProperty = $manifest.PSObject.Properties["upstream_committed_at"]
$upstreamCommittedAt = [System.DateTimeOffset]::MinValue
if (
  $null -eq $upstreamCommittedAtProperty -or
  -not [System.DateTimeOffset]::TryParse(
    [string]$upstreamCommittedAtProperty.Value,
    [ref]$upstreamCommittedAt
  )
) {
  throw "src/codex/UPSTREAM.json has an invalid upstream_committed_at"
}
$null = Get-RequiredString $manifest "strategy" '^namespace-rebase-v1$'

$localAdaptersProperty = $manifest.PSObject.Properties["local_adapters"]
if ($null -eq $localAdaptersProperty) {
  throw "src/codex/UPSTREAM.json has no local_adapters contract"
}
$adapters = @($localAdaptersProperty.Value)
$requiredAdapters = @("src/codex/mod.rs", "src/codex/process.rs")
foreach ($requiredAdapter in $requiredAdapters) {
  if ($adapters -notcontains $requiredAdapter) {
    throw "src/codex/UPSTREAM.json must identify $requiredAdapter as Pulse-owned"
  }
}

$filesProperty = $manifest.PSObject.Properties["files"]
if ($null -eq $filesProperty) {
  throw "src/codex/UPSTREAM.json has no vendored files"
}
$files = @($filesProperty.Value)
if ($files.Count -eq 0) {
  throw "src/codex/UPSTREAM.json has no vendored files"
}

$seenTargets = [System.Collections.Generic.HashSet[string]]::new(
  [System.StringComparer]::OrdinalIgnoreCase
)
foreach ($file in $files) {
  $source = Get-RequiredString $file "source" '^src/.+\.rs$'
  $target = Get-RequiredString $file "target" '^src/codex/.+\.rs$'
  $null = Get-RequiredString $file "source_sha256" '^[0-9a-fA-F]{64}$'
  $expectedHash = Get-RequiredString $file "sha256" '^[0-9a-fA-F]{64}$'

  $null = Get-ContainedPath $rootPath $source "src/"
  $targetPath = Get-ContainedPath $rootPath $target "src/codex/"
  if ($adapters -contains $target) {
    throw "Pulse-owned adapter cannot be listed as a mirrored file: $target"
  }
  if (-not $seenTargets.Add($target)) {
    throw "Duplicate vendored target in src/codex/UPSTREAM.json: $target"
  }
  if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
    throw "Vendored file is missing: $target"
  }

  $actualHash = (Get-FileHash -LiteralPath $targetPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualHash -ne $expectedHash.ToLowerInvariant()) {
    throw "Vendored file hash mismatch: $target expected=$expectedHash actual=$actualHash"
  }
}

Write-Output "Codex Rich Presence integrity verified: commit=$commit files=$($files.Count)"

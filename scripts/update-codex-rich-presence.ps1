[CmdletBinding()]
param(
  [string]$Repository = "https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence.git",
  [Parameter(Mandatory = $true)]
  [string]$Tag,
  [Parameter(Mandatory = $true)]
  [string]$Commit,
  [string]$Root = (Join-Path $PSScriptRoot "..")
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$tagPattern = '^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*))?$'
$shaPattern = '^[0-9a-fA-F]{40}$'
$canonicalUrl = "https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence"
$relativeFiles = @(
  "config.rs",
  "cost.rs",
  "discord.rs",
  "model.rs",
  "session.rs",
  "util.rs",
  "session/activity.rs",
  "session/parser.rs",
  "telemetry/limits.rs",
  "telemetry/plan.rs",
  "telemetry/service_tier.rs"
)
$namespaceModules = @(
  "config",
  "cost",
  "discord",
  "metrics",
  "model",
  "opencode",
  "process_guard",
  "session",
  "telemetry",
  "util"
)
$localAdapters = @("src/codex/mod.rs", "src/codex/process.rs")

function Invoke-Git {
  param([string[]]$Arguments)

  $output = @(& git @Arguments 2>&1)
  if ($LASTEXITCODE -ne 0) {
    throw "git $($Arguments -join ' ') failed: $($output -join [Environment]::NewLine)"
  }
  $output | ForEach-Object { [string]$_ }
}

function Get-Sha256 {
  param([string]$Path)
  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-PulseAdapters {
  param([string]$CodexDirectory)

  $modulePath = Join-Path $CodexDirectory "mod.rs"
  $processPath = Join-Path $CodexDirectory "process.rs"
  if (-not (Test-Path -LiteralPath $modulePath -PathType Leaf)) {
    throw "Pulse-owned adapter is missing: src/codex/mod.rs"
  }
  if (-not (Test-Path -LiteralPath $processPath -PathType Leaf)) {
    throw "Pulse-owned adapter is missing: src/codex/process.rs"
  }

  $moduleSource = Get-Content -Raw -LiteralPath $modulePath
  if (-not $moduleSource.Contains("pub mod process;")) {
    throw "Pulse-owned src/codex/mod.rs must expose the process adapter"
  }

  $processSource = Get-Content -Raw -LiteralPath $processPath
  foreach ($requiredSymbol in @(
    "Codex.exe",
    "is_opencode_running",
    "is_codex_app_running",
    "is_desktop_surface_running"
  )) {
    if (-not $processSource.Contains($requiredSymbol)) {
      throw "Pulse-owned process adapter is missing required symbol: $requiredSymbol"
    }
  }
}

if ($Tag -notmatch $tagPattern) {
  throw "Tag must be an immutable semantic version tag such as v1.7.2"
}
if ($Commit -notmatch $shaPattern) {
  throw "Commit must be a full 40-character Git SHA"
}
$Commit = $Commit.ToLowerInvariant()

$rootPath = (Resolve-Path -LiteralPath $Root).Path
$codexDir = Join-Path $rootPath "src/codex"
Assert-PulseAdapters $codexDir

if (Test-Path -LiteralPath $Repository -PathType Container) {
  $repositorySource = (Resolve-Path -LiteralPath $Repository).Path
} else {
  $repositoryUri = $null
  if (
    -not [System.Uri]::TryCreate($Repository, [System.UriKind]::Absolute, [ref]$repositoryUri) -or
    $repositoryUri.Scheme -ne "https" -or
    $repositoryUri.Host -ne "github.com"
  ) {
    throw "Repository must be an existing local Git repository or an HTTPS GitHub URL"
  }
  $normalizedUrl = $Repository.TrimEnd("/") -replace '\.git$', ''
  if ($normalizedUrl -ne $canonicalUrl) {
    throw "Remote repository must be $canonicalUrl"
  }
  $repositorySource = $Repository
}

$remoteRefs = Invoke-Git @(
  "ls-remote",
  "--tags",
  $repositorySource,
  "refs/tags/$Tag",
  "refs/tags/$Tag^{}"
)
$resolvedCommit = $null
foreach ($line in $remoteRefs) {
  if ($line -match "^(?<sha>[0-9a-fA-F]{40})\s+refs/tags/$([regex]::Escape($Tag))\^\{\}$") {
    $resolvedCommit = $Matches.sha.ToLowerInvariant()
    break
  }
}
if ($null -eq $resolvedCommit) {
  foreach ($line in $remoteRefs) {
    if ($line -match "^(?<sha>[0-9a-fA-F]{40})\s+refs/tags/$([regex]::Escape($Tag))$") {
      $resolvedCommit = $Matches.sha.ToLowerInvariant()
      break
    }
  }
}
if ($null -eq $resolvedCommit) {
  throw "Remote repository has no tag named $Tag"
}
if ($resolvedCommit -ne $Commit) {
  throw "Tag $Tag does not resolve to commit $Commit (resolved $resolvedCommit)"
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("pulse-codex-rp-sync-" + [guid]::NewGuid().ToString("N"))
$staging = Join-Path $work "pulse-vendor"
try {
  $null = Invoke-Git @(
    "-c",
    "core.autocrlf=false",
    "clone",
    "--quiet",
    "--depth",
    "1",
    "--branch",
    $Tag,
    "--single-branch",
    $repositorySource,
    $work
  )
  $checkedOutCommit = (Invoke-Git @("-C", $work, "rev-parse", "HEAD") | Select-Object -First 1).Trim().ToLowerInvariant()
  if ($checkedOutCommit -ne $Commit) {
    throw "Checked out commit $checkedOutCommit does not match requested commit $Commit"
  }
  $upstreamCommittedAt = (Invoke-Git @("-C", $work, "show", "-s", "--format=%cI", "HEAD") | Select-Object -First 1).Trim()

  foreach ($relative in $relativeFiles) {
    $sourcePath = Join-Path (Join-Path $work "src") $relative
    if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
      throw "Canonical tag $Tag is missing required source file: src/$relative"
    }
  }

  $canonicalCost = Get-Content -Raw -LiteralPath (Join-Path $work "src/cost.rs")
  foreach ($requiredSymbol in @("pub fn is_fast_capable", "pub fn speed_multiplier")) {
    if (-not $canonicalCost.Contains($requiredSymbol)) {
      throw "Canonical cost contract is missing required symbol: $requiredSymbol"
    }
  }
  $canonicalDiscord = Get-Content -Raw -LiteralPath (Join-Path $work "src/discord.rs")
  if (-not $canonicalDiscord.Contains("pub fn presence_lines")) {
    throw "Canonical Discord contract must expose pub fn presence_lines"
  }

  $namespacePattern = "\bcrate::(" + (($namespaceModules | ForEach-Object { [regex]::Escape($_) }) -join "|") + ")\b"
  $fileEntries = @()
  foreach ($relative in $relativeFiles) {
    $sourcePath = Join-Path (Join-Path $work "src") $relative
    $stagedPath = Join-Path $staging $relative
    New-Item -ItemType Directory -Force -Path (Split-Path $stagedPath) | Out-Null

    $sourceText = [System.IO.File]::ReadAllText($sourcePath, [System.Text.Encoding]::UTF8)
    $vendoredText = [regex]::Replace($sourceText, $namespacePattern, 'crate::codex::$1')
    [System.IO.File]::WriteAllText($stagedPath, $vendoredText, $utf8NoBom)

    $fileEntries += [ordered]@{
      source = "src/$($relative.Replace('\', '/'))"
      target = "src/codex/$($relative.Replace('\', '/'))"
      source_sha256 = Get-Sha256 $sourcePath
      sha256 = Get-Sha256 $stagedPath
    }
  }

  foreach ($relative in $relativeFiles) {
    $stagedPath = Join-Path $staging $relative
    $targetPath = Join-Path $codexDir $relative
    New-Item -ItemType Directory -Force -Path (Split-Path $targetPath) | Out-Null
    [System.IO.File]::WriteAllBytes($targetPath, [System.IO.File]::ReadAllBytes($stagedPath))
  }

  $manifest = [ordered]@{
    schema_version = 2
    sync_version = 2
    repository = $canonicalUrl
    ref = $Tag
    commit = $Commit
    upstream_committed_at = $upstreamCommittedAt
    strategy = "namespace-rebase-v1"
    files = $fileEntries
    local_adapters = $localAdapters
  }
  $manifestJson = ($manifest | ConvertTo-Json -Depth 5) + "`n"
  $manifestPath = Join-Path $codexDir "UPSTREAM.json"
  $manifestTemporaryPath = "$manifestPath.tmp"
  [System.IO.File]::WriteAllText($manifestTemporaryPath, $manifestJson, $utf8NoBom)
  Move-Item -Force -LiteralPath $manifestTemporaryPath -Destination $manifestPath

  & (Join-Path $PSScriptRoot "check-codex-rich-presence-upstream.ps1") -Root $rootPath
  if ($LASTEXITCODE -ne 0) {
    throw "Vendored source integrity verification failed"
  }

  Write-Output "Codex Rich Presence synced from $Tag at $Commit"
} finally {
  if (Test-Path -LiteralPath $work) {
    Remove-Item -LiteralPath $work -Recurse -Force
  }
}

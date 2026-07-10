[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$Tag,
  [Parameter(Mandatory)] [string]$Commit,
  [string]$Root = (Join-Path $PSScriptRoot ".."),
  [switch]$TestMode,
  [string]$TestRepository
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$tagPattern = '^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*))?$'
$shaPattern = '^[0-9a-fA-F]{40}$'

function Invoke-Git {
  param([Parameter(Mandatory)] [string[]]$Arguments)

  $output = @(& git @Arguments 2>&1)
  if ($LASTEXITCODE -ne 0) {
    throw "git $($Arguments -join ' ') failed: $($output -join [Environment]::NewLine)"
  }
  @($output | ForEach-Object { [string]$_ })
}

function Get-Sha256 {
  param([Parameter(Mandatory)] [string]$Path)
  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-PulseAdapters {
  param([Parameter(Mandatory)] [string]$CodexDirectory)

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

function Test-InventoryAvailable {
  param(
    [Parameter(Mandatory)] [object]$Inventory,
    [Parameter(Mandatory)] [string]$CheckoutRoot
  )

  foreach ($mapping in @($Inventory.files)) {
    if (-not (Test-Path -LiteralPath (Join-Path $CheckoutRoot ([string]$mapping.source)) -PathType Leaf)) {
      return $false
    }
  }
  $true
}

if ($Tag -notmatch $tagPattern) {
  throw "Tag must be an immutable semantic version tag such as v1.7.2"
}
if ($Commit -notmatch $shaPattern) {
  throw "Commit must be a full 40-character Git SHA"
}
$Commit = $Commit.ToLowerInvariant()

$rootPath = (Resolve-Path -LiteralPath $Root).Path
$codexDirectory = Join-Path $rootPath "src/codex"
Assert-PulseAdapters $codexDirectory

$contractPath = Join-Path $PSScriptRoot "codex-vendor-contract.json"
try {
  $contract = Get-Content -Raw -LiteralPath $contractPath | ConvertFrom-Json
} catch {
  throw "scripts/codex-vendor-contract.json is not valid JSON: $($_.Exception.Message)"
}
$officialRepository = [string]$contract.official_repository
if ($officialRepository -ne "https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence") {
  throw "Vendoring contract names an unexpected official repository"
}

if ($TestMode) {
  if ([string]::IsNullOrWhiteSpace($TestRepository) -or -not (Test-Path -LiteralPath $TestRepository -PathType Container)) {
    throw "-TestMode requires an existing local -TestRepository"
  }
  $repositorySource = (Resolve-Path -LiteralPath $TestRepository).Path
  $provenance = "test"
} else {
  if (-not [string]::IsNullOrWhiteSpace($TestRepository)) {
    throw "-TestRepository is accepted only with -TestMode"
  }
  $repositorySource = "$officialRepository.git"
  $provenance = "official"
}

$remoteRefs = Invoke-Git @(
  "ls-remote",
  "--tags",
  $repositorySource,
  "refs/tags/$Tag",
  "refs/tags/$Tag^{}"
)
$tagObject = $remoteRefs | Where-Object { $_ -match "^[0-9a-fA-F]{40}\s+refs/tags/$([regex]::Escape($Tag))$" } | Select-Object -First 1
$peeledTag = $remoteRefs | Where-Object { $_ -match "^[0-9a-fA-F]{40}\s+refs/tags/$([regex]::Escape($Tag))\^\{\}$" } | Select-Object -First 1
if ($null -eq $tagObject) {
  throw "Repository has no tag named $Tag"
}
if ($null -eq $peeledTag) {
  throw "Tag $Tag must be an annotated tag"
}
$resolvedCommit = ($peeledTag -split '\s+')[0].ToLowerInvariant()
if ($resolvedCommit -ne $Commit) {
  throw "Tag $Tag does not resolve to commit $Commit (resolved $resolvedCommit)"
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("pulse-codex-rp-sync-" + [guid]::NewGuid().ToString("N"))
$checkout = Join-Path $work "canonical"
$verificationRoot = Join-Path $work "verification"
$verificationCodex = Join-Path $verificationRoot "src/codex"
try {
  $null = Invoke-Git @(
    "-c", "core.autocrlf=false",
    "clone", "--quiet", "--depth", "1", "--branch", $Tag, "--single-branch",
    $repositorySource, $checkout
  )
  $checkedOutCommit = (Invoke-Git @("-C", $checkout, "rev-parse", "HEAD") | Select-Object -First 1).Trim().ToLowerInvariant()
  if ($checkedOutCommit -ne $Commit) {
    throw "Checked out commit $checkedOutCommit does not match requested commit $Commit"
  }

  $latestInventoryName = [string]$contract.latest_inventory
  $inventoryProperty = $contract.inventories.PSObject.Properties[$latestInventoryName]
  if ($null -eq $inventoryProperty) {
    throw "Vendoring contract latest_inventory is missing"
  }
  $inventoryName = $null
  $inventory = $null
  if (Test-InventoryAvailable $inventoryProperty.Value $checkout) {
    $inventoryName = $latestInventoryName
    $inventory = $inventoryProperty.Value
  } else {
    foreach ($candidate in $contract.inventories.PSObject.Properties) {
      if (Test-InventoryAvailable $candidate.Value $checkout) {
        $inventoryName = [string]$candidate.Name
        $inventory = $candidate.Value
        break
      }
    }
  }
  if ($null -eq $inventory) {
    throw "Canonical tag $Tag does not satisfy any vendoring inventory"
  }

  $namespaceModules = @($contract.namespace_modules | ForEach-Object { [regex]::Escape([string]$_) })
  $namespacePattern = "\bcrate::(" + ($namespaceModules -join "|") + ")\b"
  $fileEntries = @()
  foreach ($mapping in @($inventory.files)) {
    $source = [string]$mapping.source
    $target = [string]$mapping.target
    $mode = [string]$mapping.mode
    $sourcePath = Join-Path $checkout $source
    $targetPath = Join-Path $verificationRoot $target
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $targetPath) | Out-Null

    switch ($mode) {
      "byte-copy" {
        [System.IO.File]::WriteAllBytes($targetPath, [System.IO.File]::ReadAllBytes($sourcePath))
      }
      { $_ -in @("namespace-rebase", "legacy-overlay") } {
        $sourceText = [System.IO.File]::ReadAllText($sourcePath, [System.Text.Encoding]::UTF8)
        $targetText = [regex]::Replace($sourceText, $namespacePattern, 'crate::codex::$1')
        [System.IO.File]::WriteAllText($targetPath, $targetText, $utf8NoBom)
      }
      default {
        throw "Unsupported vendoring mode '$mode' for $source"
      }
    }

    $fileEntries += [ordered]@{
      source = $source
      target = $target
      mode = $mode
      source_sha256 = Get-Sha256 $sourcePath
      target_sha256 = Get-Sha256 $targetPath
    }
  }

  New-Item -ItemType Directory -Force -Path $verificationCodex | Out-Null
  foreach ($adapter in @($contract.local_adapters)) {
    $adapterSource = Join-Path $rootPath ([string]$adapter)
    $adapterTarget = Join-Path $verificationRoot ([string]$adapter)
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $adapterTarget) | Out-Null
    [System.IO.File]::WriteAllBytes($adapterTarget, [System.IO.File]::ReadAllBytes($adapterSource))
  }

  $upstreamCommittedAt = (Invoke-Git @("-C", $checkout, "show", "-s", "--format=%cI", "HEAD") | Select-Object -First 1).Trim()
  $manifest = [ordered]@{
    schema_version = 2
    sync_version = [int]$contract.sync_version
    repository = $officialRepository
    ref = $Tag
    commit = $Commit
    upstream_committed_at = $upstreamCommittedAt
    strategy = [string]$inventory.strategy
    inventory = $inventoryName
    provenance = $provenance
    contract_sha256 = Get-Sha256 $contractPath
    files = $fileEntries
    local_adapters = @($contract.local_adapters)
  }
  $manifestJson = ($manifest | ConvertTo-Json -Depth 8) + "`n"
  [System.IO.File]::WriteAllText((Join-Path $verificationCodex "UPSTREAM.json"), $manifestJson, $utf8NoBom)

  & (Join-Path $PSScriptRoot "check-codex-rich-presence-upstream.ps1") -Root $verificationRoot -TestMode:$TestMode
  if ($LASTEXITCODE -ne 0) {
    throw "Vendored source integrity verification failed"
  }

  foreach ($mapping in @($inventory.files)) {
    $target = [string]$mapping.target
    $verifiedPath = Join-Path $verificationRoot $target
    $destinationPath = Join-Path $rootPath $target
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destinationPath) | Out-Null
    [System.IO.File]::WriteAllBytes($destinationPath, [System.IO.File]::ReadAllBytes($verifiedPath))
  }
  $manifestPath = Join-Path $codexDirectory "UPSTREAM.json"
  $manifestTemporaryPath = "$manifestPath.tmp"
  [System.IO.File]::WriteAllText($manifestTemporaryPath, $manifestJson, $utf8NoBom)
  Move-Item -Force -LiteralPath $manifestTemporaryPath -Destination $manifestPath

  & (Join-Path $PSScriptRoot "check-codex-rich-presence-upstream.ps1") -Root $rootPath -TestMode:$TestMode
  if ($LASTEXITCODE -ne 0) {
    throw "Vendored source integrity verification failed after installation"
  }

  Write-Output "Codex Rich Presence synced from $Tag at $Commit inventory=$inventoryName"
} finally {
  if (Test-Path -LiteralPath $work) {
    Remove-Item -LiteralPath $work -Recurse -Force
  }
}

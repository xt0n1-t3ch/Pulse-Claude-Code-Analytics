[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$ArtifactsDirectory,
  [Parameter(Mandatory)] [string]$OutputDirectory,
  [Parameter(Mandatory)] [string]$Tag,
  [Parameter(Mandatory)] [string]$Repository
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)

# Every entry is required. The `.sig` payloads are what the in-app updater
# verifies against the pubkey in tauri.conf.json, so a release without them
# would publish installers that Pulse can never offer as an update.
$requirements = @(
  [pscustomobject]@{ Prefix = "pulse-windows-x64-"; Suffix = ".exe" },
  [pscustomobject]@{ Prefix = "pulse-windows-x64-"; Suffix = ".exe.sig"; UpdaterTarget = "windows-x86_64" },
  [pscustomobject]@{ Prefix = "pulse-windows-x64-"; Suffix = ".msi" },
  [pscustomobject]@{ Prefix = "pulse-windows-x64"; Suffix = ".spdx.json" },
  [pscustomobject]@{ Prefix = "pulse-macos-arm64-"; Suffix = ".app.tar.gz" },
  [pscustomobject]@{ Prefix = "pulse-macos-arm64-"; Suffix = ".app.tar.gz.sig"; UpdaterTarget = "darwin-aarch64" },
  [pscustomobject]@{ Prefix = "pulse-macos-arm64-"; Suffix = ".dmg" },
  [pscustomobject]@{ Prefix = "pulse-macos-x64-"; Suffix = ".app.tar.gz" },
  [pscustomobject]@{ Prefix = "pulse-macos-x64-"; Suffix = ".app.tar.gz.sig"; UpdaterTarget = "darwin-x86_64" },
  [pscustomobject]@{ Prefix = "pulse-macos-x64-"; Suffix = ".dmg" },
  [pscustomobject]@{ Prefix = "pulse-linux-x64-"; Suffix = ".deb" },
  [pscustomobject]@{ Prefix = "pulse-linux-x64-"; Suffix = ".rpm" },
  [pscustomobject]@{ Prefix = "pulse-linux-x64-"; Suffix = ".AppImage" },
  [pscustomobject]@{ Prefix = "pulse-linux-x64-"; Suffix = ".AppImage.sig"; UpdaterTarget = "linux-x86_64" }
)

if ($Tag -notmatch '^v(?<version>(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?)$') {
  throw "Tag must be a semantic version tag such as v1.6.1"
}
$version = $Matches.version
if ($Repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
  throw "Repository must be in owner/name form"
}

$artifactsPath = (Resolve-Path -LiteralPath $ArtifactsDirectory).Path
$assets = @(Get-ChildItem -LiteralPath $artifactsPath -Recurse -File | Sort-Object Name, FullName)
if ($assets.Count -eq 0) {
  throw "No release assets were produced"
}

$selected = @()
$updaterPlatforms = [ordered]@{}
foreach ($requirement in $requirements) {
  $matches = @($assets | Where-Object {
    $_.Name.StartsWith($requirement.Prefix, [System.StringComparison]::Ordinal) -and
    $_.Name.EndsWith($requirement.Suffix, [System.StringComparison]::OrdinalIgnoreCase)
  })
  if ($matches.Count -ne 1) {
    throw "Missing required release asset prefix '$($requirement.Prefix)' suffix '$($requirement.Suffix)': found $($matches.Count)"
  }
  $selected += $matches[0]

  if ($requirement.PSObject.Properties.Name -contains "UpdaterTarget") {
    $signature = (Get-Content -Raw -LiteralPath $matches[0].FullName).Trim()
    if ([string]::IsNullOrWhiteSpace($signature)) {
      throw "Updater signature is empty: $($matches[0].Name)"
    }
    # The signed payload is the installer the .sig sits next to.
    $payloadName = $matches[0].Name.Substring(0, $matches[0].Name.Length - 4)
    $updaterPlatforms[[string]$requirement.UpdaterTarget] = [ordered]@{
      signature = $signature
      url = "https://github.com/$Repository/releases/download/$Tag/$payloadName"
    }
  }
}

$payloadNames = @($selected | ForEach-Object { $_.Name })
foreach ($platform in $updaterPlatforms.GetEnumerator()) {
  $payloadName = ([uri]$platform.Value.url).Segments[-1]
  if ($payloadNames -notcontains $payloadName) {
    throw "Updater target $($platform.Key) references an unpublished payload: $payloadName"
  }
}

$duplicateNames = @($selected | Group-Object Name | Where-Object Count -gt 1)
if ($duplicateNames.Count -gt 0) {
  throw "Release assets have duplicate names: $($duplicateNames.Name -join ', ')"
}

if (Test-Path -LiteralPath $OutputDirectory) {
  $existing = @(Get-ChildItem -LiteralPath $OutputDirectory -Force)
  if ($existing.Count -gt 0) {
    throw "Release asset output directory must be empty: $OutputDirectory"
  }
} else {
  New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
}
$outputPath = (Resolve-Path -LiteralPath $OutputDirectory).Path

foreach ($asset in $selected) {
  Copy-Item -LiteralPath $asset.FullName -Destination (Join-Path $outputPath $asset.Name)
}

# Written before the checksum pass so the manifest the updater reads is
# covered by SHA256SUMS.txt like every other published asset.
$manifest = [ordered]@{
  version = $version
  pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  notes = "Pulse $Tag. See the release notes for details."
  platforms = $updaterPlatforms
}
[System.IO.File]::WriteAllText(
  (Join-Path $outputPath "latest.json"),
  ($manifest | ConvertTo-Json -Depth 5) + "`n",
  $utf8NoBom
)

$checksumLines = @(
  Get-ChildItem -LiteralPath $outputPath -File |
    Sort-Object Name |
    ForEach-Object {
      $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
      "$hash  $($_.Name)"
    }
)
$checksumText = ($checksumLines -join "`n") + "`n"
[System.IO.File]::WriteAllText((Join-Path $outputPath "SHA256SUMS.txt"), $checksumText, $utf8NoBom)

Write-Output "Release assets prepared: files=$($selected.Count) updater-targets=$($updaterPlatforms.Count) checksums=SHA256SUMS.txt"

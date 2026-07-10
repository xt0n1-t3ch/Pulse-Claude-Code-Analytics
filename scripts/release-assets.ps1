[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$ArtifactsDirectory,
  [Parameter(Mandatory)] [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)

$requirements = @(
  [pscustomobject]@{ Prefix = "pulse-windows-x64-"; Suffix = ".exe" },
  [pscustomobject]@{ Prefix = "pulse-windows-x64-"; Suffix = ".msi" },
  [pscustomobject]@{ Prefix = "pulse-macos-arm64-"; Suffix = ".app.tar.gz" },
  [pscustomobject]@{ Prefix = "pulse-macos-arm64-"; Suffix = ".dmg" },
  [pscustomobject]@{ Prefix = "pulse-macos-x64-"; Suffix = ".app.tar.gz" },
  [pscustomobject]@{ Prefix = "pulse-macos-x64-"; Suffix = ".dmg" },
  [pscustomobject]@{ Prefix = "pulse-linux-x64-"; Suffix = ".deb" },
  [pscustomobject]@{ Prefix = "pulse-linux-x64-"; Suffix = ".rpm" },
  [pscustomobject]@{ Prefix = "pulse-linux-x64-"; Suffix = ".AppImage" }
)

$artifactsPath = (Resolve-Path -LiteralPath $ArtifactsDirectory).Path
$assets = @(Get-ChildItem -LiteralPath $artifactsPath -Recurse -File | Sort-Object Name, FullName)
if ($assets.Count -eq 0) {
  throw "No release assets were produced"
}

$selected = @()
foreach ($requirement in $requirements) {
  $matches = @($assets | Where-Object {
    $_.Name.StartsWith($requirement.Prefix, [System.StringComparison]::Ordinal) -and
    $_.Name.EndsWith($requirement.Suffix, [System.StringComparison]::OrdinalIgnoreCase)
  })
  if ($matches.Count -ne 1) {
    throw "Missing required release asset prefix '$($requirement.Prefix)' suffix '$($requirement.Suffix)': found $($matches.Count)"
  }
  $selected += $matches[0]
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

Write-Output "Release assets prepared: files=$($selected.Count) checksums=SHA256SUMS.txt"

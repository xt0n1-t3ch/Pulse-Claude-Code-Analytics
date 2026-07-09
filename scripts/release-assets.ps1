[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$ArtifactsDirectory,
  [Parameter(Mandatory = $true)]
  [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)

$artifactsPath = (Resolve-Path -LiteralPath $ArtifactsDirectory).Path
$assets = @(Get-ChildItem -LiteralPath $artifactsPath -Recurse -File | Sort-Object Name, FullName)
if ($assets.Count -eq 0) {
  throw "No release assets were produced"
}

$duplicateNames = @($assets | Group-Object Name | Where-Object Count -gt 1)
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

foreach ($asset in $assets) {
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

Write-Output "Release assets prepared: files=$($assets.Count) checksums=SHA256SUMS.txt"

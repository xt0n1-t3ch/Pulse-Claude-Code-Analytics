[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$InputDirectory,
  [Parameter(Mandatory)] [string]$OutputDirectory,
  [Parameter(Mandatory)]
  [ValidateSet("windows-x64", "macos-arm64", "macos-x64", "linux-x64")]
  [string]$Platform
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$requiredSuffixes = switch ($Platform) {
  "windows-x64" { @(".exe", ".msi") }
  { $_ -in @("macos-arm64", "macos-x64") } { @(".app.tar.gz", ".dmg") }
  "linux-x64" { @(".deb", ".rpm", ".AppImage") }
}

$inputPath = (Resolve-Path -LiteralPath $InputDirectory).Path
$files = @(Get-ChildItem -LiteralPath $inputPath -Recurse -File | Sort-Object FullName)
if ($files.Count -eq 0) {
  throw "No release assets were produced for $Platform"
}
if (-not (Test-Path -LiteralPath $OutputDirectory)) {
  New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
}
$outputPath = (Resolve-Path -LiteralPath $OutputDirectory).Path

$collected = @()
foreach ($suffix in $requiredSuffixes) {
  $matches = @($files | Where-Object { $_.Name.EndsWith($suffix, [System.StringComparison]::OrdinalIgnoreCase) })
  if ($matches.Count -ne 1) {
    throw "Missing required release asset for $Platform suffix '$suffix': found $($matches.Count)"
  }

  $destinationName = "pulse-$Platform-$($matches[0].Name)"
  $destination = Join-Path $outputPath $destinationName
  if (Test-Path -LiteralPath $destination) {
    throw "Release asset collision: $destinationName"
  }
  Copy-Item -LiteralPath $matches[0].FullName -Destination $destination
  $collected += $destinationName
}

Write-Output "Platform release assets collected: platform=$Platform files=$($collected.Count)"

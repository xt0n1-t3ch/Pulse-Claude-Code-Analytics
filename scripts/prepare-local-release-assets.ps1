[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$BundleDirectory,
  [Parameter(Mandatory)] [string]$OutputDirectory,
  [Parameter(Mandatory)] [string]$Tag
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)

$tagPattern = '^v(?<version>(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*))?)$'
if ($Tag -notmatch $tagPattern) {
  throw "Tag must be a semantic version tag such as v1.7.2 or v1.7.3-rc.1"
}
$version = $Matches.version
$bundle = (Resolve-Path -LiteralPath $BundleDirectory).Path
$output = [System.IO.Path]::GetFullPath($OutputDirectory)

if (Test-Path -LiteralPath $output) {
  throw "Local release output must not already exist: $output"
}

$required = @(
  (Join-Path $bundle "nsis/Pulse_${version}_x64-setup.exe"),
  (Join-Path $bundle "msi/Pulse_${version}_x64_en-US.msi"),
  (Join-Path $bundle "pulse-windows-x64.spdx.json")
)
foreach ($path in $required) {
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "Required local release asset is missing: $path"
  }
}

$temporary = "$output.tmp-$([guid]::NewGuid().ToString('N'))"
try {
  New-Item -ItemType Directory -Path $temporary | Out-Null
  foreach ($path in $required) {
    Copy-Item -LiteralPath $path -Destination (Join-Path $temporary ([System.IO.Path]::GetFileName($path)))
  }

  $checksumLines = @(
    Get-ChildItem -LiteralPath $temporary -File |
      Sort-Object Name |
      ForEach-Object {
        $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $($_.Name)"
      }
  )
  [System.IO.File]::WriteAllText(
    (Join-Path $temporary "SHA256SUMS.txt"),
    (($checksumLines -join "`n") + "`n"),
    $utf8NoBom
  )

  Move-Item -LiteralPath $temporary -Destination $output
  Write-Output "Local release assets prepared: version=$version files=4 output=$output"
} finally {
  if (Test-Path -LiteralPath $temporary) {
    Remove-Item -LiteralPath $temporary -Recurse -Force
  }
}

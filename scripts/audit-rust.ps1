[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$contractPath = Join-Path $PSScriptRoot "rustsec-accepted-warnings.json"
try {
  $contract = Get-Content -Raw -LiteralPath $contractPath | ConvertFrom-Json
} catch {
  throw "RustSec exception contract is invalid JSON: $($_.Exception.Message)"
}
if ($contract.schema_version -ne 1) {
  throw "RustSec exception contract must use schema_version 1"
}

$reviewAfter = [System.DateTimeOffset]::MinValue
if (-not [System.DateTimeOffset]::TryParse([string]$contract.review_after, [ref]$reviewAfter)) {
  throw "RustSec exception contract has an invalid review_after"
}
if ($reviewAfter -lt [System.DateTimeOffset]::UtcNow) {
  throw "RustSec exception contract expired on $($reviewAfter.ToString('yyyy-MM-dd'))"
}

$arguments = @("audit", "--deny", "warnings")
$seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($advisory in @($contract.advisories)) {
  $id = [string]$advisory.id
  $reason = [string]$advisory.reason
  if ($id -notmatch '^RUSTSEC-\d{4}-\d{4}$' -or [string]::IsNullOrWhiteSpace($reason)) {
    throw "RustSec exception contract contains an invalid advisory"
  }
  if (-not $seen.Add($id)) {
    throw "RustSec exception contract contains duplicate advisory $id"
  }
  $arguments += @("--ignore", $id)
}

& cargo @arguments
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

[CmdletBinding()]
param(
    [string]$Root
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Root)) {
    $Root = (Resolve-Path (Join-Path $PSScriptRoot "../.."))
}

$fixtureRoot = Join-Path $Root "tests/fixtures/providers"
$manifestPath = Join-Path $fixtureRoot "manifest.json"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Missing provider fixture manifest: $manifestPath"
}
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$requiredFixtures = @($manifest.fixtures)
if ($requiredFixtures.Count -ne 11) {
    throw "Provider fixture manifest must list 11 fixtures; found $($requiredFixtures.Count)."
}
$requiredFixtureNames = @(
    "codex_subscription", "openai_api_negative_auth", "openai_api_negative_status",
    "claude_subscription", "anthropic_api_negative_auth", "anthropic_api_negative_status",
    "hybrid", "no_data", "stale_cache", "window_removal", "discord"
)
$actualFixtureNames = @($requiredFixtures | ForEach-Object { $_.name })
if (@($actualFixtureNames | Select-Object -Unique).Count -ne $requiredFixtures.Count) {
    throw "Provider fixture manifest contains duplicate names."
}
foreach ($requiredName in $requiredFixtureNames) {
    if ($requiredName -notin $actualFixtureNames) {
        throw "Provider fixture manifest is missing '$requiredName'."
    }
}

foreach ($entry in $requiredFixtures) {
    $fixture = $entry.name
    $fixturePath = Join-Path $fixtureRoot $entry.path
    if (-not (Test-Path -LiteralPath $fixturePath -PathType Leaf)) {
        throw "Missing provider fixture asset: $fixturePath"
    }
}

$metricsRoot = Join-Path $Root "tests/fixtures/metrics"
$metricsManifestPath = Join-Path $metricsRoot "manifest.json"
if (-not (Test-Path -LiteralPath $metricsManifestPath -PathType Leaf)) {
    throw "Missing metrics fixture manifest: $metricsManifestPath"
}
$metricsManifest = Get-Content -Raw -LiteralPath $metricsManifestPath | ConvertFrom-Json
$metricEntries = @($metricsManifest.fixtures)
if ($metricEntries.Count -ne 2) {
    throw "Metrics fixture manifest must list true_zero and unavailable; found $($metricEntries.Count)."
}
$metricNames = @($metricEntries | ForEach-Object { $_.name })
if (@($metricNames | Select-Object -Unique).Count -ne 2 -or "true_zero" -notin $metricNames -or "unavailable" -notin $metricNames) {
    throw "Metrics fixture manifest must contain unique true_zero and unavailable entries."
}
foreach ($entry in $metricEntries) {
    $metricPath = Join-Path $metricsRoot $entry.path
    if (-not (Test-Path -LiteralPath $metricPath -PathType Leaf)) {
        throw "Missing metrics fixture asset: $metricPath"
    }
    $metric = Get-Content -Raw -LiteralPath $metricPath | ConvertFrom-Json
    foreach ($field in @("fixture_version", "raw_payload", "status", "source_contract", "captured_at", "expected_dto")) {
        if ($null -eq $metric.PSObject.Properties[$field]) {
            throw "Metrics fixture '$($entry.name)' is missing required field '$field'."
        }
    }
    if ([int]$metric.fixture_version -ne [int]$metricsManifest.fixture_version -or $metric.source_contract -ne $entry.source_contract) {
        throw "Metrics fixture '$($entry.name)' is not aligned with its manifest."
    }
    foreach ($field in @("cost", "cost_available", "cost_basis")) {
        if ($null -eq $metric.expected_dto.PSObject.Properties[$field]) {
            throw "Metrics fixture '$($entry.name)' expected_dto is missing '$field'."
        }
    }
    if ($entry.name -eq "true_zero" -and ($metric.expected_dto.cost -ne 0 -or $metric.expected_dto.cost_available -ne $true -or $metric.expected_dto.cost_basis -ne "exact")) {
        throw "true_zero must remain an exact available $0.00, not unavailable."
    }
    if ($entry.name -eq "unavailable" -and ($metric.expected_dto.cost -ne 0 -or $metric.expected_dto.cost_available -ne $false -or $metric.expected_dto.cost_basis -ne "unavailable")) {
        throw "unavailable must remain non-numeric despite a zero transport value."
    }
}

foreach ($entry in $requiredFixtures) {
    $fixture = $entry.name
    $fixturePath = Join-Path $fixtureRoot $entry.path
    $payload = Get-Content -Raw -LiteralPath $fixturePath | ConvertFrom-Json
    foreach ($field in @("fixture_version", "raw_payload", "status", "source_contract", "captured_at", "expected_dto")) {
        if ($null -eq $payload.PSObject.Properties[$field]) {
            throw "Fixture '$fixture' is missing required field '$field'."
        }
    }
    if ([int]$payload.fixture_version -ne [int]$manifest.fixture_version) {
        throw "Fixture '$fixture' version does not match the manifest."
    }
    if ($payload.source_contract -ne $entry.source_contract) {
        throw "Fixture '$fixture' source_contract does not match the manifest."
    }

    if ($payload.source_contract -eq "discord_ipc.v1") {
        $events = @($payload.expected_dto.events)
        if ($events.Count -lt 2 -or $payload.expected_dto.proof_requirements.set_activity -ne $true -or $payload.expected_dto.proof_requirements.clear_activity -ne $true) {
            throw "Discord fixture must require SET_ACTIVITY and clear_activity proof."
        }
        if (@($events | Where-Object { [string]::IsNullOrWhiteSpace([string]$_.nonce) }).Count -gt 0) {
            throw "Discord fixture events must carry non-empty correlation nonces."
        }
        continue
    }
    $routes = if ($null -ne $payload.expected_dto.PSObject.Properties["routes"]) {
        @($payload.expected_dto.routes)
    } else {
        @($payload.expected_dto)
    }
    $allowedKinds = switch ($payload.source_contract) {
        "codex_subscription.v1" { @("codex_subscription") }
        "open_ai_api.v1" { @("open_ai_api") }
        "claude_subscription.v1" { @("claude_subscription") }
        "anthropic_api.v1" { @("anthropic_api") }
        "hybrid.v1" { @("codex_subscription", "claude_subscription") }
        "aggregate.v1" { @() }
        "cache.v1" { @("claude_subscription") }
        "window_delta.v1" { @("codex_subscription") }
        default { throw "Fixture '$fixture' has unsupported source_contract '$($payload.source_contract)'." }
    }
    $actualKinds = @($routes | ForEach-Object { $_.source.kind } | Sort-Object -Unique)
    if ((Compare-Object -ReferenceObject @($allowedKinds) -DifferenceObject $actualKinds).Count -ne 0) {
        throw "Fixture '$fixture' source kinds do not match contract '$($payload.source_contract)'."
    }
    foreach ($route in $routes) {
        foreach ($field in @("source", "availability", "freshness", "provenance", "observed_at", "fetched_at", "expires_at", "windows", "credits", "extra_usage", "error")) {
            if ($null -eq $route.PSObject.Properties[$field]) {
                throw "Fixture '$fixture' route is missing DTO field '$field'."
            }
        }
        if ($route.source.kind -notin @("codex_subscription", "open_ai_api", "claude_subscription", "anthropic_api")) {
            throw "Fixture '$fixture' has an unknown access source kind."
        }
        if ($route.availability -notin @("available", "unavailable")) {
            throw "Fixture '$fixture' has an unknown availability value."
        }
        if ($route.freshness -notin @("fresh", "stale", "unknown")) {
            throw "Fixture '$fixture' has an unknown freshness value."
        }
        if ($route.provenance -notin @("app_server", "provider_api", "memory_cache", "session_jsonl", "none")) {
            throw "Fixture '$fixture' has an unknown provenance value."
        }
        if ($null -ne $route.error -and $route.error -isnot [string]) {
            throw "Fixture '$fixture' route error must be a string or null."
        }
        if ($null -ne $route.credits) {
            foreach ($field in @("balance", "has_credits", "unlimited")) {
                if ($null -eq $route.credits.PSObject.Properties[$field]) {
                    throw "Fixture '$fixture' credits is missing '$field'."
                }
            }
        }
        if ($null -ne $route.extra_usage) {
            foreach ($field in @("enabled", "limit", "used", "utilization")) {
                if ($null -eq $route.extra_usage.PSObject.Properties[$field]) {
                    throw "Fixture '$fixture' extra_usage is missing '$field'."
                }
            }
        }
        foreach ($window in @($route.windows)) {
            foreach ($field in @("key", "label", "window_minutes", "used_percent", "remaining_percent", "resets_at")) {
                if ($null -eq $window.PSObject.Properties[$field]) {
                    throw "Fixture '$fixture' window is missing '$field'."
                }
            }
        }
    }
}

$runnerPath = Join-Path $Root "scripts/e2e/run-pulse.ps1"
if (-not (Test-Path -LiteralPath $runnerPath -PathType Leaf)) {
    throw "Missing central Pulse runner: $runnerPath"
}
$runnerSource = Get-Content -Raw -LiteralPath $runnerPath
foreach ($requiredToken in @("RunPlaywright", "WEBVIEW2_USER_DATA_FOLDER", "PULSE_TAURI_CDP_URL")) {
    if ($runnerSource -notlike "*$requiredToken*") {
        throw "Pulse runner is missing the native Tauri E2E contract token '$requiredToken'."
    }
}

Write-Output ("PASS provider-contract fixtures={0}" -f $requiredFixtures.Count)

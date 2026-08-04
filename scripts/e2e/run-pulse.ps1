[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Browser", "Tauri", "Discord")]
    [string]$Mode,

    [string]$Fixture,
    [switch]$DryRun,
    [switch]$RunPlaywright,
    [int]$StartupTimeoutSeconds = 20,
    [string]$DiscordProofPath,
    [string]$DiscordProofOutput
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$fixtureRoot = Join-Path $repoRoot "tests/fixtures/providers"
$manifestPath = Join-Path $fixtureRoot "manifest.json"
$runId = [guid]::NewGuid().ToString("N")
$runRoot = Join-Path ([IO.Path]::GetTempPath()) "pulse-e2e-$runId"
$ownedProcesses = [System.Collections.Generic.List[object]]::new()
$environmentBackup = @{}
$port = $null
$browserPort = 1420
$bridgePort = 1421

function Fail([string]$Message) {
    throw "PULSE_E2E_BLOCKED: $Message"
}

function Invoke-ContractProbe {
    $probePath = Join-Path $PSScriptRoot "test-pulse-contract.ps1"
    if (-not (Test-Path -LiteralPath $probePath -PathType Leaf)) {
        Fail "central contract probe is missing: $probePath"
    }
    $hostCommand = (Get-Command pwsh.exe -ErrorAction SilentlyContinue)
    if ($null -eq $hostCommand) { $hostCommand = Get-Command powershell.exe -ErrorAction SilentlyContinue }
    if ($null -eq $hostCommand) { Fail "PowerShell host is unavailable for the fixture contract probe" }
    $probeOutput = & $hostCommand.Source -NoProfile -ExecutionPolicy Bypass -File $probePath -Root $repoRoot 2>&1
    if ($LASTEXITCODE -ne 0) {
        Fail "fixture contract probe failed: $($probeOutput -join ' ')"
    }
    $probeOutput | ForEach-Object { Write-Output $_ }
}

function Get-Manifest {
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        Fail "fixture manifest is missing: $manifestPath"
    }
    try {
        return Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
    } catch {
        Fail "fixture manifest is not valid JSON: $manifestPath ($($_.Exception.Message))"
    }
}

function Get-Fixture([object]$Manifest) {
    $fixtureName = $Fixture
    if ([string]::IsNullOrWhiteSpace($fixtureName)) {
        $fixtureName = if ($Mode -eq "Discord") { "discord" } else { "no_data" }
    }

    $entry = @($Manifest.fixtures | Where-Object { $_.name -eq $fixtureName }) | Select-Object -First 1
    if ($null -eq $entry) {
        Fail "fixture '$fixtureName' is not listed in $manifestPath"
    }

    $fixturePath = Join-Path $fixtureRoot $entry.path
    if (-not (Test-Path -LiteralPath $fixturePath -PathType Leaf)) {
        Fail "fixture asset is missing: $fixturePath"
    }
    try {
        $fixture = Get-Content -Raw -LiteralPath $fixturePath | ConvertFrom-Json
    } catch {
        Fail "fixture '$fixtureName' is not valid JSON: $fixturePath ($($_.Exception.Message))"
    }

    foreach ($field in @("fixture_version", "raw_payload", "status", "source_contract", "captured_at", "expected_dto")) {
        if ($null -eq $fixture.PSObject.Properties[$field]) {
            Fail "fixture '$fixtureName' is missing required field '$field'"
        }
    }
    if ([int]$fixture.fixture_version -ne [int]$Manifest.fixture_version) {
        Fail "fixture '$fixtureName' version $($fixture.fixture_version) does not match manifest version $($Manifest.fixture_version)"
    }
    if ($fixture.source_contract -ne $entry.source_contract) {
        Fail "fixture '$fixtureName' source_contract '$($fixture.source_contract)' does not match manifest '$($entry.source_contract)'"
    }
    try {
        [DateTimeOffset]::Parse($fixture.captured_at) | Out-Null
    } catch {
        Fail "fixture '$fixtureName' captured_at is not an ISO-8601 timestamp"
    }

    # Synthetic fixtures must never turn into a credential transport by accident.
    $raw = Get-Content -Raw -LiteralPath $fixturePath
    if ($raw -match '(?i)sk-[A-Za-z0-9]{8,}|ghp_[A-Za-z0-9]{8,}|Bearer\s+[A-Za-z0-9._-]{8,}|"(access|refresh|session)_?token"\s*:\s*"[^" ]{8,}"') {
        Fail "fixture '$fixtureName' contains a credential-shaped value"
    }

    return [pscustomobject]@{ Name = $fixtureName; Path = $fixturePath; Entry = $entry; Data = $fixture }
}

function Set-TestEnvironment([string]$Name, [string]$Value) {
    if (-not $environmentBackup.ContainsKey($Name)) {
        $environmentBackup[$Name] = if (Test-Path "Env:$Name") { [Environment]::GetEnvironmentVariable($Name) } else { $null }
    }
    [Environment]::SetEnvironmentVariable($Name, $Value, "Process")
}

function Restore-TestEnvironment {
    foreach ($name in $environmentBackup.Keys) {
        [Environment]::SetEnvironmentVariable($name, $environmentBackup[$name], "Process")
    }
}

function Assert-RepoOwnedPath([string]$Path, [string]$Label) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    $rootWithSlash = $repoRoot.TrimEnd('\') + '\'
    if (-not $fullPath.StartsWith($rootWithSlash, [StringComparison]::OrdinalIgnoreCase)) {
        Fail "$Label must stay inside the repository: $fullPath"
    }
    return $fullPath
}

function New-FreePort {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([Net.IPEndPoint]$listener.LocalEndpoint).Port
    } finally {
        $listener.Stop()
    }
}

function Get-ProcessIdentity([int]$ProcessId) {
    $cim = Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId" -ErrorAction SilentlyContinue
    if ($null -eq $cim) { return $null }
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    $created = $null
    if ($null -ne $process) {
        try { $created = $process.StartTime.ToUniversalTime() } catch { }
    }
    if ($null -eq $created -and $cim.CreationDate) {
        try { $created = [Management.ManagementDateTimeConverter]::ToDateTime($cim.CreationDate).ToUniversalTime() } catch { }
    }
    return [pscustomobject]@{
        Pid = $ProcessId
        CreationTime = $created
        ExecutablePath = [string]$cim.ExecutablePath
        CommandLine = [string]$cim.CommandLine
        ParentPid = [int]$cim.ParentProcessId
    }
}

function Assert-ProcessIdentity([object]$Expected, [switch]$AllowExecutableDrift) {
    $actual = Get-ProcessIdentity $Expected.Pid
    if ($null -eq $actual) { Fail "owned process $($Expected.Pid) exited before verification" }
    if ($null -ne $Expected.CreationTime -and $null -ne $actual.CreationTime -and $Expected.CreationTime -ne $actual.CreationTime) {
        Fail "PID $($Expected.Pid) creation time changed; refusing to touch a reused PID"
    }
    if (-not $AllowExecutableDrift -and $Expected.ExecutablePath -and $actual.ExecutablePath -and
        -not $actual.ExecutablePath.Equals($Expected.ExecutablePath, [StringComparison]::OrdinalIgnoreCase)) {
        Fail "PID $($Expected.Pid) executable identity changed; refusing cleanup"
    }
    if ($Expected.CommandLine -and $actual.CommandLine -and $actual.CommandLine -ne $Expected.CommandLine) {
        Fail "PID $($Expected.Pid) command line changed; refusing cleanup"
    }
    return $actual
}

function Start-OwnedProcess([string]$FilePath, [string[]]$Arguments, [string]$WorkingDirectory, [string]$Label) {
    if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) {
        Fail "$Label executable is missing: $FilePath"
    }
    $stdout = Join-Path $runRoot "$Label.stdout.log"
    $stderr = Join-Path $runRoot "$Label.stderr.log"
    $startParams = @{
        FilePath = $FilePath
        WorkingDirectory = $WorkingDirectory
        PassThru = $true
        WindowStyle = "Hidden"
        RedirectStandardOutput = $stdout
        RedirectStandardError = $stderr
    }
    if ($Arguments.Count -gt 0) {
        $startParams.ArgumentList = $Arguments
    }
    $process = Start-Process @startParams
    Start-Sleep -Milliseconds 300
    $identity = Get-ProcessIdentity $process.Id
    if ($null -eq $identity) { Fail "$Label process disappeared before identity verification" }
    $ownedProcesses.Add($identity)
    Write-Output ("PROCESS label={0} pid={1} created={2:o} exe={3} command={4}" -f $Label, $identity.Pid, $identity.CreationTime, $identity.ExecutablePath, $identity.CommandLine)
    return [pscustomobject]@{ Identity = $identity; Stdout = $stdout; Stderr = $stderr }
}

function Test-ProcessAlive([object]$Identity) {
    return $null -ne (Get-ProcessIdentity $Identity.Pid)
}

function Test-ProcessDescendant([int]$ProcessId, [int]$AncestorPid) {
    $cursor = Get-ProcessIdentity $ProcessId
    $seen = @{}
    while ($null -ne $cursor -and $cursor.ParentPid -gt 0 -and -not $seen.ContainsKey($cursor.Pid)) {
        if ($cursor.ParentPid -eq $AncestorPid) { return $true }
        $seen[$cursor.Pid] = $true
        $cursor = Get-ProcessIdentity $cursor.ParentPid
    }
    return $false
}

function Wait-Http([string]$Uri, [object]$Identity, [string]$Label) {
    $deadline = (Get-Date).AddSeconds($StartupTimeoutSeconds)
    do {
        if (-not (Test-ProcessAlive $Identity)) {
            Fail "$Label exited before startup probe; inspect $runRoot"
        }
        try {
            $response = Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 -Uri $Uri
            if ([int]$response.StatusCode -ge 200 -and [int]$response.StatusCode -lt 500) {
                Write-Output ("PROBE label={0} uri={1} status={2}" -f $Label, $Uri, $response.StatusCode)
                return
            }
        } catch { }
        Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline)
    Fail "$Label startup probe timed out at $Uri; no backend endpoint is inferred"
}

function Assert-PortOwned([int]$Port, [object]$ParentIdentity, [string]$Label) {
    $deadline = (Get-Date).AddSeconds($StartupTimeoutSeconds)
    do {
        $connections = @(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
        foreach ($connection in $connections) {
            $owner = Get-ProcessIdentity ([int]$connection.OwningProcess)
            if ($null -eq $owner) { continue }
            $isParent = $owner.Pid -eq $ParentIdentity.Pid
            $isDescendant = Test-ProcessDescendant $owner.Pid $ParentIdentity.Pid
            if ($isParent -or $isDescendant) {
                if (-not (@($ownedProcesses | Where-Object { $_.Pid -eq $owner.Pid }).Count)) {
                    $ownedProcesses.Add($owner)
                }
                Write-Output ("PORT_OWNER label={0} port={1} pid={2} command={3}" -f $Label, $Port, $owner.Pid, $owner.CommandLine)
                return
            }
        }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)
    Fail "$Label did not establish an owned listener on loopback port $Port"
}

function Initialize-TestState([object]$FixtureInfo) {
    if ($Mode -ne "Discord" -and $FixtureInfo.Name -ne "no_data") {
        Fail "fixture '$($FixtureInfo.Name)' is contract-only; the current runtime can materialize only no_data without a provider-specific E2E adapter"
    }
    New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
    $claudeHome = Join-Path $runRoot "claude-home"
    $codexHome = Join-Path $runRoot "codex-home"
    New-Item -ItemType Directory -Force -Path $claudeHome, $codexHome | Out-Null
    $fixtureCopy = Join-Path $runRoot "provider-fixture.json"
    Copy-Item -LiteralPath $FixtureInfo.Path -Destination $fixtureCopy
    Copy-Item -LiteralPath $FixtureInfo.Path -Destination (Join-Path $claudeHome "provider-fixture.json")
    Copy-Item -LiteralPath $FixtureInfo.Path -Destination (Join-Path $codexHome "provider-fixture.json")
    $providerConfig = [ordered]@{
        schema_version = 1
        run_id = $runId
        mode = $Mode
        fixture_name = $FixtureInfo.Name
        fixture_path = $fixtureCopy
        claude_home = $claudeHome
        codex_home = $codexHome
        analytics_db = (Join-Path $runRoot "pulse-analytics.db")
    }
    $providerConfigPath = Join-Path $runRoot "provider-config.json"
    $providerConfig | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath $providerConfigPath

    Set-TestEnvironment "CLAUDE_HOME" $claudeHome
    Set-TestEnvironment "CODEX_HOME" $codexHome
    Set-TestEnvironment "PULSE_ANALYTICS_DB" $providerConfig.analytics_db
    Set-TestEnvironment "PULSE_PROVIDER_FIXTURE" $fixtureCopy
    Set-TestEnvironment "PULSE_PROVIDER_CONFIG" $providerConfigPath
    Set-TestEnvironment "PULSE_E2E_RUN_ID" $runId
    Write-Output ("STATE run_root={0} claude_home={1} codex_home={2} db={3} fixture={4}" -f $runRoot, $claudeHome, $codexHome, $providerConfig.analytics_db, $FixtureInfo.Name)
}

function Invoke-BrowserMode {
    $frontendRoot = Assert-RepoOwnedPath (Join-Path $repoRoot "frontend") "frontend root"
    $packageJson = Join-Path $frontendRoot "package.json"
    if (-not (Test-Path -LiteralPath $packageJson -PathType Leaf)) { Fail "Browser mode requires frontend/package.json" }
    $bun = (Get-Command bun.exe -ErrorAction SilentlyContinue).Source
    if ([string]::IsNullOrWhiteSpace($bun)) { Fail "Browser mode requires bun.exe on PATH" }
    $port = $browserPort
    $started = Start-OwnedProcess $bun @("--cwd", $frontendRoot, "run", "dev") $repoRoot "browser"
    Assert-PortOwned $bridgePort $started.Identity "browser-bridge"
    Assert-PortOwned $browserPort $started.Identity "browser-vite"
    Wait-Http "http://127.0.0.1:$browserPort/" $started.Identity "browser-vite"
    Write-Output ("READY mode=Browser port={0} bridge={1} url=http://127.0.0.1:{0}/" -f $browserPort, $bridgePort)
}

function Invoke-TauriMode {
    # Pulse is a workspace member, so Cargo's canonical target directory lives
    # at the repository root rather than beneath src-tauri/.
    $binary = Assert-RepoOwnedPath (Join-Path $repoRoot "target/debug/pulse.exe") "Tauri debug binary"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        Fail "Tauri mode requires repo-owned target/debug/pulse.exe; build it first. Installed pulse.exe is never inspected or touched."
    }

    # The debug binary owns native IPC and its background poller. Tauri mode
    # therefore starts Vite only; launching the browser bridge here would create
    # a second producer against the same isolated database.
    $frontendRoot = Assert-RepoOwnedPath (Join-Path $repoRoot "frontend") "frontend root"
    $packageJson = Join-Path $frontendRoot "package.json"
    if (-not (Test-Path -LiteralPath $packageJson -PathType Leaf)) { Fail "Tauri mode requires frontend/package.json" }
    $bun = (Get-Command bun.exe -ErrorAction SilentlyContinue).Source
    if ([string]::IsNullOrWhiteSpace($bun)) { Fail "Tauri mode requires bun.exe on PATH" }
    $frontend = Start-OwnedProcess $bun @("run", "dev:ui") $frontendRoot "tauri-frontend"
    Assert-PortOwned $browserPort $frontend.Identity "tauri-vite"
    Wait-Http "http://localhost:$browserPort/" $frontend.Identity "tauri-vite"

    $port = New-FreePort
    $oldWebViewArgs = if (Test-Path "Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") { [Environment]::GetEnvironmentVariable("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") } else { $null }
    $webViewArgs = ((@($oldWebViewArgs) | Where-Object { $_ }) + "--remote-debugging-port=$port") -join " "
    Set-TestEnvironment "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS" $webViewArgs
    # WebView2 shares the installed Pulse profile by default. Isolate the
    # repo-owned probe so its CDP options cannot conflict with PID 14932.
    $webViewData = Join-Path $runRoot "webview2-user-data"
    New-Item -ItemType Directory -Force -Path $webViewData | Out-Null
    Set-TestEnvironment "WEBVIEW2_USER_DATA_FOLDER" $webViewData
    $started = Start-OwnedProcess $binary @() $repoRoot "tauri"
    Assert-PortOwned $port $started.Identity "tauri-cdp"
    Wait-Http "http://127.0.0.1:$port/json/version" $started.Identity "tauri-cdp"
    Write-Output ("READY mode=Tauri cdp=http://127.0.0.1:{0}/json/version" -f $port)

    if ($RunPlaywright) {
        $npm = (Get-Command npm.cmd -ErrorAction SilentlyContinue).Source
        if ([string]::IsNullOrWhiteSpace($npm)) { Fail "Tauri Playwright mode requires npm.cmd on PATH" }
        Set-TestEnvironment "PULSE_TAURI_CDP_URL" ("http://127.0.0.1:{0}/json/version" -f $port)
        Write-Output ("PLAYWRIGHT_START mode=Tauri endpoint=http://127.0.0.1:{0}/json/version" -f $port)
        & $npm --prefix $frontendRoot run test:e2e:tauri
        if ($LASTEXITCODE -ne 0) {
            Fail "Tauri Playwright E2E failed with exit code $LASTEXITCODE"
        }
        Write-Output "PLAYWRIGHT_PASS mode=Tauri"
    }
}

function Get-DiscordProcesses {
    @(Get-Process -Name "Discord", "DiscordCanary", "DiscordPTB" -ErrorAction SilentlyContinue)
}

function Invoke-DiscordMode([object]$FixtureInfo) {
    if ($env:PULSE_DISCORD_LIVE -ne "1") {
        Fail "Discord mode is opt-in; set PULSE_DISCORD_LIVE=1. The runner never restarts Discord."
    }
    if ($FixtureInfo.Name -ne "discord") { Fail "Discord mode requires the discord fixture" }
    if ((Get-DiscordProcesses).Count -eq 0) { Fail "Discord desktop is not running; start it manually. The runner never restarts Discord." }
    if ([string]::IsNullOrWhiteSpace($DiscordProofPath)) { $DiscordProofPath = $env:PULSE_DISCORD_PROOF }
    if ([string]::IsNullOrWhiteSpace($DiscordProofPath) -or -not (Test-Path -LiteralPath $DiscordProofPath -PathType Leaf)) {
        Fail "Discord mode requires -DiscordProofPath (or PULSE_DISCORD_PROOF) containing raw SET_ACTIVITY and clear_activity events"
    }
    try { $proof = Get-Content -Raw -LiteralPath $DiscordProofPath | ConvertFrom-Json } catch { Fail "Discord proof is not valid JSON: $DiscordProofPath" }
    # The checked-in Discord fixture wraps raw events under raw_payload; a live
    # observer may provide the same events at the document root.  Accept both
    # envelopes while keeping the SET_ACTIVITY/clear proof checks identical.
    $events = if ($null -ne $proof.events) {
        @($proof.events)
    } elseif ($null -ne $proof.raw_payload -and $null -ne $proof.raw_payload.events) {
        @($proof.raw_payload.events)
    } else {
        @()
    }
    $setEvent = @($events | Where-Object {
        $_.command -eq "SET_ACTIVITY" -and $null -ne $_.args.activity -and
        -not [string]::IsNullOrWhiteSpace([string]$_.nonce)
    })
    $clearEvent = @($events | Where-Object {
        $_.command -eq "SET_ACTIVITY" -and $_.clear_activity -eq $true -and
        $null -ne $_.args -and $null -eq $_.args.activity -and
        -not [string]::IsNullOrWhiteSpace([string]$_.nonce)
    })
    if ($setEvent.Count -eq 0 -and $null -ne $proof.outbound_set_activity) {
        $setEvent = @($proof.outbound_set_activity) | Where-Object {
            $_.cmd -eq "SET_ACTIVITY" -and $null -ne $_.args.activity -and -not [string]::IsNullOrWhiteSpace([string]$_.nonce)
        }
    }
    if ($clearEvent.Count -eq 0 -and $null -ne $proof.clear_activity_reply) {
        $clearEvent = @($proof.clear_activity_reply) | Where-Object {
            $_.cmd -eq "SET_ACTIVITY" -and $null -eq $_.data -and -not [string]::IsNullOrWhiteSpace([string]$_.nonce)
        }
    }
    if ($setEvent.Count -eq 0) { Fail "Discord proof has no raw SET_ACTIVITY event with an activity payload and nonce" }
    if ($clearEvent.Count -eq 0) { Fail "Discord proof has no raw SET_ACTIVITY clear reply with null data and nonce" }
    $validatedEvents = @($setEvent) + @($clearEvent)
    if ([string]::IsNullOrWhiteSpace($DiscordProofOutput)) {
        $DiscordProofOutput = Join-Path ([IO.Path]::GetTempPath()) "pulse-e2e-discord-proof-$runId.json"
    }
    $proofDir = Split-Path -Parent ([IO.Path]::GetFullPath($DiscordProofOutput))
    New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
    [ordered]@{
        schema_version = 1
        run_id = $runId
        source = "raw-discord-ipc-proof"
        captured_at = (Get-Date).ToUniversalTime().ToString("o")
        events = $validatedEvents
        validation = [ordered]@{ set_activity = $true; clear_activity = $true }
    } | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8 -LiteralPath $DiscordProofOutput
    Write-Output ("PROOF_ARTIFACT mode=Discord path={0}" -f ([IO.Path]::GetFullPath($DiscordProofOutput)))
    Write-Output "READY mode=Discord raw SET_ACTIVITY + clear_activity validated; Discord process was not restarted"
}

function Stop-OwnedProcesses {
    foreach ($expected in @($ownedProcesses | Sort-Object -Property Pid -Descending)) {
        $actual = Get-ProcessIdentity $expected.Pid
        if ($null -eq $actual) { continue }
        try {
            Assert-ProcessIdentity $expected | Out-Null
        } catch {
            # A WebView2 child may exit naturally after Playwright reports a
            # failed assertion; re-check before treating that race as drift.
            if ($null -eq (Get-ProcessIdentity $expected.Pid)) { continue }
            Write-Warning $_.Exception.Message
            continue
        }
        Stop-Process -Id $expected.Pid -ErrorAction SilentlyContinue
        try { Wait-Process -Id $expected.Pid -Timeout 5 -ErrorAction SilentlyContinue } catch { }
        if ($null -ne (Get-ProcessIdentity $expected.Pid)) {
            try {
                Assert-ProcessIdentity $expected | Out-Null
                Stop-Process -Id $expected.Pid -Force -ErrorAction SilentlyContinue
            } catch { Write-Warning $_.Exception.Message }
        }
    }
}

$manifest = $null
$fixtureInfo = $null
try {
    if ($RunPlaywright -and $Mode -ne "Tauri") {
        Fail "-RunPlaywright is supported only with -Mode Tauri"
    }
    Invoke-ContractProbe
    $manifest = Get-Manifest
    $fixtureInfo = Get-Fixture $manifest
    Initialize-TestState $fixtureInfo

    if ($DryRun) {
        switch ($Mode) {
            "Browser" {
                if (-not (Test-Path -LiteralPath (Join-Path $repoRoot "frontend/package.json") -PathType Leaf)) { Fail "Browser mode requires frontend/package.json" }
                if ($null -eq (Get-Command bun.exe -ErrorAction SilentlyContinue)) { Fail "Browser mode requires bun.exe on PATH" }
                Write-Output ("PORT_CONTRACT mode=Browser vite={0} bridge={1} launcher=bun --cwd frontend run dev" -f $browserPort, $bridgePort)
            }
            "Tauri" {
                $null = Assert-RepoOwnedPath (Join-Path $repoRoot "target/debug/pulse.exe") "Tauri debug binary"
                if (-not (Test-Path -LiteralPath (Join-Path $repoRoot "target/debug/pulse.exe") -PathType Leaf)) { Fail "Tauri mode requires repo-owned debug binary; no installed pulse.exe is touched" }
            }
            "Discord" {
                if ($env:PULSE_DISCORD_LIVE -ne "1") { Fail "Discord mode is opt-in; set PULSE_DISCORD_LIVE=1" }
            }
        }
        Write-Output ("DRY_RUN mode={0} fixture={1} state={2}" -f $Mode, $fixtureInfo.Name, $runRoot)
    } else {
        switch ($Mode) {
            "Browser" { Invoke-BrowserMode }
            "Tauri" { Invoke-TauriMode }
            "Discord" { Invoke-DiscordMode $fixtureInfo }
        }
    }
} finally {
    Stop-OwnedProcesses
    Restore-TestEnvironment
    if (Test-Path -LiteralPath $runRoot -PathType Container) {
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
        $fullRunRoot = [IO.Path]::GetFullPath($runRoot)
        if ($fullRunRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and $fullRunRoot -match 'pulse-e2e-[0-9a-f]{32}$') {
            Remove-Item -LiteralPath $fullRunRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

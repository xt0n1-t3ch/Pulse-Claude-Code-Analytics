# Stop cc-discord-presence daemon (Rust v2, Windows)
# The binary uses file locks, so we just need to find and terminate the process.

$ClaudeDir = Join-Path $env:USERPROFILE ".claude"
$MetaFile = Join-Path $ClaudeDir "cc-discord-presence.instance.json"

$Stopped = $false

# Try to read PID from instance metadata (Rust v2 writes this file)
if (Test-Path $MetaFile) {
    try {
        $Meta = Get-Content $MetaFile -Raw | ConvertFrom-Json
        $Pid = $Meta.pid
        if ($Pid) {
            $Process = Get-Process -Id $Pid -ErrorAction SilentlyContinue
            if ($Process) {
                Stop-Process -Id $Pid -Force -ErrorAction SilentlyContinue
                $Stopped = $true
                Write-Host "cc-discord-presence stopped (PID: $Pid)"
            }
        }
    } catch {
        # Metadata parse failed, fall through to name-based kill
    }
}

if (-not $Stopped) {
    # Fallback: kill by process name
    $Processes = Get-Process -Name "cc-discord-presence*" -ErrorAction SilentlyContinue
    if ($Processes) {
        $Processes | Stop-Process -Force
        Write-Host "cc-discord-presence stopped (fallback)"
    }
}

# Clean up legacy v1 files
Remove-Item (Join-Path $ClaudeDir "discord-presence.pid") -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $ClaudeDir "discord-presence.refcount") -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $ClaudeDir "discord-presence-sessions") -Recurse -Force -ErrorAction SilentlyContinue

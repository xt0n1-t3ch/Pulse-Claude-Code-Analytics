# Start cc-discord-presence daemon (Rust v2, Windows)
# The binary handles single-instance locking with auto-takeover internally.

$ErrorActionPreference = "Stop"

$ClaudeDir = Join-Path $env:USERPROFILE ".claude"
$BinDir = Join-Path $ClaudeDir "bin"
$Repo = "tsanva/cc-discord-presence"
$Version = "v2.0.0"

# Ensure directories exist
New-Item -ItemType Directory -Path $ClaudeDir -Force | Out-Null
New-Item -ItemType Directory -Path $BinDir -Force | Out-Null

$BinaryName = "cc-discord-presence-windows-amd64.exe"
$Binary = Join-Path $BinDir $BinaryName

# Download binary if not present
if (-not (Test-Path $Binary)) {
    Write-Host "Downloading cc-discord-presence for windows-amd64..."

    $DownloadUrl = "https://github.com/$Repo/releases/download/$Version/$BinaryName"

    try {
        Invoke-WebRequest -Uri $DownloadUrl -OutFile $Binary -UseBasicParsing
        Write-Host "Downloaded successfully!"
    } catch {
        Write-Error "Failed to download binary: $_"
        exit 1
    }
}

if (-not (Test-Path $Binary)) {
    Write-Error "Error: Binary not found at $Binary"
    exit 1
}

# Start the daemon in background (hidden window).
# The binary handles single-instance locking and auto-takeover of any existing instance.
Start-Process -FilePath $Binary -WindowStyle Hidden

Write-Host "cc-discord-presence started"

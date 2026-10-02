# Builds the workspace and installs sa_bridge.asi into the GTA San Andreas directory.
# Usage: ./scripts/deploy.ps1 [-GameDir <path>] [-Debug]
param(
    [string]$GameDir = $env:GTA_SA_DIR,
    [switch]$Debug
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if (-not $GameDir) { throw 'Pass -GameDir or set GTA_SA_DIR to the folder containing gta_sa.exe' }
if (-not (Test-Path (Join-Path $GameDir 'gta_sa.exe'))) { throw "gta_sa.exe not found in $GameDir" }
if (Get-Process gta_sa -ErrorAction SilentlyContinue) { throw 'gta_sa.exe is running; close it first (the .asi is locked while loaded)' }

$profileName = if ($Debug) { 'debug' } else { 'release' }
Push-Location $root
try {
    if ($Debug) { cargo build } else { cargo build --release }
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
} finally { Pop-Location }

$dll = Join-Path $root "target/i686-pc-windows-msvc/$profileName/sa_bridge.dll"
$dest = Join-Path $GameDir 'sa_bridge.asi'
Copy-Item $dll $dest -Force
Write-Host "installed $dest"

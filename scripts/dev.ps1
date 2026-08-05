[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'invoke-in-directory.ps1')

$root = Split-Path -Parent $PSScriptRoot
$frontend = Join-Path $root 'src\frontend'
$backend = Join-Path $root 'src\backend'
$tauri = Join-Path $frontend 'node_modules\.bin\tauri.cmd'

if (-not (Test-Path (Join-Path $frontend 'node_modules'))) {
    Invoke-InDirectory $frontend 'npm install' { npm install --no-audit --no-fund }
}

Push-Location $backend
try {
    & $tauri dev
} finally {
    Pop-Location
}

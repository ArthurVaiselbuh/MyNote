[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'invoke-in-directory.ps1')

$root = Split-Path -Parent $PSScriptRoot
$frontend = Join-Path $root 'src\frontend'
$backend = Join-Path $root 'src\backend'
$tauri = Join-Path $frontend 'node_modules\.bin\tauri.cmd'
$outputDir = Join-Path $root 'output'
$builtExe = Join-Path $backend 'target\release\MyNote.exe'
$releasedExe = Join-Path $outputDir 'MyNote.exe'

Write-Host '=== MyNote build ==='

Write-Host '[1/2] npm install'
Invoke-InDirectory $frontend 'npm install' { npm install --no-audit --no-fund }

Write-Host '[2/2] tauri build (runs frontend build, then cargo release)'
Invoke-InDirectory $backend 'tauri build' { & $tauri build --no-bundle }

New-Item -ItemType Directory -Force $outputDir | Out-Null
Copy-Item $builtExe $releasedExe -Force

Write-Host ''
Write-Host "Done: $releasedExe"

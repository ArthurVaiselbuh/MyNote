[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'invoke-in-directory.ps1')

$root = Split-Path -Parent $PSScriptRoot
$frontend = Join-Path $root 'src\frontend'
$backend = Join-Path $root 'src\backend'
$e2e = Join-Path $root 'src\e2e'
$releasedExe = Join-Path $root 'output\MyNote.exe'

Write-Host '=== MyNote tests ==='

if (-not (Test-Path (Join-Path $frontend 'node_modules'))) {
    Invoke-InDirectory $frontend 'npm install' { npm install --no-audit --no-fund }
}

Write-Host '[1/4] svelte-check'
Invoke-InDirectory $frontend 'svelte-check' { npm run check }

Write-Host '[2/4] vitest'
Invoke-InDirectory $frontend 'vitest' { npm test }

Write-Host '[3/4] cargo test'
Invoke-InDirectory $backend 'cargo test' { cargo test }

if (-not (Test-Path $releasedExe)) {
    throw "[4/4] skipped: $releasedExe missing - run scripts\build.ps1 first"
}

Write-Host '[4/4] e2e against output\MyNote.exe'
Invoke-InDirectory $e2e 'e2e' { npm test }

Write-Host ''
Write-Host 'All tests passed.'

<#
    Bumps MyNote one minor version (1.2 -> 1.3, 1.9 -> 1.10) across every file
    that carries it. The commit this produces is the anchor ci_version.sh
    counts from, so the CI patch number restarts at 0 after each stamp.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$tauriConf = Join-Path $root 'src\backend\tauri.conf.json'
$cargoToml = Join-Path $root 'src\backend\Cargo.toml'
$cargoLock = Join-Path $root 'src\backend\Cargo.lock'

# Local builds carry a sentinel patch; CI overwrites it with the real count.
$localPatch = '9999'

function Get-CurrentVersion {
    $conf = Get-Content $tauriConf -Raw | ConvertFrom-Json
    if ($conf.version -notmatch '^(\d+)\.(\d+)\.') {
        throw "Unrecognized version '$($conf.version)' in $tauriConf"
    }
    [pscustomobject]@{ Major = [int]$Matches[1]; Minor = [int]$Matches[2] }
}

function Set-VersionInFile {
    param([string]$Path, [string]$Pattern, [string]$Replacement)

    $text = Get-Content $Path -Raw
    if ($text -notmatch $Pattern) {
        throw "No version line matching /$Pattern/ in $Path"
    }
    $updated = [regex]::Replace($text, $Pattern, $Replacement, 1)
    Set-Content -Path $Path -Value $updated -NoNewline
}

$current = Get-CurrentVersion
$newVersion = "{0}.{1}.{2}" -f $current.Major, ($current.Minor + 1), $localPatch

Set-VersionInFile $tauriConf '(?m)^(\s*"version"\s*:\s*)"[^"]*"' "`${1}""$newVersion"""
Set-VersionInFile $cargoToml '(?m)^version = "[^"]*"' "version = ""$newVersion"""
Set-VersionInFile $cargoLock '(?m)^(name = "MyNote"\r?\nversion = )"[^"]*"' "`${1}""$newVersion"""

Write-Host "Stamped version $($current.Major).$($current.Minor) -> $($current.Major).$($current.Minor + 1) (files now read $newVersion)"
Write-Host "Commit this change — CI counts its patch number from it."

function Invoke-InDirectory {
    param(
        [Parameter(Mandatory)][string]$Directory,
        [Parameter(Mandatory)][string]$What,
        [Parameter(Mandatory)][scriptblock]$Command
    )

    Push-Location $Directory
    try {
        & $Command
        if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

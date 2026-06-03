param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ClawArgs
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$defaultExe = Join-Path $repoRoot "rust\target\debug\claw.exe"
$releaseExe = Join-Path $repoRoot "rust\target\release\claw.exe"
$clawExe = if (Test-Path -LiteralPath $releaseExe) { $releaseExe } else { $defaultExe }
$logDir = Join-Path $env:LOCALAPPDATA "Galen\logs"
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logFile = Join-Path $logDir "claw-$timestamp.log"

New-Item -ItemType Directory -Force -Path $logDir | Out-Null

function Write-LauncherLog {
    param([string]$Message)
    $line = "[{0}] {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $Message
    Add-Content -LiteralPath $logFile -Value $line
}

Write-LauncherLog "repoRoot=$repoRoot"
Write-LauncherLog "clawExe=$clawExe"
Write-LauncherLog "args=$($ClawArgs -join ' ')"

if (-not (Test-Path -LiteralPath $clawExe)) {
    Write-LauncherLog "missing executable"
    Write-Host "claw.exe was not found."
    Write-Host "Expected: $clawExe"
    Write-Host "Build it from the repo with:"
    Write-Host "  cd `"$repoRoot\rust`""
    Write-Host "  cargo build --workspace"
    Write-Host ""
    Write-Host "Log: $logFile"
    exit 1
}

Push-Location $repoRoot
try {
    & $clawExe @ClawArgs 2>&1 | Tee-Object -FilePath $logFile -Append
    $exitCode = $LASTEXITCODE
    Write-LauncherLog "exitCode=$exitCode"
    if ($exitCode -ne 0) {
        Write-Host ""
        Write-Host "Galen exited with code $exitCode."
        Write-Host "Log: $logFile"
    }
    exit $exitCode
}
catch {
    Write-LauncherLog "exception=$($_.Exception.Message)"
    Write-Host ""
    Write-Host "Galen launcher failed:"
    Write-Host $_.Exception.Message
    Write-Host "Log: $logFile"
    exit 1
}
finally {
    Pop-Location
}

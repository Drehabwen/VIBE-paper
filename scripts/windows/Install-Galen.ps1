param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Galen\bin"),
    [switch]$SkipBuild,
    [switch]$AddToPath,
    [switch]$NoShortcut,
    [switch]$NoExit
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$rustDir = Join-Path $repoRoot "rust"
$releaseExe = Join-Path $rustDir "target\release\claw.exe"
$installedExe = Join-Path $InstallDir "claw.exe"

if (-not $SkipBuild) {
    Write-Host "Building release binary..."
    Push-Location $rustDir
    try {
        cargo build --release -p rusty-claude-cli
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path -LiteralPath $releaseExe)) {
    throw "Release binary not found: $releaseExe. Run without -SkipBuild or build it manually first."
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -LiteralPath $releaseExe -Destination $installedExe -Force
Write-Host "Installed: $installedExe"

if ($AddToPath) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $pathEntries = @()
    if ($userPath) {
        $pathEntries = $userPath -split ";"
    }
    if ($pathEntries -notcontains $InstallDir) {
        $newPath = (($pathEntries + $InstallDir) | Where-Object { $_ } | Select-Object -Unique) -join ";"
        try {
            [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
            Write-Host "Added to user PATH: $InstallDir"
            Write-Host "Open a new terminal for PATH changes to take effect."
        }
        catch {
            Write-Warning "Could not update user PATH: $($_.Exception.Message)"
            Write-Warning "You can still run the installed binary directly: $installedExe"
        }
    }
    else {
        Write-Host "PATH already contains: $InstallDir"
    }
}

if (-not $NoShortcut) {
    $shortcutScript = Join-Path $PSScriptRoot "Install-GalenShortcut.ps1"
    if ($NoExit) {
        & $shortcutScript -NoExit
    }
    else {
        & $shortcutScript
    }
}

& $installedExe launch-check

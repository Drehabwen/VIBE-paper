param(
    [string]$ShortcutName = "Galen",
    [switch]$NoExit
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$launcher = Join-Path $repoRoot "scripts\windows\Start-Galen.ps1"
$desktop = [Environment]::GetFolderPath("Desktop")
$shortcutPath = Join-Path $desktop "$ShortcutName.lnk"
$powershell = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
$releaseExe = Join-Path $repoRoot "rust\target\release\claw.exe"
$debugExe = Join-Path $repoRoot "rust\target\debug\claw.exe"
$iconExe = if (Test-Path -LiteralPath $releaseExe) { $releaseExe } else { $debugExe }
$noExitFlag = if ($NoExit) { "-NoExit " } else { "" }

if (-not (Test-Path -LiteralPath $launcher)) {
    throw "Launcher script not found: $launcher"
}

$ws = New-Object -ComObject WScript.Shell
$shortcut = $ws.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $powershell
$shortcut.Arguments = "${noExitFlag}-ExecutionPolicy Bypass -File `"$launcher`""
$shortcut.WorkingDirectory = $repoRoot
$shortcut.Description = "Launch Galen with Windows logging"
if (Test-Path -LiteralPath $iconExe) {
    $shortcut.IconLocation = "$iconExe,0"
}
$shortcut.Save()

Write-Host "Shortcut created: $shortcutPath"
Write-Host "Launcher: $launcher"
Write-Host "Logs: $env:LOCALAPPDATA\Galen\logs"

# RightType installer — portable per-user install.
# Usage:  pwsh -File install.ps1 [-Autostart] [-Exe path\to\righttype.exe]

param(
    [switch]$Autostart,
    [string]$Exe = (Join-Path $PSScriptRoot "..\target\release\righttype.exe")
)

$ErrorActionPreference = "Stop"
$dest = Join-Path $env:LOCALAPPDATA "RightType"
$src = if (Test-Path $Exe) { $Exe } else { Join-Path $PSScriptRoot "righttype.exe" }
if (-not (Test-Path $src)) { throw "righttype.exe not found at $src" }

New-Item -ItemType Directory -Force -Path $dest | Out-Null
Copy-Item $src (Join-Path $dest "righttype.exe") -Force

# Start Menu shortcut
$sm = [Environment]::GetFolderPath("Programs")
$ws = New-Object -ComObject WScript.Shell
$lnk = $ws.CreateShortcut((Join-Path $sm "RightType.lnk"))
$lnk.TargetPath = Join-Path $dest "righttype.exe"
$lnk.WorkingDirectory = $dest
$lnk.Description = "Fix wrong-keyboard-layout Thai/English typing"
$lnk.Save()

# Optional launch-at-login
if ($Autostart) {
    Set-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" `
        -Name "RightType" -Value (Join-Path $dest "righttype.exe")
}

Write-Host "Installed to $dest"
Write-Host "Start Menu shortcut: RightType"
if ($Autostart) { Write-Host "Autostart: enabled (HKCU Run)" }
Write-Host "Launch it from the Start Menu — look for the tray icon."

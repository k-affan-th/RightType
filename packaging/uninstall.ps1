# RightType uninstaller — removes app files, shortcut, autostart, settings.
param([switch]$KeepSettings)

$ErrorActionPreference = "SilentlyContinue"
Get-Process righttype -ErrorAction SilentlyContinue | Stop-Process -Force

$dest = Join-Path $env:LOCALAPPDATA "RightType"
Remove-Item $dest -Recurse -Force

$sm = [Environment]::GetFolderPath("Programs")
Remove-Item (Join-Path $sm "RightType.lnk") -Force

Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "RightType"

if (-not $KeepSettings) {
    Remove-Item (Join-Path $env:APPDATA "RightType") -Recurse -Force
}

Write-Host "RightType removed." (-not $KeepSettings ? " Settings cleared." : " Settings kept.")

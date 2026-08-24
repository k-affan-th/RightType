# Build the release artifact set: exe + portable zip (+ Inno setup if ISCC exists).
# Usage: pwsh -File packaging\build_release.ps1

$ErrorActionPreference = "Stop"
$ver = "1.0.0-rc1"
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root

cargo build --release --features winos
if ($LASTEXITCODE -ne 0) { throw "build failed" }

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

# Portable payload
$stage = Join-Path $dist "RightType-$ver-portable"
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item "$root\target\release\righttype.exe" $stage -Force
Copy-Item "$root\packaging\install.ps1" $stage -Force
Copy-Item "$root\packaging\uninstall.ps1" $stage -Force
Copy-Item "$root\README.md" $stage -Force
Compress-Archive -Path "$stage\*" -DestinationPath "$dist\RightType-$ver-x64.zip" -Force
Remove-Item $stage -Recurse -Force

# Hash + size
$exe = Get-Item "$root\target\release\righttype.exe"
$hash = (Get-FileHash $exe.FullName -Algorithm SHA256).Hash
"$($exe.Length) bytes`nSHA256: $hash" | Out-File "$dist\SHA256.txt" -Encoding utf8

# Optional Inno Setup
$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (Test-Path $iscc) {
    & $iscc "$root\packaging\RightType.iss"
} else {
    Write-Host "Inno Setup not found — skipped setup.exe (zip + scripts are ready)."
}

Write-Host "Artifacts in $dist :"
Get-ChildItem $dist | Format-Table Name, Length -AutoSize
Pop-Location

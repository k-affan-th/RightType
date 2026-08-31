# Build the release artifact set: exe + portable zip (+ Inno setup if ISCC exists).
# Usage: pwsh -File packaging\build_release.ps1

$ErrorActionPreference = "Stop"
# Keep in step with Cargo.toml and packaging/RightType.iss.
$ver = "1.0.0"
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


# Optional Inno Setup
# winget installs Inno Setup per-user by default, which is not under
# Program Files; check both so a normal `winget install JRSoftware.InnoSetup`
# is enough to produce setup.exe.
$isccCandidates = @(
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
)
$iscc = $isccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($iscc) {
    & $iscc "$root\packaging\RightType.iss"
} else {
    Write-Host "Inno Setup not found — skipped setup.exe (zip + scripts are ready)."
}

# Checksums for everything that gets published, written last so the installer
# is included. Signing changes a file's hash, so re-run this after signtool.
$lines = @(
    "RightType $ver - SHA-256",
    "Computed $(Get-Date -Format o)",
    "UNSIGNED: regenerate this file after signing - the hashes will change.",
    ""
)
$published = Get-ChildItem $dist -File | Where-Object { $_.Name -ne "SHA256.txt" } | Sort-Object Name
foreach ($f in $published) {
    $lines += "{0}  {1}  ({2} bytes)" -f (Get-FileHash $f.FullName -Algorithm SHA256).Hash, $f.Name, $f.Length
}
$lines | Out-File "$dist\SHA256.txt" -Encoding utf8

Write-Host "Artifacts in $dist :"
Get-ChildItem $dist | Format-Table Name, Length -AutoSize
Pop-Location

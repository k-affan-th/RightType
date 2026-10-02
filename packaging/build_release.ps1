# Build the release artifact set: exe + portable zip (+ Inno setup if ISCC exists).
# Usage: pwsh -File packaging\build_release.ps1 [-Arch x64|arm64]
#
# x64 (the default) is built for the machine's own target; arm64 is
# cross-built (rustup target add aarch64-pc-windows-msvc, and Visual Studio's
# ARM64 build tools) and packaged as RightType-<version>-arm64-setup.exe and
# RightType-<version>-arm64.zip.

param([ValidateSet("x64", "arm64")][string]$Arch = "x64")

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root

# Cargo.toml is the single source of the version; the installer receives it
# on the ISCC command line.
$verLine = Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $verLine) { throw "version not found in Cargo.toml" }
$ver = $verLine.Matches[0].Groups[1].Value

if ($Arch -eq "arm64") {
    cargo build --release --features winos --target aarch64-pc-windows-msvc
    $exeDir = "target\aarch64-pc-windows-msvc\release"
} else {
    cargo build --release --features winos
    $exeDir = "target\release"
}
if ($LASTEXITCODE -ne 0) { throw "build failed" }

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

# Portable payload
$stage = Join-Path $dist "RightType-$ver-$Arch-portable"
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item "$root\$exeDir\righttype.exe" $stage -Force
Copy-Item "$root\packaging\install.ps1" $stage -Force
Copy-Item "$root\packaging\uninstall.ps1" $stage -Force
Copy-Item "$root\README.md" $stage -Force
Copy-Item "$root\CHANGELOG.md" $stage -Force
Copy-Item "$root\LICENSE-MIT" $stage -Force
Copy-Item "$root\LICENSE-APACHE" $stage -Force
Copy-Item "$root\assets\fonts\OFL.txt" "$stage\FONT-LICENSE-OFL.txt" -Force
Compress-Archive -Path "$stage\*" -DestinationPath "$dist\RightType-$ver-$Arch.zip" -Force
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
    & $iscc "/DMyAppVersion=$ver" "/DArch=$Arch" "/DExeDir=$exeDir" "$root\packaging\RightType.iss"
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
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
# Only this version's files: artifacts left in dist\ by an earlier build must
# not be listed as part of this release.
$published = Get-ChildItem $dist -File | Where-Object { $_.Name -like "RightType-$ver-*" } | Sort-Object Name
foreach ($f in $published) {
    $lines += "{0}  {1}  ({2} bytes)" -f (Get-FileHash $f.FullName -Algorithm SHA256).Hash, $f.Name, $f.Length
}
$lines | Out-File "$dist\SHA256.txt" -Encoding utf8

Write-Host "Artifacts in $dist :"
Get-ChildItem $dist | Format-Table Name, Length -AutoSize
Pop-Location

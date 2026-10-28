# Build the Chocolatey package for a release:
# stages package files into dist/choco/package/ and runs `choco pack`
# to generate dist/choco/righttype.<version>.nupkg.
#
# SHA-256 hashes are resolved from local installer files if present,
# falling back to dist/SHA256.txt or the remote GitHub release SHA256.txt.
#
# Usage: pwsh -File packaging/choco.ps1 -Version 2.6.3 [-SetupX64 dist/RightType-2.6.3-setup.exe] [-Out dist/choco]
#
# Staged package contents:
#   dist/choco/package/
#     righttype.nuspec
#     tools/
#       chocolateyInstall.ps1
#       chocolateyUninstall.ps1
#       VERIFICATION.txt
#       LICENSE.txt

param(
    [Parameter(Mandatory)][string]$Version,
    [string]$SetupX64,
    [string]$SetupArm64,
    [string]$Setup, # Backwards compatibility alias for SetupX64
    [string]$Out
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
if ($Setup -and -not $SetupX64) { $SetupX64 = $Setup }
if (-not $SetupX64) { $SetupX64 = Join-Path $root "dist/RightType-$Version-setup.exe" }
if (-not $SetupArm64) { $SetupArm64 = Join-Path $root "dist/RightType-$Version-arm64-setup.exe" }
if (-not $Out) {
    $Out = Join-Path $root "dist/choco"
} else {
    if (-not [System.IO.Path]::IsPathRooted($Out)) {
        $Out = [System.IO.Path]::GetFullPath((Join-Path $PWD $Out))
    }
}

$repo = "https://github.com/k-affan-th/RightType"
$x64Name = "RightType-$Version-setup.exe"
$arm64Name = "RightType-$Version-arm64-setup.exe"

function Get-HashFromText([string]$content, [string]$filename) {
    if ([string]::IsNullOrWhiteSpace($content)) { return $null }
    $baseName = [System.IO.Path]::GetFileName($filename)
    foreach ($line in ($content -split "`r?`n")) {
        $trimmed = $line.Trim()
        if ($trimmed -match "^([0-9a-fA-F]{64})\s+[*]?(\S+)") {
            if ($Matches[2] -eq $baseName) { return $Matches[1].ToUpper() }
        }
        if ($trimmed -match "^[*]?(\S+)\s+([0-9a-fA-F]{64})") {
            if ($Matches[1] -eq $baseName) { return $Matches[2].ToUpper() }
        }
    }
    return $null
}

$shaX64 = $null
$shaArm64 = $null

# Tier 1: Local installer files
if ($SetupX64 -and (Test-Path $SetupX64)) {
    $shaX64 = (Get-FileHash $SetupX64 -Algorithm SHA256).Hash.ToUpper()
}
if ($SetupArm64 -and (Test-Path $SetupArm64)) {
    $shaArm64 = (Get-FileHash $SetupArm64 -Algorithm SHA256).Hash.ToUpper()
}

# Tier 2: dist/SHA256.txt
if (-not $shaX64 -or -not $shaArm64) {
    $localShaFile = Join-Path $root "dist/SHA256.txt"
    if (Test-Path $localShaFile) {
        $shaText = Get-Content $localShaFile -Raw -Encoding utf8
        if (-not $shaX64) { $shaX64 = Get-HashFromText $shaText $x64Name }
        if (-not $shaArm64) { $shaArm64 = Get-HashFromText $shaText $arm64Name }
    }
}

# Tier 3: Remote GitHub release SHA256.txt
if (-not $shaX64 -or -not $shaArm64) {
    $remoteUrl = "$repo/releases/download/v$Version/SHA256.txt"
    try {
        $remoteShaText = (Invoke-RestMethod -Uri $remoteUrl -Method Get)
        if (-not $shaX64) { $shaX64 = Get-HashFromText $remoteShaText $x64Name }
        if (-not $shaArm64) { $shaArm64 = Get-HashFromText $remoteShaText $arm64Name }
    } catch {
        Write-Warning "Could not fetch remote SHA256.txt from $remoteUrl`: $_"
    }
}

if (-not $shaX64) {
    throw "Unable to resolve SHA256 hash for $x64Name from local file, dist/SHA256.txt, or GitHub release."
}

# Stage package directory
$packageDir = Join-Path $Out "package"
$toolsDir = Join-Path $packageDir "tools"

if (-not (Test-Path $toolsDir)) {
    New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
}

$templateDir = Join-Path $PSScriptRoot "choco"
$utf8 = New-Object System.Text.UTF8Encoding $false

# 1. righttype.nuspec
$nuspecTemplate = Get-Content (Join-Path $templateDir "righttype.nuspec.template") -Raw -Encoding utf8
$nuspecContent = $nuspecTemplate.Replace("__VERSION__", $Version).Replace("__SHA256_X64__", $shaX64)
[IO.File]::WriteAllText((Join-Path $packageDir "righttype.nuspec"), $nuspecContent, $utf8)

# 2. tools/chocolateyInstall.ps1
$installTemplate = Get-Content (Join-Path $templateDir "tools/chocolateyInstall.ps1.template") -Raw -Encoding utf8
$installContent = $installTemplate.Replace("__VERSION__", $Version).Replace("__SHA256_X64__", $shaX64)
[IO.File]::WriteAllText((Join-Path $toolsDir "chocolateyInstall.ps1"), $installContent, $utf8)

# 3. tools/chocolateyUninstall.ps1
$uninstallPath = Join-Path $templateDir "tools/chocolateyUninstall.ps1"
Copy-Item $uninstallPath (Join-Path $toolsDir "chocolateyUninstall.ps1") -Force

# 4. tools/VERIFICATION.txt
$verTemplate = Get-Content (Join-Path $templateDir "tools/VERIFICATION.txt.template") -Raw -Encoding utf8
$verContent = $verTemplate.Replace("__VERSION__", $Version).Replace("__SHA256_X64__", $shaX64)
[IO.File]::WriteAllText((Join-Path $toolsDir "VERIFICATION.txt"), $verContent, $utf8)

# 5. tools/LICENSE.txt (copied from repo root LICENSE-MIT)
$licenseSource = Join-Path $root "LICENSE-MIT"
if (-not (Test-Path $licenseSource)) {
    $licenseSource = Join-Path $templateDir "tools/LICENSE.txt"
}
Copy-Item $licenseSource (Join-Path $toolsDir "LICENSE.txt") -Force

Write-Host "Chocolatey package files staged in $packageDir"
Get-ChildItem -Recurse $packageDir | Format-Table FullName, Length -AutoSize

# Pack .nupkg using choco CLI if available
$choco = Get-Command choco -ErrorAction SilentlyContinue
$nuspecPath = Join-Path $packageDir "righttype.nuspec"
if ($choco) {
    Write-Host "Packing Chocolatey package with choco pack..."
    & $choco.Source pack $nuspecPath --outputdirectory $Out
    if ($LASTEXITCODE -ne 0) {
        throw "choco pack failed with exit code $LASTEXITCODE"
    }
    $nupkg = Join-Path $Out "righttype.$Version.nupkg"
    if (Test-Path $nupkg) {
        Write-Host "Successfully generated $nupkg"
        Get-Item $nupkg | Format-Table Name, Length -AutoSize
    }
} else {
    Write-Warning "Chocolatey CLI ('choco') was not found in PATH."
    Write-Host "Staged package files are ready in: $packageDir"
    Write-Host "To build the package manually once choco is installed, run:"
    Write-Host "  choco pack `"$nuspecPath`" --outputdirectory `"$Out`""
}

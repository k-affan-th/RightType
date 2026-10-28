# Write the winget manifests for a release: the three files that
# microsoft/winget-pkgs expects under manifests/k/k-affan-th/RightType/<version>/.
#
# SHA-256 hashes are resolved from local installer files if present,
# falling back to dist/SHA256.txt or the remote GitHub release SHA256.txt.
#
# Usage: pwsh -File packaging\winget.ps1 -Version 2.6.3 [-SetupX64 dist\RightType-2.6.3-setup.exe] [-SetupArm64 dist\RightType-2.6.3-arm64-setup.exe] [-Out dist\winget]
#
# Submitting them is a pull request to microsoft/winget-pkgs: the Release
# workflow does it with wingetcreate when a WINGET_TOKEN secret is set, and
# otherwise attaches the manifests to the release for a manual submission.

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
    $Out = Join-Path $root "dist/winget"
} else {
    if (-not [System.IO.Path]::IsPathRooted($Out)) {
        $Out = [System.IO.Path]::GetFullPath((Join-Path $PWD $Out))
    }
}

$id = "k-affan-th.RightType"
$repo = "https://github.com/k-affan-th/RightType"
$x64Name = "RightType-$Version-setup.exe"
$arm64Name = "RightType-$Version-arm64-setup.exe"
$urlX64 = "$repo/releases/download/v$Version/$x64Name"
$urlArm64 = "$repo/releases/download/v$Version/$arm64Name"

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

if ($SetupX64 -and (Test-Path $SetupX64)) {
    $shaX64 = (Get-FileHash $SetupX64 -Algorithm SHA256).Hash.ToUpper()
}
if ($SetupArm64 -and (Test-Path $SetupArm64)) {
    $shaArm64 = (Get-FileHash $SetupArm64 -Algorithm SHA256).Hash.ToUpper()
}

if (-not $shaX64 -or -not $shaArm64) {
    $localShaFile = Join-Path $root "dist/SHA256.txt"
    if (Test-Path $localShaFile) {
        $shaText = Get-Content $localShaFile -Raw -Encoding utf8
        if (-not $shaX64) { $shaX64 = Get-HashFromText $shaText $x64Name }
        if (-not $shaArm64) { $shaArm64 = Get-HashFromText $shaText $arm64Name }
    }
}

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

if (-not $shaX64) { throw "Unable to resolve SHA256 hash for $x64Name from local file, dist/SHA256.txt, or GitHub release." }
if (-not $shaArm64) { throw "Unable to resolve SHA256 hash for $arm64Name from local file, dist/SHA256.txt, or GitHub release." }

# Inno Setup's uninstall entry: the AppId in RightType.iss plus "_is1".
$productCode = "{8C6B9A2E-52C1-4E63-9B7A-7C1F4A2B9D33}_is1"
$schema = "1.6.0"
$header = "# Created by packaging/winget.ps1"

$dir = Join-Path $Out $Version
New-Item -ItemType Directory -Force -Path $dir | Out-Null

$versionManifest = @"
$header
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.version.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $Version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: $schema
"@

$installerManifest = @"
$header
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.installer.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $Version
Platform:
- Windows.Desktop
MinimumOSVersion: 10.0.17763.0
InstallerType: inno
Scope: user
InstallModes:
- interactive
- silent
- silentWithProgress
UpgradeBehavior: install
ProductCode: '$productCode'
Installers:
- Architecture: x64
  InstallerUrl: $urlX64
  InstallerSha256: $shaX64
- Architecture: arm64
  InstallerUrl: $urlArm64
  InstallerSha256: $shaArm64
ManifestType: installer
ManifestVersion: $schema
"@

$localeManifest = @"
$header
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.defaultLocale.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $Version
PackageLocale: en-US
Publisher: k-affan-th
PublisherUrl: https://github.com/k-affan-th
PublisherSupportUrl: $repo/issues
PackageName: RightType
PackageUrl: $repo
License: MIT OR Apache-2.0
LicenseUrl: $repo/blob/main/LICENSE-MIT
ShortDescription: Fixes Thai/English text typed in the wrong keyboard layout, without retyping.
Description: |-
  RightType fixes text typed in the wrong keyboard layout (Thai Kedmanee or Pattachote while
  English is active, or the other way round) automatically as you type, or on a hotkey. It never
  goes online and never writes what you type to disk.
Moniker: righttype
Tags:
- keyboard
- keyboard-layout
- thai
- typing
- rightlang
ReleaseNotesUrl: $repo/releases/tag/v$Version
ManifestType: defaultLocale
ManifestVersion: $schema
"@

$utf8 = New-Object System.Text.UTF8Encoding $false
[IO.File]::WriteAllText((Join-Path $dir "$id.yaml"), $versionManifest + "`n", $utf8)
[IO.File]::WriteAllText((Join-Path $dir "$id.installer.yaml"), $installerManifest + "`n", $utf8)
[IO.File]::WriteAllText((Join-Path $dir "$id.locale.en-US.yaml"), $localeManifest + "`n", $utf8)

Write-Host "winget manifests for $id $Version in $dir"
Get-ChildItem $dir | Format-Table Name, Length -AutoSize

# Write the winget manifests for a release: the three files that
# microsoft/winget-pkgs expects under manifests/k/k-affan-th/RightType/<version>/.
#
# The installer's SHA-256 is read from the built setup.exe, so run this after
# build_release.ps1 (and after signing, if the installer is ever signed).
# Usage: pwsh -File packaging\winget.ps1 -Version 2.0.0 [-Setup dist\RightType-2.0.0-setup.exe] [-Out dist\winget]
#
# Submitting them is a pull request to microsoft/winget-pkgs: the Release
# workflow does it with wingetcreate when a WINGET_TOKEN secret is set, and
# otherwise attaches the manifests to the release for a manual submission.

param(
    [Parameter(Mandatory)][string]$Version,
    [string]$Setup,
    [string]$Out
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
if (-not $Setup) { $Setup = Join-Path $root "dist/RightType-$Version-setup.exe" }
if (-not $Out) { $Out = Join-Path $root "dist/winget" }
if (-not (Test-Path $Setup)) { throw "installer not found: $Setup" }

$id = "k-affan-th.RightType"
$repo = "https://github.com/k-affan-th/RightType"
$url = "$repo/releases/download/v$Version/RightType-$Version-setup.exe"
$sha = (Get-FileHash $Setup -Algorithm SHA256).Hash
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
  InstallerUrl: $url
  InstallerSha256: $sha
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

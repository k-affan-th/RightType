# Build the Microsoft Store package (.msix) for one architecture.
# Usage: pwsh -File packaging\msix.ps1 -IdentityName <Package/Identity/Name> `
#            -Publisher "CN=..." -PublisherDisplayName "<name>" [-Arch x64|arm64] [-Out dist\msix]
#
# The three identity values are shown in Partner Center under the MSIX product's
# "Product identity" page. They are required: a package with the wrong identity
# is rejected on upload.
#
# Run packaging\build_release.ps1 -Arch <arch> first: this script packs the exe it
# built. The .msix is deliberately unsigned - the Store signs it on ingestion.
# (Sideloading it locally needs signing with a matching test certificate.)

param(
    [Parameter(Mandatory)][string]$IdentityName,
    [Parameter(Mandatory)][string]$Publisher,
    [Parameter(Mandatory)][string]$PublisherDisplayName,
    [ValidateSet("x64", "arm64")][string]$Arch = "x64",
    [string]$Out = "dist\msix"
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

$verLine = Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $verLine) { throw "version not found in Cargo.toml" }
$ver = $verLine.Matches[0].Groups[1].Value
# MSIX versions are four-part; the Store reserves the last part (must be 0).
$msixVer = "$ver.0"

$exeDir = if ($Arch -eq "arm64") { "target\aarch64-pc-windows-msvc\release" } else { "target\release" }
$exe = Join-Path $root "$exeDir\righttype.exe"
if (-not (Test-Path $exe)) { throw "$exe not found - run packaging\build_release.ps1 -Arch $Arch first" }

$kitArch = if ($Arch -eq "arm64") { "arm64" } else { "x64" }
$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending |
    ForEach-Object { Join-Path $_.FullName "$kitArch\makeappx.exe" } |
    Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $makeappx) { throw "makeappx.exe not found - install the Windows 10/11 SDK" }

$stage = Join-Path $root "dist\msix-stage-$Arch"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path "$stage\Assets" | Out-Null

Copy-Item $exe $stage
Copy-Item "$root\LICENSE-MIT", "$root\LICENSE-APACHE" $stage

# Tile/logo PNGs drawn from the app icon (largest frame of the .ico via the PNG in docs/img).
Add-Type -AssemblyName System.Drawing
$src = [System.Drawing.Image]::FromFile((Join-Path $root "docs\img\icon.png"))
function Save-Logo([int]$w, [int]$h, [string]$name) {
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.Clear([System.Drawing.Color]::Transparent)
    $side = [Math]::Min($w, $h)
    $g.DrawImage($src, [int](($w - $side) / 2), [int](($h - $side) / 2), $side, $side)
    $g.Dispose()
    $bmp.Save("$stage\Assets\$name", [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}
Save-Logo 50 50 "StoreLogo.png"
Save-Logo 44 44 "Square44x44Logo.png"
Save-Logo 150 150 "Square150x150Logo.png"
$src.Dispose()

$xml = Get-Content "$PSScriptRoot\msix\AppxManifest.xml.template" -Raw
$map = @{
    "@IDENTITY_NAME@"           = $IdentityName
    "@PUBLISHER@"               = $Publisher
    "@PUBLISHER_DISPLAY_NAME@"  = $PublisherDisplayName
    "@VERSION@"                 = $msixVer
    "@ARCH@"                    = $Arch
}
foreach ($k in $map.Keys) { $xml = $xml.Replace($k, [System.Security.SecurityElement]::Escape($map[$k])) }
[System.IO.File]::WriteAllText("$stage\AppxManifest.xml", $xml, (New-Object System.Text.UTF8Encoding $false))

$outDir = Join-Path $root $Out
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$pkg = Join-Path $outDir "RightType-$ver-$Arch.msix"
& $makeappx pack /d $stage /p $pkg /o
if ($LASTEXITCODE -ne 0) { throw "makeappx failed" }
Remove-Item $stage -Recurse -Force
Write-Host "Built $pkg"

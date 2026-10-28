# Write the Scoop manifest for a release: righttype.json for a Scoop bucket.
#
# SHA-256 hashes are resolved from local portable zip files if present,
# falling back to dist/SHA256.txt or the remote GitHub release SHA256.txt.
#
# Usage: pwsh -File packaging\scoop.ps1 -Version 2.6.3 [-ZipX64 dist\RightType-2.6.3-x64.zip] [-ZipArm64 dist\RightType-2.6.3-arm64.zip] [-Out dist\scoop]
#
# The generated righttype.json can be published to a custom Scoop bucket
# (e.g. k-affan-th/scoop-bucket under bucket/righttype.json) or submitted
# to ScoopInstaller/Extras once eligible.

param(
    [Parameter(Mandatory)][string]$Version,
    [string]$ZipX64,
    [string]$ZipArm64,
    [string]$Zip, # Backwards compatibility alias for ZipX64
    [string]$Out
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
if ($Zip -and -not $ZipX64) { $ZipX64 = $Zip }
if (-not $ZipX64) { $ZipX64 = Join-Path $root "dist/RightType-$Version-x64.zip" }
if (-not $ZipArm64) { $ZipArm64 = Join-Path $root "dist/RightType-$Version-arm64.zip" }
if (-not $Out) { $Out = Join-Path $root "dist/scoop" }

$repo = "https://github.com/k-affan-th/RightType"
$x64ZipName = "RightType-$Version-x64.zip"
$arm64ZipName = "RightType-$Version-arm64.zip"
$urlX64 = "$repo/releases/download/v$Version/$x64ZipName"
$urlArm64 = "$repo/releases/download/v$Version/$arm64ZipName"

function Get-HashFromText([string]$content, [string]$filename) {
    if ([string]::IsNullOrWhiteSpace($content)) { return $null }
    $baseName = [System.IO.Path]::GetFileName($filename)
    foreach ($line in ($content -split "`r?`n")) {
        $trimmed = $line.Trim()
        if ($trimmed -match "^([0-9a-fA-F]{64})\s+[*]?(\S+)") {
            if ($Matches[2] -eq $baseName) { return $Matches[1].ToLower() }
        }
        if ($trimmed -match "^[*]?(\S+)\s+([0-9a-fA-F]{64})") {
            if ($Matches[1] -eq $baseName) { return $Matches[2].ToLower() }
        }
    }
    return $null
}

$shaX64 = $null
$shaArm64 = $null

if ($ZipX64 -and (Test-Path $ZipX64)) {
    $shaX64 = (Get-FileHash $ZipX64 -Algorithm SHA256).Hash.ToLower()
}
if ($ZipArm64 -and (Test-Path $ZipArm64)) {
    $shaArm64 = (Get-FileHash $ZipArm64 -Algorithm SHA256).Hash.ToLower()
}

if (-not $shaX64 -or -not $shaArm64) {
    $localShaFile = Join-Path $root "dist/SHA256.txt"
    if (Test-Path $localShaFile) {
        $shaText = Get-Content $localShaFile -Raw -Encoding utf8
        if (-not $shaX64) { $shaX64 = Get-HashFromText $shaText $x64ZipName }
        if (-not $shaArm64) { $shaArm64 = Get-HashFromText $shaText $arm64ZipName }
    }
}

if (-not $shaX64 -or -not $shaArm64) {
    $remoteUrl = "$repo/releases/download/v$Version/SHA256.txt"
    try {
        $remoteShaText = (Invoke-RestMethod -Uri $remoteUrl -Method Get)
        if (-not $shaX64) { $shaX64 = Get-HashFromText $remoteShaText $x64ZipName }
        if (-not $shaArm64) { $shaArm64 = Get-HashFromText $remoteShaText $arm64ZipName }
    } catch {
        Write-Warning "Could not fetch remote SHA256.txt from $remoteUrl`: $_"
    }
}

if (-not $shaX64) { throw "Unable to resolve SHA256 hash for $x64ZipName from local file, dist/SHA256.txt, or GitHub release." }
if (-not $shaArm64) { throw "Unable to resolve SHA256 hash for $arm64ZipName from local file, dist/SHA256.txt, or GitHub release." }

if (-not (Test-Path $Out)) {
    New-Item -ItemType Directory -Force -Path $Out | Out-Null
}

$manifest = [ordered]@{
    version      = $Version
    description  = "Fix wrong-keyboard-layout Thai/English typing — a modern, safe Rust successor to RightLang"
    homepage     = $repo
    license      = "MIT OR Apache-2.0"
    architecture = [ordered]@{
        "64bit" = [ordered]@{
            url  = $urlX64
            hash = $shaX64
        }
        "arm64" = [ordered]@{
            url  = $urlArm64
            hash = $shaArm64
        }
    }
    bin          = "righttype.exe"
    shortcuts    = @(
        , @("righttype.exe", "RightType")
    )
    checkver     = "github"
    autoupdate   = [ordered]@{
        architecture = [ordered]@{
            "64bit" = [ordered]@{
                url = 'https://github.com/k-affan-th/RightType/releases/download/v$version/RightType-$version-x64.zip'
            }
            "arm64" = [ordered]@{
                url = 'https://github.com/k-affan-th/RightType/releases/download/v$version/RightType-$version-arm64.zip'
            }
        }
    }
}

$targetFile = Join-Path $Out "righttype.json"
$json = $manifest | ConvertTo-Json -Depth 10
$utf8 = New-Object System.Text.UTF8Encoding $false
[IO.File]::WriteAllText($targetFile, $json + "`n", $utf8)

Write-Host "Scoop manifest for RightType $Version written to $targetFile"
Get-Item $targetFile | Format-Table Name, Length -AutoSize

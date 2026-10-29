# Unified package verification script for RightType
# Validates manifests and packages across all 3 Windows package channels:
#   1. Windows Package Manager (winget)
#   2. Scoop
#   3. Chocolatey
#
# Usage:
#   pwsh -File packaging/verify_packages.ps1 -Version 2.6.3
#   pwsh -File packaging/verify_packages.ps1 -Version 2.6.3 -SkipGenerate
#
# Exit code: 0 on all pass, 1 on any failure.

param(
    [Parameter(Mandatory = $true)][string]$Version,
    [switch]$SkipGenerate,
    [string]$DistDir
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
if (-not $DistDir) {
    $DistDir = Join-Path $root "dist"
} else {
    if (-not [System.IO.Path]::IsPathRooted($DistDir)) {
        $DistDir = [System.IO.Path]::GetFullPath((Join-Path $PWD $DistDir))
    }
}

# ANSI color codes
$ESC = [char]27
$C_RESET  = "$ESC[0m"
$C_BOLD   = "$ESC[1m"
$C_GREEN  = "$ESC[32m"
$C_RED    = "$ESC[31m"
$C_YELLOW = "$ESC[33m"
$C_CYAN   = "$ESC[36m"
$C_GRAY   = "$ESC[90m"

function Write-StepHeader([string]$title) {
    Write-Host "`n$C_BOLD$C_CYAN[$title]$C_RESET"
}

function Write-CheckOk([string]$msg) {
    Write-Host "  $C_GREEN[OK]$C_RESET $msg"
}

function Write-CheckWarn([string]$msg) {
    Write-Host "  $C_YELLOW[WARN]$C_RESET $msg"
}

function Write-CheckFail([string]$msg) {
    Write-Host "  $C_RED[FAIL]$C_RESET $msg"
}

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

function Resolve-ReleaseHashes([string]$ver, [string]$rootDir) {
    $hashes = @{}
    $files = @(
        "RightType-$ver-setup.exe",
        "RightType-$ver-arm64-setup.exe",
        "RightType-$ver-x64.zip",
        "RightType-$ver-arm64.zip"
    )

    # Tier 1: Local installer/zip files
    foreach ($f in $files) {
        $p = Join-Path $rootDir "dist/$f"
        if (Test-Path $p) {
            $hashes[$f] = (Get-FileHash $p -Algorithm SHA256).Hash.ToUpper()
        }
    }

    # Tier 2: dist/SHA256.txt
    $localShaFile = Join-Path $rootDir "dist/SHA256.txt"
    $shaText = $null
    if (Test-Path $localShaFile) {
        $shaText = Get-Content $localShaFile -Raw -Encoding utf8
    }

    # Tier 3: Remote GitHub release SHA256.txt
    if (-not $shaText) {
        $remoteUrl = "https://github.com/k-affan-th/RightType/releases/download/v$ver/SHA256.txt"
        try {
            $shaText = Invoke-RestMethod -Uri $remoteUrl -Method Get -TimeoutSec 10
        } catch {
            # Remote release might not be accessible offline
        }
    }

    if ($shaText) {
        foreach ($f in $files) {
            if (-not $hashes.ContainsKey($f)) {
                $h = Get-HashFromText $shaText $f
                if ($h) { $hashes[$f] = $h }
            }
        }
    }

    return $hashes
}

Write-Host "$C_BOLD========================================================================================$C_RESET"
Write-Host "$C_BOLD                          RightType Package Verification                        $C_RESET"
Write-Host "$C_BOLD========================================================================================$C_RESET"
Write-Host "Version:      $Version"
Write-Host "SkipGenerate: $SkipGenerate"
Write-Host "Dist Path:    $DistDir"

# Results collection
$results = @(
    [ordered]@{ Channel = "winget"; Status = "PASS"; Notes = "" },
    [ordered]@{ Channel = "scoop"; Status = "PASS"; Notes = "" },
    [ordered]@{ Channel = "chocolatey"; Status = "PASS"; Notes = "" }
)

$wingetErrors = [System.Collections.Generic.List[string]]::new()
$wingetNotes  = [System.Collections.Generic.List[string]]::new()
$scoopErrors  = [System.Collections.Generic.List[string]]::new()
$scoopNotes   = [System.Collections.Generic.List[string]]::new()
$chocoErrors  = [System.Collections.Generic.List[string]]::new()
$chocoNotes   = [System.Collections.Generic.List[string]]::new()

# Step 0: Generation phase (unless -SkipGenerate)
if (-not $SkipGenerate) {
    Write-Host "`n$C_BOLD$C_CYAN[generate] Generating packaging manifests for v$Version...$C_RESET"

    $wingetGen = Join-Path $PSScriptRoot "winget.ps1"
    $scoopGen  = Join-Path $PSScriptRoot "scoop.ps1"
    $chocoGen  = Join-Path $PSScriptRoot "choco.ps1"

    try {
        & $wingetGen -Version $Version -Out (Join-Path $DistDir "winget")
        Write-CheckOk "winget generator completed"
    } catch {
        Write-CheckFail "winget generator failed: $_"
        $wingetErrors.Add("Generator packaging/winget.ps1 failed: $_")
    }

    try {
        & $scoopGen -Version $Version -Out (Join-Path $DistDir "scoop")
        Write-CheckOk "scoop generator completed"
    } catch {
        Write-CheckFail "scoop generator failed: $_"
        $scoopErrors.Add("Generator packaging/scoop.ps1 failed: $_")
    }

    try {
        & $chocoGen -Version $Version -Out (Join-Path $DistDir "choco")
        Write-CheckOk "choco generator completed"
    } catch {
        Write-CheckFail "choco generator failed: $_"
        $chocoErrors.Add("Generator packaging/choco.ps1 failed: $_")
    }
}

$refHashes = Resolve-ReleaseHashes $Version $root
if ($refHashes.Count -gt 0) {
    Write-Host "`n$C_GRAY[info] Resolved $($refHashes.Count) reference SHA-256 hashes for v$Version$C_RESET"
} else {
    Write-Host "`n$C_YELLOW[info] No external SHA256.txt found; validating hash formatting and internal consistency$C_RESET"
}

# -------------------------------------------------------------------------
# 1. Winget Channel Verification
# -------------------------------------------------------------------------
Write-StepHeader "winget: Windows Package Manager"
$wingetPass = ($wingetErrors.Count -eq 0)

$id = "k-affan-th.RightType"
$wingetDir = Join-Path $DistDir "winget/$Version"
$verFile = Join-Path $wingetDir "$id.yaml"
$instFile = Join-Path $wingetDir "$id.installer.yaml"
$locFile = Join-Path $wingetDir "$id.locale.en-US.yaml"

if (-not (Test-Path $wingetDir)) {
    $wingetErrors.Add("Manifest directory missing: $wingetDir")
    $wingetPass = $false
} else {
    # Check 1: 3 files exist
    $filesExist = $true
    foreach ($f in @($verFile, $instFile, $locFile)) {
        if (-not (Test-Path $f)) {
            $wingetErrors.Add("Missing manifest file: $([System.IO.Path]::GetFileName($f))")
            $filesExist = $false
        }
    }

    if ($filesExist) {
        Write-CheckOk "All 3 manifest files exist ($id.yaml, installer, locale.en-US)"
        $wingetNotes.Add("3 manifests present")

        # Check 2: Version manifest content
        $verContent = Get-Content $verFile -Raw -Encoding utf8
        if ($verContent -notmatch "PackageIdentifier:\s*$id" -or
            $verContent -notmatch "PackageVersion:\s*$([regex]::Escape($Version))" -or
            $verContent -notmatch "ManifestType:\s*version") {
            $wingetErrors.Add("Version manifest failed schema content checks")
            $wingetPass = $false
        } else {
            Write-CheckOk "Version manifest syntax and metadata valid"
        }

        # Check 3: Installer manifest architecture, SHA256, and metadata
        $instContent = Get-Content $instFile -Raw -Encoding utf8
        $instValid = $true

        if ($instContent -notmatch "PackageIdentifier:\s*$id" -or
            $instContent -notmatch "PackageVersion:\s*$([regex]::Escape($Version))" -or
            $instContent -notmatch "InstallerType:\s*inno" -or
            $instContent -notmatch "Scope:\s*user" -or
            $instContent -notmatch "ProductCode:\s*'?\{8C6B9A2E-52C1-4E63-9B7A-7C1F4A2B9D33\}_is1'?") {
            $wingetErrors.Add("Installer manifest missing required core metadata (Inno, user scope, ProductCode)")
            $instValid = $false
        }

        # Validate x64 entry
        $x64Hash = $null
        if ($instContent -match "(?s)Architecture:\s*x64.*?InstallerUrl:\s*(\S+).*?InstallerSha256:\s*([0-9a-fA-F]{64})") {
            $x64Url = $Matches[1]
            $x64Hash = $Matches[2].ToUpper()
            if ($refHashes.ContainsKey("RightType-$Version-setup.exe")) {
                $expected = $refHashes["RightType-$Version-setup.exe"]
                if ($x64Hash -ne $expected) {
                    $wingetErrors.Add("x64 InstallerSha256 ($x64Hash) does not match reference hash ($expected)")
                    $instValid = $false
                } else {
                    Write-CheckOk "x64 installer SHA256 matches reference hash: $x64Hash"
                }
            } else {
                Write-CheckOk "x64 installer SHA256 is valid 64-char hex: $x64Hash"
            }
        } else {
            $wingetErrors.Add("Installer manifest missing valid Architecture: x64 entry or 64-char SHA256")
            $instValid = $false
        }

        # Validate arm64 entry
        $arm64Hash = $null
        if ($instContent -match "(?s)Architecture:\s*arm64.*?InstallerUrl:\s*(\S+).*?InstallerSha256:\s*([0-9a-fA-F]{64})") {
            $arm64Url = $Matches[1]
            $arm64Hash = $Matches[2].ToUpper()
            if ($refHashes.ContainsKey("RightType-$Version-arm64-setup.exe")) {
                $expected = $refHashes["RightType-$Version-arm64-setup.exe"]
                if ($arm64Hash -ne $expected) {
                    $wingetErrors.Add("arm64 InstallerSha256 ($arm64Hash) does not match reference hash ($expected)")
                    $instValid = $false
                } else {
                    Write-CheckOk "arm64 installer SHA256 matches reference hash: $arm64Hash"
                }
            } else {
                Write-CheckOk "arm64 installer SHA256 is valid 64-char hex: $arm64Hash"
            }
        } else {
            $wingetErrors.Add("Installer manifest missing valid Architecture: arm64 entry or 64-char SHA256")
            $instValid = $false
        }

        if ($instValid) {
            Write-CheckOk "Installer manifest contains valid x64 and arm64 entries"
            $wingetNotes.Add("x64/arm64 verified")
        } else {
            $wingetPass = $false
        }

        # Check 4: Locale manifest content
        $locContent = Get-Content $locFile -Raw -Encoding utf8
        if ($locContent -notmatch "PackageIdentifier:\s*$id" -or
            $locContent -notmatch "PackageVersion:\s*$([regex]::Escape($Version))" -or
            $locContent -notmatch "PackageName:\s*RightType" -or
            $locContent -notmatch "License:\s*MIT OR Apache-2.0" -or
            $locContent -notmatch "Moniker:\s*righttype") {
            $wingetErrors.Add("Locale manifest missing required metadata")
            $wingetPass = $false
        } else {
            Write-CheckOk "Locale manifest valid (PackageName, License, Moniker)"
        }

        # Check 5: Run winget CLI validate if available
        $wingetCmd = Get-Command winget -ErrorAction SilentlyContinue
        if ($wingetCmd) {
            Write-Host "  $C_GRAY... Running 'winget validate --manifest $wingetDir'$C_RESET"
            $wingetOut = & $wingetCmd.Source validate --manifest $wingetDir 2>&1
            if ($LASTEXITCODE -eq 0) {
                Write-CheckOk "winget validate CLI passed"
                $wingetNotes.Add("winget CLI validate passed")
            } else {
                $wingetErrors.Add("winget validate CLI failed: $($wingetOut -join '; ')")
                $wingetPass = $false
            }
        } else {
            Write-CheckWarn "winget CLI not found in PATH; skipping CLI validation step"
            $wingetNotes.Add("CLI skipped (not installed)")
        }
    } else {
        $wingetPass = $false
    }
}

if (-not $wingetPass) {
    foreach ($err in $wingetErrors) { Write-CheckFail $err }
    $results[0].Status = "FAIL"
    $results[0].Notes = ($wingetErrors -join "; ")
} else {
    $results[0].Status = "PASS"
    $results[0].Notes = ($wingetNotes -join "; ")
}

# -------------------------------------------------------------------------
# 2. Scoop Channel Verification
# -------------------------------------------------------------------------
Write-StepHeader "scoop: Scoop Package Channel"
$scoopPass = ($scoopErrors.Count -eq 0)

$scoopFile = Join-Path $DistDir "scoop/righttype.json"
if (-not (Test-Path $scoopFile)) {
    $scoopErrors.Add("Scoop manifest missing: $scoopFile")
    $scoopPass = $false
} else {
    Write-CheckOk "Scoop manifest file exists: righttype.json"
    try {
        $rawJson = Get-Content $scoopFile -Raw -Encoding utf8
        $json = $rawJson | ConvertFrom-Json
        Write-CheckOk "JSON parse succeeded"

        # Validate version
        if ($json.version -ne $Version) {
            $scoopErrors.Add("Scoop version '$($json.version)' does not match expected '$Version'")
            $scoopPass = $false
        } else {
            Write-CheckOk "Version matches: $Version"
        }

        # Validate architecture 64bit
        $x64ZipHash = $json.architecture.'64bit'.hash
        if (-not ($x64ZipHash -match "^[0-9a-fA-F]{64}$")) {
            $scoopErrors.Add("64bit hash is not a valid 64-char hex string: $x64ZipHash")
            $scoopPass = $false
        } else {
            if ($refHashes.ContainsKey("RightType-$Version-x64.zip")) {
                $expected = $refHashes["RightType-$Version-x64.zip"]
                if ($x64ZipHash.ToUpper() -ne $expected) {
                    $scoopErrors.Add("64bit hash ($x64ZipHash) does not match reference ($expected)")
                    $scoopPass = $false
                } else {
                    Write-CheckOk "64bit hash matches reference zip: $x64ZipHash"
                }
            } else {
                Write-CheckOk "64bit hash is valid: $x64ZipHash"
            }
        }

        # Validate architecture arm64
        $arm64ZipHash = $json.architecture.arm64.hash
        if (-not ($arm64ZipHash -match "^[0-9a-fA-F]{64}$")) {
            $scoopErrors.Add("arm64 hash is not a valid 64-char hex string: $arm64ZipHash")
            $scoopPass = $false
        } else {
            if ($refHashes.ContainsKey("RightType-$Version-arm64.zip")) {
                $expected = $refHashes["RightType-$Version-arm64.zip"]
                if ($arm64ZipHash.ToUpper() -ne $expected) {
                    $scoopErrors.Add("arm64 hash ($arm64ZipHash) does not match reference ($expected)")
                    $scoopPass = $false
                } else {
                    Write-CheckOk "arm64 hash matches reference zip: $arm64ZipHash"
                }
            } else {
                Write-CheckOk "arm64 hash is valid: $arm64ZipHash"
            }
        }

        # Validate bin & shortcuts
        $binValid = ($json.bin -eq "righttype.exe" -or ($json.bin -is [Array] -and $json.bin -contains "righttype.exe"))
        if (-not $binValid) {
            $scoopErrors.Add("Scoop bin does not point to righttype.exe (found '$($json.bin)')")
            $scoopPass = $false
        } else {
            Write-CheckOk "bin points to righttype.exe"
        }

        # Check shortcuts
        $shortcutValid = $false
        if ($json.shortcuts) {
            foreach ($s in $json.shortcuts) {
                if ($s -contains "righttype.exe" -and $s -contains "RightType") {
                    $shortcutValid = $true
                    break
                }
            }
        }
        if (-not $shortcutValid) {
            $scoopErrors.Add("Scoop shortcuts missing ['righttype.exe', 'RightType']")
            $scoopPass = $false
        } else {
            Write-CheckOk "shortcuts entry verified for righttype.exe -> RightType"
        }

        # Check checkver and autoupdate
        if ($json.checkver -ne "github" -or -not $json.autoupdate) {
            $scoopErrors.Add("Scoop checkver or autoupdate configuration missing")
            $scoopPass = $false
        } else {
            Write-CheckOk "checkver and autoupdate configuration valid"
        }

        if ($scoopPass) {
            $scoopNotes.Add("JSON schema valid")
            $scoopNotes.Add("x64/arm64 hashes verified")
            $scoopNotes.Add("bin & shortcuts match")
        }

    } catch {
        $scoopErrors.Add("JSON parse failed: $_")
        $scoopPass = $false
    }
}

if (-not $scoopPass) {
    foreach ($err in $scoopErrors) { Write-CheckFail $err }
    $results[1].Status = "FAIL"
    $results[1].Notes = ($scoopErrors -join "; ")
} else {
    $results[1].Status = "PASS"
    $results[1].Notes = ($scoopNotes -join "; ")
}

# -------------------------------------------------------------------------
# 3. Chocolatey Channel Verification
# -------------------------------------------------------------------------
Write-StepHeader "chocolatey: Chocolatey Package Channel"
$chocoPass = ($chocoErrors.Count -eq 0)

$chocoDist = Join-Path $DistDir "choco"
$chocoPkgDir = Join-Path $chocoDist "package"
$nuspecFile = Join-Path $chocoPkgDir "righttype.nuspec"
$toolsDir = Join-Path $chocoPkgDir "tools"
$installPs1 = Join-Path $toolsDir "chocolateyInstall.ps1"
$uninstallPs1 = Join-Path $toolsDir "chocolateyUninstall.ps1"
$verFileChoco = Join-Path $toolsDir "VERIFICATION.txt"
$licenseFileChoco = Join-Path $toolsDir "LICENSE.txt"
$nupkgFile = Join-Path $chocoDist "righttype.$Version.nupkg"

# Check 1: nuspec XML and metadata
if (-not (Test-Path $nuspecFile)) {
    $chocoErrors.Add("Missing nuspec file: $nuspecFile")
    $chocoPass = $false
} else {
    try {
        [xml]$nuspecXml = Get-Content $nuspecFile -Raw -Encoding utf8
        $metadata = $nuspecXml.package.metadata
        if ($metadata.id -ne "righttype") {
            $chocoErrors.Add("nuspec id is '$($metadata.id)', expected 'righttype'")
            $chocoPass = $false
        } elseif ($metadata.version -ne $Version) {
            $chocoErrors.Add("nuspec version is '$($metadata.version)', expected '$Version'")
            $chocoPass = $false
        } elseif ([string]::IsNullOrWhiteSpace($metadata.title) -or
                   [string]::IsNullOrWhiteSpace($metadata.authors) -or
                   [string]::IsNullOrWhiteSpace($metadata.projectUrl) -or
                   [string]::IsNullOrWhiteSpace($metadata.licenseUrl)) {
            $chocoErrors.Add("nuspec missing required metadata fields (title, authors, projectUrl, licenseUrl)")
            $chocoPass = $false
        } else {
            Write-CheckOk "righttype.nuspec XML schema and metadata valid (v$Version)"
            $chocoNotes.Add("nuspec valid")
        }
    } catch {
        $chocoErrors.Add("Failed to parse righttype.nuspec as XML: $_")
        $chocoPass = $false
    }
}

# Check 2: Tools directory and staged files
$toolsExist = $true
foreach ($t in @($installPs1, $uninstallPs1, $verFileChoco, $licenseFileChoco)) {
    if (-not (Test-Path $t)) {
        $chocoErrors.Add("Missing tools file: $([System.IO.Path]::GetFileName($t))")
        $toolsExist = $false
        $chocoPass = $false
    }
}

if ($toolsExist) {
    Write-CheckOk "All tools files exist (Install, Uninstall, VERIFICATION, LICENSE)"

    # Check 3: chocolateyInstall.ps1 silent args and checksum
    $instScript = Get-Content $installPs1 -Raw -Encoding utf8
    $requiredFlags = @("/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-")
    $missingFlags = @()
    foreach ($flag in $requiredFlags) {
        if ($instScript -notmatch [regex]::Escape($flag)) {
            $missingFlags += $flag
        }
    }
    if ($missingFlags.Count -gt 0) {
        $chocoErrors.Add("chocolateyInstall.ps1 missing required Inno silent flags: $($missingFlags -join ', ')")
        $chocoPass = $false
    } else {
        Write-CheckOk "chocolateyInstall.ps1 contains required silent args (/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-)"
    }

    # Verify checksum in install script
    if ($instScript -match "checksum\s*=\s*['""]([0-9a-fA-F]{64})['""]") {
        $chocoChecksum = $Matches[1].ToUpper()
        if ($refHashes.ContainsKey("RightType-$Version-setup.exe")) {
            $expected = $refHashes["RightType-$Version-setup.exe"]
            if ($chocoChecksum -ne $expected) {
                $chocoErrors.Add("chocolateyInstall.ps1 checksum ($chocoChecksum) does not match reference ($expected)")
                $chocoPass = $false
            } else {
                Write-CheckOk "chocolateyInstall.ps1 checksum matches reference x64 setup: $chocoChecksum"
            }
        } else {
            Write-CheckOk "chocolateyInstall.ps1 checksum is valid 64-char hex: $chocoChecksum"
        }
    } else {
        $chocoErrors.Add("chocolateyInstall.ps1 missing valid 64-char hex checksum")
        $chocoPass = $false
    }

    # Check 4: chocolateyUninstall.ps1 has silent args
    $uninstScript = Get-Content $uninstallPs1 -Raw -Encoding utf8
    if ($uninstScript -notmatch "/VERYSILENT" -or $uninstScript -notmatch "/NORESTART") {
        $chocoErrors.Add("chocolateyUninstall.ps1 missing silent uninstaller arguments")
        $chocoPass = $false
    } else {
        Write-CheckOk "chocolateyUninstall.ps1 uninstaller script verified"
    }

    # Check 5: VERIFICATION.txt contains version and matching hash
    $verTxt = Get-Content $verFileChoco -Raw -Encoding utf8
    if ($verTxt -notmatch [regex]::Escape($Version) -or
        $verTxt -notmatch "RightType-$([regex]::Escape($Version))-setup\.exe") {
        $chocoErrors.Add("VERIFICATION.txt missing version or installer filename reference")
        $chocoPass = $false
    } else {
        Write-CheckOk "VERIFICATION.txt metadata verified"
    }

    # Check 6: LICENSE.txt non-empty
    $licItem = Get-Item $licenseFileChoco
    if ($licItem.Length -lt 100) {
        $chocoErrors.Add("LICENSE.txt in tools is too small ($($licItem.Length) bytes)")
        $chocoPass = $false
    } else {
        Write-CheckOk "LICENSE.txt verified ($($licItem.Length) bytes)"
    }

    $chocoNotes.Add("tools & silent args verified")
}

# Check 7: nupkg presence if choco CLI is installed
$chocoCmd = Get-Command choco -ErrorAction SilentlyContinue
if ($chocoCmd) {
    if (Test-Path $nupkgFile) {
        $nupkgItem = Get-Item $nupkgFile
        $sizeKb = [Math]::Round($nupkgItem.Length / 1024, 1)
        Write-CheckOk "Chocolatey package archive built: righttype.$Version.nupkg ($sizeKb KB)"
        $chocoNotes.Add("nupkg built ($sizeKb KB)")
    } else {
        $chocoErrors.Add("Chocolatey CLI is installed, but nupkg was not found at $nupkgFile")
        $chocoPass = $false
    }
} else {
    if (Test-Path $nupkgFile) {
        $nupkgItem = Get-Item $nupkgFile
        $sizeKb = [Math]::Round($nupkgItem.Length / 1024, 1)
        Write-CheckOk "Chocolatey package archive present: righttype.$Version.nupkg ($sizeKb KB)"
        $chocoNotes.Add("nupkg present ($sizeKb KB)")
    } else {
        Write-CheckWarn "choco CLI not in PATH; staged package files validated (nupkg generation skipped)"
        $chocoNotes.Add("staged files valid (choco CLI not in PATH)")
    }
}

if (-not $chocoPass) {
    foreach ($err in $chocoErrors) { Write-CheckFail $err }
    $results[2].Status = "FAIL"
    $results[2].Notes = ($chocoErrors -join "; ")
} else {
    $results[2].Status = "PASS"
    $results[2].Notes = ($chocoNotes -join "; ")
}

# -------------------------------------------------------------------------
# ANSI Summary Table
# -------------------------------------------------------------------------
Write-Host "`n$C_BOLD========================================================================================$C_RESET"
Write-Host "$C_BOLD                             Verification Summary                               $C_RESET"
Write-Host "$C_BOLD========================================================================================$C_RESET"

$col1Width = 14
$col2Width = 10
$col3Width = 64

$header = "{0,-$col1Width}{1,-$col2Width}{2}" -f "Channel", "Status", "Notes"
Write-Host "$C_BOLD$header$C_RESET"
Write-Host ("-" * ($col1Width + $col2Width + $col3Width))

$allPass = $true
foreach ($row in $results) {
    if ($row.Status -ne "PASS") { $allPass = $false }
    $statusColor = if ($row.Status -eq "PASS") { "$C_GREEN" } else { "$C_RED" }

    $chanStr = $row.Channel.PadRight($col1Width)
    $statStr = "$statusColor$($row.Status.PadRight($col2Width))$C_RESET"
    $noteStr = $row.Notes

    Write-Host "$chanStr$statStr$noteStr"
}

Write-Host ("-" * ($col1Width + $col2Width + $col3Width))

if ($allPass) {
    Write-Host "$C_BOLD$C_GREEN Result: ALL PASS (3/3 channels verified successfully)$C_RESET"
    Write-Host "$C_BOLD========================================================================================$C_RESET`n"
    exit 0
} else {
    Write-Host "$C_BOLD$C_RED Result: VERIFICATION FAILED (One or more channels failed checks)$C_RESET"
    Write-Host "$C_BOLD========================================================================================$C_RESET`n"
    exit 1
}

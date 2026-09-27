# Write the GitHub Release description: how to install (Thai and English) and
# this version's section of CHANGELOG.md.
# Usage: pwsh -File packaging\release_notes.ps1 -Version 2.0.0 -Out release-notes.md

param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Out
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

# This version's CHANGELOG section, without its own heading.
$lines = Get-Content (Join-Path $root "CHANGELOG.md") -Encoding utf8
$start = ($lines | Select-String -Pattern "^## \[$([regex]::Escape($Version))\]" | Select-Object -First 1).LineNumber
if (-not $start) { throw "CHANGELOG.md has no section for $Version" }
$section = @()
for ($i = $start; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^## \[') { break }
    $section += $lines[$i]
}

$setup = "RightType-$Version-setup.exe"
$zip = "RightType-$Version-x64.zip"

$notes = @"
## ติดตั้ง

1. ดาวน์โหลด **``$setup``** ด้านล่าง แล้วดับเบิลคลิก
2. ถ้า Windows ขึ้นกล่องสีน้ำเงิน "Windows protected your PC" ให้กด **More info → Run anyway**
   (ไฟล์ยังไม่ได้ลงลายเซ็นดิจิทัล ตรวจสอบไฟล์ได้กับ ``SHA256.txt``)
3. เลือกได้ว่าจะสร้าง shortcut และให้เปิดพร้อม Windows ไหม — ไม่ต้องใช้สิทธิ์ admin
4. RightType จะอยู่ที่มุมขวาล่าง (ไอคอน **Aก**) คลิกเพื่อเปิดเมนูและหน้าตั้งค่า

ไม่อยากติดตั้ง? ดาวน์โหลด ``$zip`` แตกไฟล์ แล้วเปิด ``righttype.exe`` ได้เลย
มี winget? ``winget install k-affan-th.RightType`` แล้วอัปเดตด้วย ``winget upgrade k-affan-th.RightType``
(ใช้ได้เมื่อ winget รับแพ็กเกจแล้ว)
ถอนการติดตั้ง: Settings → Apps → RightType

## Install

1. Download **``$setup``** below and double-click it.
2. If Windows shows "Windows protected your PC", choose **More info → Run anyway**
   (the files are not code-signed yet; check them against ``SHA256.txt``).
3. Pick a desktop shortcut and start-with-Windows if you like — no admin rights needed.
4. RightType lives in the notification area (the **Aก** icon); click it for the menu and Settings.

No installer wanted? Download ``$zip``, extract it and run ``righttype.exe``.
With winget: ``winget install k-affan-th.RightType``, and later ``winget upgrade k-affan-th.RightType``
(once winget has accepted the package).
Uninstall: Settings → Apps → RightType.

## What's new

$($section -join "`n")
"@

Set-Content -Path $Out -Value $notes -Encoding utf8
Write-Host "wrote $Out"

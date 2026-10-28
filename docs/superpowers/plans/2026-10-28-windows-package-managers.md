# Windows Package Managers Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide automated manifest generation, packaging scripts, and submission tooling for RightType across **winget**, **Scoop**, and **Chocolatey** on Windows (x64 & ARM64).

**Architecture:** PowerShell automation scripts in `packaging/` read release artifacts or local builds, calculate SHA-256 hashes, generate spec-compliant manifests (YAML, JSON, NUSPEC), and package `.nupkg` artifacts into `dist/`. A unified verification script tests manifest schemas, and a distribution guide documents exact commands and PR workflows for maintainers.

**Tech Stack:** PowerShell 7+ (`pwsh`), winget manifest schema 1.6.0, Scoop manifest schema, Chocolatey (`choco pack`), Inno Setup silent flags.

**Spec:** `docs/superpowers/specs/2026-10-28-windows-package-managers-design.md`

## Global Constraints

- Platform: Windows 10 (1809+) and Windows 11 on both `x64` and `arm64`.
- Silent install flags: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-` for Inno Setup installer.
- Product code: `{8C6B9A2E-52C1-4E63-9B7A-7C1F4A2B9D33}_is1`.
- Scope: `user` (per-user installation, elevation not required).
- Repository URL: `https://github.com/k-affan-th/RightType`.
- No modification to `src/` core or Windows integration logic.
- Target baseline version: `2.6.3`.

## Review Focus

- Missing ARM64 installer entry in winget installer manifest causes ARM64 machines to fail or fall back to x64 emulation; installer manifest must explicitly define both `x64` and `arm64`.
- Scoop path or shortcuts pointing to wrong executable name breaks start menu shortcut or execution; must point to `righttype.exe`.
- Incorrect Chocolatey silent arguments causes Inno Setup wizard dialogs to hang headless background installs; `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-` must be preserved.
- Hash mismatch between generated manifests and actual GitHub Release assets causes package managers to abort install; scripts must resolve correct release SHA256 hashes.
- Incomplete uninstall script leaves leftover registry entries or running processes; uninstallers must invoke silent uninstall properly.

---

### Task 1: Enhance winget Manifest Generator for ARM64 & Remote Hashes

**Files:**
- Modify: `packaging/winget.ps1`
- Test: `packaging/winget.ps1`

**Interfaces:**
- Consumes: Version string, optional local setup path or GitHub release SHA256.txt.
- Produces: `dist/winget/<version>/k-affan-th.RightType.yaml`, `...installer.yaml`, `...locale.en-US.yaml`.

- [ ] **Step 1: Write verification test command for winget generator**

Test script run:
```powershell
pwsh -File packaging/winget.ps1 -Version 2.6.3 -Out dist/test-winget
```
Expected before update: Produces x64 installer only; fails if local `dist/RightType-2.6.3-setup.exe` is absent.

- [ ] **Step 2: Update `packaging/winget.ps1`**

Support:
- Parameters: `-Version`, `-SetupX64`, `-SetupArm64`, `-Out`.
- If local setup files are not provided, read `dist/SHA256.txt` if present or fetch release `SHA256.txt` from `https://github.com/k-affan-th/RightType/releases/download/v$Version/SHA256.txt`.
- Generate `Installers` block with both `x64` and `arm64` entries:
  - `x64`: `RightType-$Version-setup.exe`
  - `arm64`: `RightType-$Version-arm64-setup.exe`
- UTF-8 without BOM writing.

- [ ] **Step 3: Run generator and verify YAML structure**

Run:
```powershell
pwsh -File packaging/winget.ps1 -Version 2.6.3 -Out dist/winget
```
Verify `dist/winget/2.6.3/k-affan-th.RightType.installer.yaml` contains both `Architecture: x64` and `Architecture: arm64` with valid 64-character SHA-256 hex strings.

- [ ] **Step 4: Commit**

```bash
git add packaging/winget.ps1
git commit -m "feat(packaging): add ARM64 support and remote hash resolution to winget.ps1"
```

---

### Task 2: Implement Scoop Manifest Generator & Custom Bucket Structure

**Files:**
- Create: `packaging/scoop.ps1`
- Create: `packaging/scoop/README.md`
- Test: `dist/scoop/righttype.json`

**Interfaces:**
- Consumes: Target version string (`2.6.3`).
- Produces: `dist/scoop/righttype.json` compliant with Scoop manifest schema.

- [ ] **Step 1: Implement `packaging/scoop.ps1`**

Script logic:
- Parameters: `[Parameter(Mandatory)][string]$Version`, `[string]$Out = "dist/scoop"`
- Resolve SHA-256 for `RightType-$Version-x64.zip` and `RightType-$Version-arm64.zip` (from local file or remote release).
- Construct JSON object:
  ```json
  {
    "version": "$Version",
    "description": "Fix wrong-keyboard-layout Thai/English typing — a modern, safe Rust successor to RightLang",
    "homepage": "https://github.com/k-affan-th/RightType",
    "license": "MIT OR Apache-2.0",
    "architecture": {
      "64bit": {
        "url": "https://github.com/k-affan-th/RightType/releases/download/v$Version/RightType-$Version-x64.zip",
        "hash": "$shaX64"
      },
      "arm64": {
        "url": "https://github.com/k-affan-th/RightType/releases/download/v$Version/RightType-$Version-arm64.zip",
        "hash": "$shaArm64"
      }
    },
    "bin": "righttype.exe",
    "shortcuts": [
      ["righttype.exe", "RightType"]
    ],
    "checkver": "github",
    "autoupdate": {
      "architecture": {
        "64bit": {
          "url": "https://github.com/k-affan-th/RightType/releases/download/v$version/RightType-$version-x64.zip"
        },
        "arm64": {
          "url": "https://github.com/k-affan-th/RightType/releases/download/v$version/RightType-$version-arm64.zip"
        }
      }
    }
  }
  ```
- Write formatted JSON to `$Out/righttype.json` in UTF-8.

- [ ] **Step 2: Create `packaging/scoop/README.md`**

Instructions explaining how to host `k-affan-th/scoop-bucket`:
- Repository structure: `bucket/righttype.json`
- User usage:
  ```powershell
  scoop bucket add righttype https://github.com/k-affan-th/scoop-bucket
  scoop install righttype
  ```

- [ ] **Step 3: Run generator and validate JSON schema**

Run:
```powershell
pwsh -File packaging/scoop.ps1 -Version 2.6.3
Get-Content dist/scoop/righttype.json | ConvertFrom-Json
```
Expected: Valid JSON object with correct URLs, hashes, bin, and shortcuts.

- [ ] **Step 4: Commit**

```bash
git add packaging/scoop.ps1 packaging/scoop/README.md
git commit -m "feat(packaging): add Scoop manifest generator and custom bucket docs"
```

---

### Task 3: Implement Chocolatey Packaging Templates & Generator

**Files:**
- Create: `packaging/choco/righttype.nuspec.template`
- Create: `packaging/choco/tools/chocolateyInstall.ps1.template`
- Create: `packaging/choco/tools/chocolateyUninstall.ps1`
- Create: `packaging/choco/tools/LICENSE.txt`
- Create: `packaging/choco/tools/VERIFICATION.txt.template`
- Create: `packaging/choco.ps1`
- Test: `dist/choco/righttype.2.6.3.nupkg`

**Interfaces:**
- Consumes: Target version string (`2.6.3`).
- Produces: `dist/choco/package/` ready for `choco pack`, and `dist/choco/righttype.<version>.nupkg`.

- [ ] **Step 1: Create Chocolatey template files**

1. `packaging/choco/righttype.nuspec.template`:
   - Package metadata: id `righttype`, version `__VERSION__`, title `RightType`, authors `k-affan-th`, projectUrl, packageSourceUrl, licenseUrl, docsUrl, tags (`thai`, `keyboard`, `typing`, `layout`).
2. `packaging/choco/tools/chocolateyInstall.ps1.template`:
   - Package arguments for Inno Setup:
     - `fileType = 'exe'`
     - `url = 'https://github.com/k-affan-th/RightType/releases/download/v__VERSION__/RightType-__VERSION__-setup.exe'`
     - `url64bit = 'https://github.com/k-affan-th/RightType/releases/download/v__VERSION__/RightType-__VERSION__-setup.exe'`
     - `checksum = '__SHA256_X64__'`
     - `checksum64 = '__SHA256_X64__'`
     - `checksumType = 'sha256'`
     - `silentArgs = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-'`
     - `validExitCodes = @(0)`
3. `packaging/choco/tools/chocolateyUninstall.ps1`:
   - Detects uninstall string in registry `{8C6B9A2E-52C1-4E63-9B7A-7C1F4A2B9D33}_is1` and runs silently.
4. `packaging/choco/tools/VERIFICATION.txt.template`:
   - Describes SHA256 checksums and verification against GitHub release.

- [ ] **Step 2: Create `packaging/choco.ps1`**

Script logic:
- Accepts `-Version`, `-Out`.
- Resolves SHA256 hashes for v2.6.3 setup.exe.
- Replaces tokens (`__VERSION__`, `__SHA256_X64__`) in templates and writes staging directory `dist/choco/package/`.
- Copies `LICENSE-MIT` to `dist/choco/package/tools/LICENSE.txt`.
- Invokes `choco pack` if `choco` CLI is installed, or outputs zip/nupkg instructions if not installed.

- [ ] **Step 3: Run `packaging/choco.ps1` and verify output**

Run:
```powershell
pwsh -File packaging/choco.ps1 -Version 2.6.3
```
Verify generated `.nuspec` and install scripts in `dist/choco/package/`.

- [ ] **Step 4: Commit**

```bash
git add packaging/choco/ packaging/choco.ps1
git commit -m "feat(packaging): add Chocolatey packaging templates and choco.ps1 builder"
```

---

### Task 4: Unified Distribution Verification Script

**Files:**
- Create: `packaging/verify_packages.ps1`
- Test: Run verification for 2.6.3

**Interfaces:**
- Consumes: Target version (`2.6.3`).
- Produces: Validation report for winget, Scoop, and Chocolatey manifests.

- [ ] **Step 1: Implement `packaging/verify_packages.ps1`**

The script runs all 3 packaging generators (`winget.ps1`, `scoop.ps1`, `choco.ps1`):
1. Verifies all output files exist in `dist/`.
2. Validates JSON parsing on `dist/scoop/righttype.json`.
3. Validates YAML structure on all three winget manifests.
4. Checks that URLs and checksums match `SHA256.txt` from release v2.6.3.
5. If `winget` tool is installed, runs `winget validate --manifest dist/winget/<version>`.
6. Prints status summary table.

- [ ] **Step 2: Run verification script**

Run:
```powershell
pwsh -File packaging/verify_packages.ps1 -Version 2.6.3
```
Expected output: All 3 package formats PASS verification.

- [ ] **Step 3: Commit**

```bash
git add packaging/verify_packages.ps1
git commit -m "feat(packaging): add unified package verification script"
```

---

### Task 5: Maintainer Distribution & Submission Guide

**Files:**
- Create: `docs/distribution/submission-guide.md`
- Test: Review links and commands

**Interfaces:**
- Produces: Complete operational guide for submitting RightType v2.6.3 to winget-pkgs, Scoop custom bucket, and Chocolatey.

- [ ] **Step 1: Write `docs/distribution/submission-guide.md`**

Sections:
1. **winget Submission**:
   - Method A (Automatic via CLI):
     ```powershell
     wingetcreate submit dist/winget/2.6.3
     ```
   - Method B (Manual Fork PR):
     - Fork `microsoft/winget-pkgs`.
     - Copy `dist/winget/2.6.3` to `manifests/k/k-affan-th/RightType/2.6.3/`.
     - Open PR with title: `New package: k-affan-th.RightType version 2.6.3`.
2. **Scoop Custom Bucket Setup**:
   - Create GitHub repository `k-affan-th/scoop-bucket`.
   - Copy `dist/scoop/righttype.json` into `bucket/righttype.json`.
   - Document user installation command:
     ```powershell
     scoop bucket add righttype https://github.com/k-affan-th/scoop-bucket
     scoop install righttype
     ```
   - Note criteria for `ScoopInstaller/Extras` submission (100 stars milestone).
3. **Chocolatey Submission**:
   - Get API key from `https://community.chocolatey.org/account`.
   - Pack and push:
     ```powershell
     choco apikey --key <API_KEY> --source https://push.chocolatey.org/
     choco push dist/choco/righttype.2.6.3.nupkg --source https://push.chocolatey.org/
     ```
   - What happens during Chocolatey moderation.
4. **Phase 2 Roadmap: Microsoft Store & MSIX**:
   - Overview of MSIX packaging tool requirements and store listing guidelines.

- [ ] **Step 2: Commit**

```bash
git add docs/distribution/submission-guide.md
git commit -m "docs(distribution): add maintainer submission guide for winget, scoop, and chocolatey"
```

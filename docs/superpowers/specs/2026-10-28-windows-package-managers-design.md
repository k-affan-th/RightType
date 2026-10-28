# RightType Windows Distribution & Package Managers Design

**Date:** 2026-10-28  
**Target Release:** v2.6.3  
**Branch:** `beta`  
**Status:** Ready for Approval  

---

## 1. Intent & The Why

RightType is a modern, lightweight, privacy-focused Windows tray application for automatic and hotkey Thai/English keyboard layout correction. Currently, users must manually download the setup executable or portable zip from GitHub Releases. 

To expand reach, simplify discovery, and streamline installation/updates for developers and power users on Windows, RightType will be distributed through the primary Windows package managers:
1. **winget** (Microsoft's default Windows Package Manager)
2. **Scoop** (Command-line installer popular among developers for portable tools)
3. **Chocolatey** (Widely used Windows package manager)

*(Note: Microsoft Store distribution requires MSIX packaging to utilize Microsoft's free code-signing; this is designated as Phase 2 to avoid blocking the CLI package manager releases).*

---

## 2. Goals, Non-Goals & Anti-Goals

### Goals
- **winget**:
  - Update `packaging/winget.ps1` to include ARM64 installers alongside x64.
  - Generate and validate v2.6.3 manifests (`winget validate`).
  - Provide automated PR submission command using `wingetcreate submit` (or instructions for manual PR into `microsoft/winget-pkgs`).
- **Scoop**:
  - Create `packaging/scoop.ps1` to generate standard Scoop manifest (`dist/scoop/righttype.json`) targeting both x64 and ARM64 zip releases.
  - Set up a standalone custom bucket structure (`bucket/righttype.json` in a dedicated repo or branch) so users can install immediately via `scoop bucket add righttype <url>` and `scoop install righttype`.
  - Ensure the manifest schema complies with `ScoopInstaller/Extras` standards for future submission when the 100-star threshold is reached.
- **Chocolatey**:
  - Create packaging templates in `packaging/choco/`:
    - `righttype.nuspec` (metadata, tags, license, release notes).
    - `tools/chocolateyInstall.ps1` (silent installer execution with Inno Setup flags).
    - `tools/chocolateyUninstall.ps1` (clean registry/uninstaller invocation).
  - Create `packaging/choco.ps1` to pack (`choco pack`) and test local installation.
- **Branch Isolation**: All packaging scripts, templates, and documentation will be developed and verified on branch `beta`.

### Non-Goals
- **Immediate Microsoft Store / MSIX Conversion (Phase 2)**: Unpackaged `.exe` submission to Microsoft Store requires a commercial Authenticode certificate ($200–$400/yr). Because Microsoft provides free code-signing for MSIX packages, MSIX packaging will be handled in a dedicated subsequent phase.
- **Linux Porting**: Developing Linux hook and injection infrastructure is a separate project.
- **GitHub Actions Auto-Publishing (Secrets)**: Auto-publishing secrets (`CHOCO_API_KEY`, etc.) will not be hardcoded or required upfront; local verification and manual PR submission will establish the baseline first.

### Anti-Goals
- Modifying core application logic or Windows OS integration code (`src/`). Packaging changes must not alter the binary or its runtime behavior.
- Hardcoding secrets or personal API tokens in git commits.

---

## 3. Constraints

- **Platform**: Windows 10 (version 1809+) and Windows 11 (x64 and ARM64).
- **Existing Release Artifacts** (`v2.6.3`):
  - `RightType-2.6.3-setup.exe` (Inno Setup x64)
  - `RightType-2.6.3-arm64-setup.exe` (Inno Setup ARM64)
  - `RightType-2.6.3-x64.zip` (Portable x64)
  - `RightType-2.6.3-arm64.zip` (Portable ARM64)
- **Permissions**:
  - Inno Setup installs to user profile (`Scope: user`), requiring no admin elevation.
  - Inno Setup AppId: `{8C6B9A2E-52C1-4E63-9B7A-7C1F4A2B9D33}`.
  - Silent flags: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-`.

---

## 4. Approaches Considered

### Approach 1: Local-First Staged Manifests & Packaging Tooling (Selected)
- **Description**: Implement packaging generators and templates in `packaging/`, generate all manifests locally on `beta`, test installation and uninstallation locally, and submit to community repositories step-by-step.
- **Trade-offs**: Requires initial manual PR creation and manual submission to dashboards, but guarantees that manifest formats, checksums, and silent arguments are verified before automating. Avoids premature CI failure due to missing third-party API tokens.
- **Decision**: Selected by user.

### Approach 2: Immediate GitHub Actions CI/CD Integration
- **Description**: Directly wire Scoop and Chocolatey publishing into `.github/workflows/release.yml` with secrets.
- **Trade-offs**: Faster long-term automation, but risky if tokens are missing or if initial manual moderation flags issues.
- **Decision**: Rejected for the first release cycle. Can be integrated after Approach 1 succeeds.

---

## 5. Implementation Specifications

### 5.1 winget (`packaging/winget.ps1`)
- Update `packaging/winget.ps1`:
  - Add ARM64 installer entry to `installerManifest` alongside x64 (both pointing to GitHub Release URLs and computing SHA-256).
  - Produce three yaml files in `dist/winget/<version>/`:
    - `k-affan-th.RightType.yaml` (Version manifest)
    - `k-affan-th.RightType.installer.yaml` (Installer manifest for x64 + arm64)
    - `k-affan-th.RightType.locale.en-US.yaml` (DefaultLocale manifest)
- Submission:
  - Validate with `winget validate --manifest dist/winget/<version>`.
  - Provide command to submit: `wingetcreate submit dist/winget/<version>` (or instructions for manual fork/PR to `microsoft/winget-pkgs`).

### 5.2 Scoop (`packaging/scoop.ps1` & Standalone Bucket)
- Script: `packaging/scoop.ps1 -Version <version>`
- Manifest fields (`dist/scoop/righttype.json`):
  - `version`: target version (e.g., `2.6.3`)
  - `description`: "Fix wrong-keyboard-layout Thai/English typing — a modern, safe Rust successor to RightLang"
  - `homepage`: `https://github.com/k-affan-th/RightType`
  - `license`: `MIT OR Apache-2.0`
  - `architecture`:
    - `64bit`: url `https://github.com/k-affan-th/RightType/releases/download/v<version>/RightType-<version>-x64.zip`, hash sha256
    - `arm64`: url `https://github.com/k-affan-th/RightType/releases/download/v<version>/RightType-<version>-arm64.zip`, hash sha256
  - `bin`: `righttype.exe`
  - `shortcuts`: `[["righttype.exe", "RightType"]]`
  - `checkver`: `github`
  - `autoupdate`: architecture URL templates for future automated updates.
- Custom Bucket Setup:
  - Document setup for `https://github.com/k-affan-th/scoop-bucket` (or a `scoop` orphan branch) containing `bucket/righttype.json`.
  - Installation command:
    ```powershell
    scoop bucket add righttype https://github.com/k-affan-th/scoop-bucket
    scoop install righttype
    ```

### 5.3 Chocolatey (`packaging/choco/` & `packaging/choco.ps1`)
- Structure:
  - `packaging/choco/righttype.nuspec`:
    - id: `righttype`
    - version: `2.6.3`
    - title: `RightType`
    - authors: `k-affan-th`
    - projectUrl: `https://github.com/k-affan-th/RightType`
    - tags: `thai keyboard typing layout rightlang auto-switch`
  - `packaging/choco/tools/chocolateyInstall.ps1`:
    - Uses `Install-ChocolateyPackage`.
    - Handles x64 and ARM64 setup URLs with SHA256 hashes.
    - Silent arguments: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-`.
    - Installer type: `inno`.
  - `packaging/choco/tools/chocolateyUninstall.ps1`:
    - Uses `Uninstall-ChocolateyPackage` targeting `{8C6B9A2E-52C1-4E63-9B7A-7C1F4A2B9D33}_is1`.
- Script `packaging/choco.ps1 -Version <version>`:
  - Generates `dist/choco/righttype.<version>.nupkg` via `choco pack`.
  - Provides verification command: `choco install righttype --source dist/choco --force`.

---

## 6. What's Left to the Builder

- Implement `packaging/scoop.ps1` and template.
- Implement `packaging/choco.ps1` and template files in `packaging/choco/`.
- Enhance `packaging/winget.ps1` with ARM64 support.
- Generate manifests for v2.6.3 in `dist/`.
- Validate package functionality on Windows.
- Provide a clear, actionable submission guide (`docs/distribution/submission-guide.md`) for submitting to `winget-pkgs`, setting up the Scoop bucket, and pushing to Chocolatey.

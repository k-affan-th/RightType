# Statically Link CRT and Update WinGet PR Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable static C runtime linkage (`+crt-static`) in RightType to eliminate `VCRUNTIME140.dll` dependency, rebuild the release artifacts for v2.6.2, update GitHub Release v2.6.2, and update the SHA-256 manifest in WinGet PR #463113.

**Architecture:** Add `.cargo/config.toml` configuring `target-feature=+crt-static` for `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`. Rebuild `righttype.exe` and the Inno Setup installer `RightType-2.6.2-setup.exe`. Upload updated assets to GitHub Release `v2.6.2` and update the installer manifest on the PR fork branch to trigger clean WinGet validation.

**Tech Stack:** Rust (cargo, rustc), Inno Setup 6 (ISCC), PowerShell, GitHub CLI (`gh`), Git.

## Global Constraints

- Must eliminate all imports of `VCRUNTIME140.dll` from the compiled `righttype.exe`.
- Must produce matching SHA-256 checksums in `dist/SHA256.txt` and `k-affan-th.RightType.installer.yaml`.
- Manifest format must conform strictly to WinGet schema 1.6.0.

---

### Task 1: Configure Static CRT in `.cargo/config.toml`

**Files:**
- Create: `.cargo/config.toml`

- [ ] **Step 1: Create `.cargo/config.toml` with `+crt-static` flags**
Configure rustflags for both x86_64 and aarch64 MSVC targets:
```toml
[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+crt-static"]

[target.aarch64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+crt-static"]
```

- [ ] **Step 2: Verify binary imports with static CRT**
Build release:
`cargo build --release --features winos`
Verify with python script that `VCRUNTIME140.dll` is not present in PE import directory.

- [ ] **Step 3: Commit `.cargo/config.toml` to RightType repository**
```bash
git add .cargo/config.toml
git commit -m "build: statically link C runtime (+crt-static) to eliminate vcruntime140 dependency"
git push origin main
```

---

### Task 2: Rebuild Release 2.6.2 Artifacts

**Files:**
- Run: `packaging/build_release.ps1`
- Target outputs: `dist/RightType-2.6.2-setup.exe`, `dist/RightType-2.6.2-x64.zip`, `dist/SHA256.txt`

- [ ] **Step 1: Execute build_release.ps1**
Run `pwsh -File packaging/build_release.ps1 -Arch x64`.
Verify `dist/RightType-2.6.2-setup.exe` is generated.

- [ ] **Step 2: Verify installer hash and no runtime dependency**
Compute SHA-256 of `dist/RightType-2.6.2-setup.exe`.
Confirm hash is recorded in `dist/SHA256.txt`.

---

### Task 3: Update GitHub Release v2.6.2 Assets

**Files:**
- Modify assets in GitHub Release `v2.6.2` via `gh release upload`

- [ ] **Step 1: Upload rebuilt assets to Release v2.6.2**
Upload `dist/RightType-2.6.2-setup.exe`, `dist/RightType-2.6.2-x64.zip`, and `dist/SHA256.txt` using `gh release upload v2.6.2 ... --clobber`.

- [ ] **Step 2: Verify uploaded asset hashes**
Download or verify hash of the asset on GitHub releases matches local build.

---

### Task 4: Update WinGet PR #463113 Manifest

**Files:**
- Modify: `manifests/k/k-affan-th/RightType/2.6.2/k-affan-th.RightType.installer.yaml` on branch `k-affan-th.RightType-2.6.2-14a84d02-276e-41dc-ad59-38061a8e071e` of `k-affan-th/winget-pkgs`.

- [ ] **Step 1: Update InstallerSha256 in the manifest**
Update `InstallerSha256` in `k-affan-th.RightType.installer.yaml` with the new hash.

- [ ] **Step 2: Commit and push to PR branch**
Commit the update to `k-affan-th/winget-pkgs` on branch `k-affan-th.RightType-2.6.2-14a84d02-276e-41dc-ad59-38061a8e071e`.

- [ ] **Step 3: Verify PR #463113 reflects new commit and CI triggers**
Run `gh pr view 463113 --repo microsoft/winget-pkgs` to verify new commit is registered and Azure validation pipeline restarts.

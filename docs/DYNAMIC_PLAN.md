# RightType Dynamic Execution Plan

> เอกสารนี้เป็น **source of truth สำหรับการลงมือทำงานปัจจุบัน**
> ส่วน [PLAN.md](PLAN.md) เก็บ vision, architecture และประวัติการวางแผนระยะยาว

## Current control panel

| Field | Value |
| --- | --- |
| Overall status | `IN_PROGRESS — EXTERNAL UNBLOCK REQUIRED` |
| Active section | `S4/S5 — Windows platform matrix` |
| Next action | เปิด exact US QWERTY และช่วง hands-off สำหรับ Word/browser E2E; จากนั้นอนุมัติ sleep/lock/UAC transition tests |
| Current release target | `v1: Windows, Thai Kedmanee ↔ US English QWERTY` |
| Last updated | `2026-08-24` |
| Last verified baseline | 76 tests, clippy `-D warnings`, fmt/diff check, release build, latency gate และ RustSec `cargo audit` ผ่านบน working tree ปัจจุบัน |
| Worktree note | implementation changes ทั้งหมดถูก commit แล้ว (`HEAD` = baseline); งานที่เปิดคือ external unblock checklist เท่านั้น |

### Status legend

- `[ ] TODO` — ยังไม่เริ่ม
- `[~] IN_PROGRESS` — กำลังทำ และต้องระบุ next action
- `[?] DECISION` — ห้าม implement ต่อจน decision owner ปิดคำถาม
- `[!] BLOCKED` — ระบุ blocker และเงื่อนไขปลดล็อก
- `[x] VERIFIED` — มี evidence ตาม exit criteria แล้วเท่านั้น
- `[-] DEFERRED` — ตัดออกจาก release นี้โดยมีเหตุผลและ destination release

### External unblock checklist

งาน implementation/automated ที่ทำได้โดยไม่เปลี่ยน environment ปิดแล้ว เหลือ:

1. เปิดใช้งาน **US English QWERTY (`00000409`)** คู่กับ Thai Kedmanee ใน Windows test session; environment ปัจจุบันคืนเฉพาะ supported Thai handle จึงทดสอบ Auto/Suggest สองทิศทางไม่ได้
2. ให้ช่วง hands-off กับ blank Word document และ Chrome/contenteditable target; รอบล่าสุด Word automation หยุดเพราะตรวจพบ user input และไม่มี Chrome target
3. อนุมัติการทดสอบที่รบกวน session: sleep/resume, lock/unlock และ UAC secure desktop (ผู้ใช้ต้อง unlock/รับช่วง UAC เอง)
4. ระบุ release version/commit และ certificate เมื่อ platform matrix ผ่าน; agent จะไม่ commit/tag/sign จาก working tree ที่มี user changes โดยเดาเอง

## How this plan stays dynamic

ทุก change ที่อ้างว่าอยู่ภายใต้แผนนี้ต้องอัปเดตไฟล์นี้ใน change เดียวกัน:

1. เปลี่ยน `Active section`, `Next action` และ `Last updated` ที่ control panel
2. อัปเดต task state โดยห้ามใช้ `[x]` จากการอ่านโค้ดหรือ compile อย่างเดียว
3. เพิ่มหลักฐานใน `Evidence ledger`: command, ผลลัพธ์, วันที่ และสิ่งที่หลักฐานนั้น **ยังไม่พิสูจน์**
4. ถ้า behavior เปลี่ยน ให้เพิ่มหรือแก้ decision record ก่อนแก้ README/spec
5. ถ้าพบงานใหม่ ให้ใส่ใน section ที่เหมาะสมพร้อม priority; ห้ามซ่อนไว้ใน prose หรือ TODO ในโค้ดอย่างเดียว
6. เมื่อ section ผ่าน exit criteria ให้ย้าย active section ไป section ถัดไป และบันทึกเหตุผลใน `Change log`

## Product invariants

ข้อเหล่านี้ต้องเป็นจริงตลอดทุก section:

1. **Context integrity:** ข้อความที่ buffer/correct/undo ต้องเป็นของ focused control และ keyboard layout เดียวกันเสมอ
2. **No destructive correction:** เมื่อ confidence ไม่ถึงเกณฑ์, focus เปลี่ยน, copy/paste ล้มเหลว หรือ injection ไม่ครบ ระบบต้องไม่รายงาน success และต้องลดความเสียหายให้มากที่สุด
3. **Secret safety:** ไม่มี typed content ถูก persist/telemetry โดยไม่ตั้งใจ; sensitive context เป็น hard deny เสมอ
4. **Deterministic conversion:** layout mapping ต้อง round-trip ได้ตาม table และรักษาลำดับ Unicode/Thai combining marks
5. **Honest UX:** README, settings, toast และ tray ต้องบอก behavior ที่ binary ทำจริง
6. **Evidence before completion:** automated tests ไม่แทน Windows E2E; manual evidence ไม่แทน regression tests

## Known implementation snapshot

ข้อมูลต่อไปนี้มาจากการตรวจโค้ดปัจจุบัน ไม่ใช่คำสัญญาของ release:

- Mode เป็น enum (`Manual`/`Auto`/`Suggest`) และ config เก่าถูก migrate
- Auto ทั้งสองทิศทางตรวจที่ whitespace boundary เท่านั้น; Suggest ไม่แก้จนกด accept
- word/Auto และ selection conversion มี undo อายุ 30 วินาที; selection undo ใช้ app-native `Ctrl+Z` เฉพาะ context เดิม
- Selection conversion ผูกกับ foreground/focus generation และตรวจ clipboard sequence แล้ว
- Selection conversion ยอมทำงานเมื่อ clipboard เดิมเป็น empty/plain Unicode เท่านั้น; rich/image/file clipboard จะถูกปฏิเสธเพื่อไม่ทำข้อมูลหาย
- Buffer รองรับ token สูงสุด 64 characters และมี regression cases สำหรับประโยคไทยผิด layout ที่มี digits/punctuation
- v1 รองรับ exact Thai Kedmanee ↔ US QWERTY; layout variants, configurable hotkeys และ per-app profiles ถูก defer ไป v1.x ตาม D-002/D-005

---

## Decision gates

### [x] D-001 — Secret-shaped wrong-layout Thai (`ACCEPTED`)

**Conflict:** strict spec ระบุให้ bail ก่อนวิเคราะห์ แต่ประโยคไทยที่พิมพ์บน English layout มักมี digits/punctuation และถูก secret classifier จัดเป็น password-like

**Accepted contract:**

- sensitive app/password field: hard deny โดยไม่มีข้อยกเว้น
- normal context: `Hex`, `Base58Wif`, `Bech32`, `ExtendedKey` เป็น hard deny; BIP39 ใช้ stream guard กับ raw/candidate และ deny ตั้งแต่ threshold เป็นต้นไป แต่ไม่อ้างว่าย้อนกลับไปป้องกันคำก่อน threshold
- `HighEntropy`/`TooLong` อาจเป็น wrong-layout Thai; อนุญาตเฉพาะเมื่อ conversion ทั้งก้อนแบ่งเป็นคำไทยได้ครบ, ไม่มี unknown segment และ confidence policy ผ่าน
- raw token ที่ได้ข้อยกเว้นนี้ห้ามเข้า learning/persistence path

**Decision owner:** product owner delegated assessment to the implementation team on 2026-08-04

**Downstream:** S1 detection, S4 threat model, README privacy claims

### [x] D-002 — v1 scope versus full RightLang parity (`ACCEPTED`)

**Accepted contract:** ship v1 ด้วย Thai Kedmanee ↔ US English QWERTY ที่ตรวจครบก่อน; ย้าย Pattachote/Dvorak/UK-AU-CA ไป `v1.x parity track` โดย engine API ต้องไม่ปิดทางเพิ่ม layout และ Auto ต้องปิดตัวเองเมื่อ HKL ไม่ตรงคู่ที่รองรับ

**Decision owner:** product owner delegated assessment to the implementation team on 2026-08-04

**Downstream:** S3, test matrix, release date และ documentation wording

### [x] D-003 — Rich clipboard behavior (`ACCEPTED FOR v1`)

**Accepted contract:** v1 เป็น plain-Unicode-clipboard only และต้อง fail closed เมื่อ clipboard เดิมมี rich/image/file formats; full-format preservation เป็น `v1.x` work item ก่อนใช้คำว่า “arbitrary clipboard”

**Decision owner:** product owner delegated assessment to the implementation team on 2026-08-04

**Downstream:** S2 architecture และ Windows E2E matrix

### [x] D-004 — Meaning of “Auto as you type” (`ACCEPTED FOR v1`)

**Conflict:** live English→Thai ให้ความรู้สึกลื่น แต่ prefix สั้นชนคำจริงได้ง่าย; boundary-only ปลอด false positive แต่ไม่ตรง README ปัจจุบัน

**Accepted contract:** v1 Auto commit ที่ whitespace boundary เท่านั้นทั้งสองทิศทาง; no-space Thai ใช้ Manual หรือ non-destructive Suggest จนกว่าจะมี candidate/ambiguous/committed state machine และ corpus false-positive budget ที่พิสูจน์ live mode ได้

**Decision owner:** product owner delegated assessment to the implementation team on 2026-08-04

**Downstream:** S1 algorithm, latency, undo และ UX copy

### [x] D-005 — Configuration breadth (`ACCEPTED FOR v1`)

**Accepted contract:** v1 ใช้ fixed documented hotkeys และ fixed security blacklist เพื่อให้ behavior และ collision surface ตรวจครบได้ก่อน; configurable hotkeys, non-CapsLock alternatives และ per-app mode profiles ย้ายไป `v1.x configuration track` โดย security blacklist ยังคงเป็น hard deny และห้าม profile override

**Decision owner:** product owner delegated assessment to the implementation team on 2026-08-04

**Downstream:** S3 settings, E2E matrix และ README wording

---

## S0 — Lock the product contract

**Status:** `[x] VERIFIED`
**Goal:** ทำให้ spec, README และ implementation ใช้คำจำกัดความเดียวกันก่อนเพิ่ม behavior

### Work items

- [x] ปิด D-001 ถึง D-005 กับ decision owner
- [x] แยก `v1 acceptance criteria` ออกจาก long-term vision ใน PLAN.md
- [x] แก้ README claims เรื่อง live conversion, selection length/clipboard และ privacyให้ตรง contract
- [x] นิยามคำว่า `Auto`, `Manual`, `Suggest`, `Undo` และ `Convert-on-demand` แบบ testable
- [x] สร้าง requirement IDs (`RT-AUTO-*`, `RT-MAN-*`, `RT-SEC-*`, `RT-REL-*`) เพื่อให้ test/evidence อ้างกลับได้

### Exit criteria

- ไม่มีข้อความขัดกันระหว่าง README, PLAN และ decision records
- ทุก v1 requirement มี owner section และวิธีพิสูจน์
- D-001 ถึง D-005 เป็น `ACCEPTED` หรือ `DEFERRED` พร้อมเหตุผล

### Evidence required

- Documentation diff review
- Requirement-to-section traceability check

### v1 requirement contract

| ID | Requirement | Owner | Verification |
| --- | --- | --- | --- |
| RT-AUTO-001 | Auto ตรวจและ commit เฉพาะเมื่อ whitespace boundary ปิด token | S1 | production-policy sequence tests + Windows E2E |
| RT-AUTO-002 | Auto ทำงานเฉพาะ exact Kedmanee/US-QWERTY pair; unsupported HKL เป็น no-op | S1 | HKL resolver tests + Windows layout E2E |
| RT-AUTO-003 | Auto failure ห้ามบันทึก stats/undo/layout switch เป็น success | S1 | injected-failure seam tests |
| RT-MAN-001 | Shift+Backspace แปลง current/last completed word และมี one-shot Undo | S2 | production state tests + Windows E2E |
| RT-MAN-002 | Shift+CapsLock แปลง selection เฉพาะ target context เดิม | S2 | focus-race and stale-command tests |
| RT-MAN-003 | v1 selection conversion preserve empty/plain Unicode clipboard; rich clipboard fail closed | S2 | clipboard-format matrix |
| RT-MODE-001 | Manual ไม่แก้อัตโนมัติ; Auto ใช้ boundary; Suggest ไม่แก้ก่อน accept | S3 | mode sequence tests + config migration |
| RT-SEC-001 | Password fields และ fixed sensitive apps เป็น hard deny | S4 | native/UIA/blacklist E2E |
| RT-SEC-002 | Identifiable keys/addresses ไม่ถูก correct/learn/persist; BIP39 raw/candidate stream ถูก deny ตั้งแต่ threshold และไม่เข้า learning | S1/S4 | adversarial secret corpus + documented non-retrospective limit |
| RT-SEC-003 | HighEntropy/TooLong exception ใช้ได้เฉพาะ full-known Thai ใน normal contextและไม่เข้า learning | S1/S4 | allow/deny policy tests |
| RT-REL-001 | Hook path ไม่มี wait/disk I/O; synchronous correction work ต้อง bounded และวัด latency | S1/S5 | instrumentation/benchmark |
| RT-REL-002 | Resume/unlock recovery และ manual Unicode injection ผ่าน Windows transition/stress matrix | S4/S5 | Windows E2E |
| RT-REL-003 | Release ต้องมาจาก clean checkout พร้อม test/clippy/fmt/build/audit evidence | S5/S6 | release checklist |

---

## S1 — Core detection and Auto mode

**Status:** `[~] IN_PROGRESS — production policy complete; failure seams/performance evidence open`
**Depends on:** D-001, D-004

### Invariant

Auto conversion จะ commit เฉพาะ candidate ที่ผ่าน language, context, secret และ confidence policy เดียวกันทุก call site

### Work items

- [x] รวม production detection policy ให้มี entry point เดียว; sequence tests เรียก production policy โดยตรง
- [x] ระบุ evidence ที่ production ใช้จริงเป็น exact dictionary หรือ full segmentation; punctuation/mixed-script ถูก gate ก่อน commit
- [x] ใช้ boundary-only policy ตาม D-004 ทั้งสองทิศทาง; ไม่มี destructive live-prefix conversion
- [x] รักษา DP segmentation และเพิ่ม ambiguous/backtracking regression corpus
- [x] รองรับ digits, shifted symbols, punctuation wrappers และ long Thai runs ตาม contract
- [x] เพิ่ม false-positive corpus ครอบคลุม English prefixes/prose, URLs/emails, code, commands, paths, secrets และ mixed scripts
- [x] เพิ่ม true-positive corpus จากการใช้งานจริงทั้งสองทิศทาง
- [x] ตรวจ buffer cap/zeroization และแยก persistence ออกจาก hook path; VirtualLock behavior ยังอยู่ใน S4 Windows evidence
- [x] เพิ่ม production injected-failure seam: regression test พิสูจน์ว่า failed injection ไม่เพิ่ม stats/Undo/pending layout; synchronous Win32 injection latency ยังอยู่ใน S5 E2E

### Exit criteria

- ทุก `RT-AUTO-*` requirement มี production-path regression test
- corpus true-positive ผ่านตาม target และ false-positive ไม่เกิน budget ที่บันทึกไว้
- callback ไม่มี blocking wait และ performance อยู่ใน budget ของ S5
- failure/injection-partial path ไม่เพิ่ม stats, undo หรือ layout switch เป็น success

### Adversarial checks

- prefix ที่เป็นคำสั้นแต่เป็นต้นคำอังกฤษยาวกว่า
- dictionary ambiguity ที่ greedy split เลือกทางตัน
- 64/65-character boundary
- focus/layout เปลี่ยนระหว่าง candidate กับ commit
- secret-shaped ASCII ที่บังเอิญแปลงเป็นคำไทยบางส่วน

---

## S2 — Manual conversion, clipboard, and Undo

**Status:** `[~] IN_PROGRESS — implementation complete at code level; Windows seam/E2E evidence open`
**Depends on:** D-003

### Invariant

Manual action ต้องแก้เฉพาะ target ที่ผู้ใช้สั่ง, restore clipboard ก่อน Unicode injection ตาม contract และมี undo semantics ที่อธิบายได้

### Work items

- [x] Implement transactional stages: capture target → snapshot → copy/read → convert → validate target → restore → Unicode injection
- [x] Implement plain-text clipboard contract ตาม D-003 และ reject rich/image/file formats
- [x] Implement context-bound selection undo ด้วย app-native `Ctrl+Z` ภายใน 30 วินาที
- [~] ใช้ bounded command queue, อายุคำสั่ง 1 วินาที และ focus generation; focus ABA ภายใน control เดิมยังต้องมี Windows test
- [x] ตรวจผล modifier release และ Unicode injection; delayed clipboard rendering มี bounded retry และผ่าน Notepad E2E
- [x] ส่ง failure toast กลับ UI thread โดยไม่ใส่ typed content
- [~] Notepad พบและแก้ delayed `CF_UNICODETEXT` rendering; locked clipboard, focus race และ partial `SendInput` seams ยังเปิด

### Exit criteria

- Shift+Backspace และ Shift+CapsLock ผ่าน Notepad, Word และ Chrome
- ไม่มี paste ข้าม control/window ใน focus-race suite
- clipboard restore ผ่าน empty, Unicode text และ formats ตาม D-003
- Undo behavior ตรงเอกสารสำหรับ auto, word และ selection
- stress test ไม่พบ repeated/reordered characters

---

## S3 — Modes, settings, and parity

**Status:** `[~] IN_PROGRESS — v1 implementation complete; E2E evidence open`
**Depends on:** D-002 และ S1/S2 contracts

### Work items

- [x] Implement `Suggest` พร้อม context-bound accept และ reject/clear โดยไม่แก้ข้อความก่อนผู้ใช้ยืนยัน
- [x] เปลี่ยน mode model จาก boolean เป็น enum และมี regression tests สำหรับ config migration
- [-] Configurable hotkeys และ non-CapsLock alternatives — DEFERRED ไป v1.x ตาม D-005
- [-] Per-app mode profiles — DEFERRED ไป v1.x ตาม D-005; fixed security blacklist ยัง hard deny
- [x] Learning ใช้ production gate เดียว: exact US-QWERTY + ไม่มี correction/seed run, มี adversarial shape tests และ bounded persistence ทำงานนอก hook
- [x] v1 exact Kedmanee/US-QWERTY resolver ตาม D-002; layout packs อื่น DEFERRED ไป v1.x
- [x] tray/settings/status copy แสดง Manual/Auto/Suggest และ accept hotkey

### Exit criteria

- Config migration ไม่ทำให้ mode/hotkeys เดิมหาย
- Auto/Manual/Suggest และ fixed v1 hotkeys มี E2E evidence
- Fixed security blacklist มี allow/deny/default tests; per-app profiles เป็น v1.x
- Layout ทุกตัวใน release มี round-trip, shifted-symbol และ detection corpus

---

## S4 — Security, privacy, and resilience

**Status:** `[~] IN_PROGRESS — fail-closed guards and memory cleanup implemented; threat model/E2E open`
**Depends on:** D-001 และ behavior ที่นิ่งจาก S1–S3

### Work items

- [x] สร้าง threat-model table: data, lifetime, storage, trust boundary และ cleanup ใน [THREAT_MODEL.md](THREAT_MODEL.md)
- [~] ตรวจ typed-text copy: buffer, detection candidate, undo, clipboard, learning และ toast ครบ source path; allocator/Windows runtime evidence ยังเปิด
- [x] เพิ่ม timeout/zeroization ให้ undo, suggestion และ queued manual state ตาม contract
- [~] Tri-state UIA failure และ fixed/custom blacklist precedence ผ่าน pure tests; native/browser/Electron password-field E2E ยังเปิด
- [x] BIP39 raw/candidate stream, WIF, xpub, bech32, hex, high-entropy และ long-token corpus ผ่านตาม D-001/TM-001
- [ ] ทดสอบ hook reinstall หลัง sleep/resume, lock/unlock, UAC และ session change
- [~] `VirtualLock`/`VirtualUnlock` และ WER `NOHEAP` flag ผ่าน runtime tests; non-elevated process-token evidence ยังเปิด
- [x] ตรวจ dependency/network surface และ learning persistence: normal dependency tree ไม่มี network crate และ RustSec audit ผ่าน 70 dependencies

### Exit criteria

- Security matrix มี allowed และ denied paths ครบ
- ไม่มี typed content ใน logs/config/crash artifact ที่ตรวจได้
- Hook recovery ผ่าน Windows transition matrix โดยไม่ restart process
- Privacy claims ใน README มี evidence หรือถูกลดระดับคำกล่าว

---

## S5 — Verification and performance

**Status:** `[~] IN_PROGRESS — automated/policy latency gates pass; platform matrix and audit open`

### Automated gates

- [x] `cargo test --features winos --all-targets` — PASS 63 tests บน working tree
- [x] `cargo clippy --features winos --all-targets -- -D warnings`
- [x] `cargo build --release --features winos`
- [x] `cargo fmt --all -- --check`
- [x] `cargo audit` — PASS: RustSec 1,186 advisories, scanned 70 locked dependencies, no vulnerability reported
- [~] benchmark production boundary policy ด้วย dependency-free release gate; full hook callback/SendInput latency ยังต้องวัดใน Windows E2E

### Windows E2E matrix

- [~] Notepad — selection conversion `l;ylfu` → `สวัสดี`, clipboard restore และ context-bound Undo PASS; Auto/Suggest ผ่านเมื่อใช้ E2E harness (`e2e/`) แต่ Win11 Notepad (WinUI) drop Thai `KEYEVENTF_UNICODE` burst ~40–60% ในช่องทาง synthetic จึงลดสถานะเป็น manual-only target
- [~] Microsoft Word — BLOCKED ในรอบนี้เพราะ Computer Use ตรวจพบ user input ซ้ำและหยุดเพื่อไม่แย่ง focus; ยังไม่มีผลทดสอบ
- [x] Edge (Chromium, engine เดียวกับ Chrome) — Auto two-direction PASS 16/16: EN→TH `l;ylfu` → `สวัสดี` ×10, TH→EN `แนพพำแะ` → `correct` ×6 ผ่าน `e2e/notepad_roundtrip.py --app edge`; Chrome-specific row เหลือยืนยันบน Chrome จริงเท่านั้น
- [ ] Native password field
- [ ] Browser/Electron password field
- [ ] Blacklisted wallet/password-manager/terminal
- [ ] Fast typing (~150 WPM), held modifiers และ hotkey repeat
- [ ] Thai tone marks/combining characters
- [ ] Sleep/resume, lock/unlock และ UAC transition

### Exit criteria

- Automated gates ผ่านจาก clean checkout
- ทุก E2E row มีวันที่, OS/app version, result และ reproduction notes
- ไม่มี unresolved P0/P1; P2 ที่ defer ต้องมี owner/release target
- performance อยู่ใน budget ที่ระบุใน PLAN.md หรือมี accepted decision เปลี่ยน budget

---

## S6 — Release readiness

**Status:** `[ ] TODO`
**Depends on:** S0–S5 verified

### Work items

- [~] Release procedure/checklist สร้างแล้วใน [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md); freeze requirement/evidence matrix หลัง platform gates เท่านั้น
- [ ] สร้าง clean reproducible release artifact
- [ ] ตรวจ portable startup/config migration/uninstall behavior
- [~] บันทึก pre-sign SHA-256 ของ local test artifact และ signing/verification procedure แล้ว; signed clean artifact ยัง BLOCKED เพราะไม่มี certificate/release commit
- [ ] ทำ pilot checklist โดยไม่เก็บ typed content
- [ ] ตรวจ README, settings screenshots/help และ known limitations รอบสุดท้าย

### Exit criteria

- Release artifact สร้างจาก clean tagged commit และผ่าน S5 gates
- Documentation, UI และ binary มี behavior ตรงกัน
- Known limitations ระบุชัดและไม่มีข้อความ “complete/shippable” ที่ไม่มี evidence

---

## Evidence ledger

| ID | Date | Section | Evidence | Result | Does not prove |
| --- | --- | --- | --- | --- | --- |
| E-001 | 2026-08-04 | Baseline | `cargo test --features winos --all-targets` | PASS: 39 library + 16 integration tests | ไม่พิสูจน์ global hook, clipboard หรือ app behavior จริง |
| E-002 | 2026-08-04 | Baseline | `cargo clippy --features winos --all-targets -- -D warnings` | PASS | ไม่พิสูจน์ runtime correctness |
| E-003 | 2026-08-04 | Baseline | `cargo build --release --features winos` | PASS | ไม่พิสูจน์ clean/reproducible release หรือ Windows E2E |
| E-004 | 2026-08-04 | S0 | Code/spec inspection | พบ mismatch เรื่อง modes, live EN→Thai, selection undo, rich clipboard และ layout parity | ยังไม่ได้ตัดสิน product contract |
| E-005 | 2026-08-04 | S0–S4 | Independent subagent audits: Auto/parity และ implementation/security | ยืนยัน prefix false-positive, exact-layout scope, hook I/O, toast/manual undo และ fail-open guard risks; ใช้ปิด D-002/D-004/D-005 และจัดลำดับแก้ | เป็น static review; ไม่พิสูจน์ Windows runtime |
| E-006 | 2026-08-04 | S1/S4 | Red-test probe: hard-deny converted BIP39 candidates | พบว่า English คำจริง เช่น `correct` อยู่ใน BIP39 จึงห้าม hard-deny token เดี่ยว; revert policy และเก็บ BIP39 เป็น stream/context risk | ยังไม่แก้ raw wrong-layout seed run ทั้ง phrase |
| E-007 | 2026-08-04 | S1–S5 | test + clippy + fmt + release build หลัง policy/mode/manual/security changes | PASS: 44 library + 3 binary + 16 integration tests; clippy/fmt/build PASS | ไม่พิสูจน์ global hook, UIA, clipboard, sleep/UAC หรือ clean checkout |
| E-008 | 2026-08-04 | S5 | `cargo audit --version` | BLOCKED: ไม่มี subcommand `audit` ใน toolchain ปัจจุบัน | ไม่ได้ตรวจ advisory database |
| E-009 | 2026-08-04 | S4 | [THREAT_MODEL.md](THREAT_MODEL.md) source/data-flow audit | บันทึก trust boundaries, lifetime, persistence, cleanup และ TM-001–TM-005 | ไม่พิสูจน์ controls ใน Windows runtime |
| E-010 | 2026-08-04 | S1/S5 | `cargo run --release --example policy_latency` | PASS: 50,000 production-policy ops, average ~604 ns, worst batch average ~708 ns; budget <1 ms | ไม่รวม Win32 context queries หรือ `SendInput` latency |
| E-011 | 2026-08-04 | S4/S5 | `cargo tree --features winos --edges normal` | PASS source inspection: ไม่มี HTTP/telemetry/updater crate ใน normal dependency tree | ไม่แทน advisory audit หรือ transitive source audit |
| E-012 | 2026-08-04 | S2/S5 | Computer-driven debug E2E, isolated config, Windows Notepad | PASS: manual selection, direct Unicode replacement, plain clipboard restore และ app-native Undo; พบ/แก้ delayed Unicode rendering bug | ไม่พิสูจน์ Word/Chrome, rich clipboard preservation, focus race หรือ release binary |
| E-013 | 2026-08-04 | S1/S5 | Windows HKL trace + resolver regression | พบ production bug: default HKL เป็น `0x041E041E`; แก้ resolver ให้รับ canonical/default handles และ reject variants; 65 tests PASS | Auto/Suggest E2E ยัง BLOCKED เพราะไม่มี supported US-QWERTY handle ใน environment |
| E-014 | 2026-08-04 | S3/S5 | Final consolidated `test`/`clippy`/`fmt`/release build | PASS: 45 library + 5 Windows-binary + 16 integration = 66 tests; clippy/fmt/release PASS without warnings | ไม่พิสูจน์ platform matrix หรือ clean checkout |
| E-015 | 2026-08-04 | S4/S5 | `cargo audit` 0.22.2 | PASS: loaded 1,186 RustSec advisories and scanned 70 `Cargo.lock` dependencies; no vulnerability reported | ไม่แทน source provenance/cargo-vet หรือ future advisory updates |
| E-016 | 2026-08-04 | S1/S3/S5 | Expanded corpus + learning-policy final gates | PASS: 45 library + 6 Windows-binary + 18 integration = 69 tests; code/command/path false positives, natural no-space Thai และ `สวัสดีดี` stress case covered; release latency average ~664 ns/worst batch ~1.351 µs | ไม่พิสูจน์ Windows Auto/Suggest หรือ actual learned-file persistence |
| E-017 | 2026-08-04 | S1/S5 | Production Auto failure seam | PASS: failed injected backend leaves Auto stats, Undo และ pending layout unchanged; final suite 45 + 7 + 18 = 70 tests, clippy/fmt/release PASS | ไม่จำลอง partial Win32 batch landing in the target app |
| E-018 | 2026-08-04 | S3/S5 | Production learning gate | PASS: exact-US/no-candidate policy and secret/non-ASCII/non-word shape tests; suite 46 + 7 + 18 = 71 tests, clippy/fmt/release PASS | ไม่พิสูจน์ disk-full/file-permission behavior |
| E-019 | 2026-08-04 | S4/S5 | Fail-closed guard and blacklist tests | PASS: UNKNOWN/PASSWORD protected, SAFE allowed; terminal/password-manager/wallet defaults and additive custom blacklist covered; suite 46 + 10 + 18 = 74 tests, clippy/fmt/release PASS | ไม่พิสูจน์ native/browser/Electron focus behaviorจริง |
| E-020 | 2026-08-04 | S4/S5 | Windows memory-hardening runtime test | PASS: `VirtualLock` and matching `VirtualUnlock` succeed on a stable heap allocation; suite 46 + 11 + 18 = 75 tests, all consolidated gates PASS | ไม่พิสูจน์ hook buffer call result, WER flags หรือ process elevation token |
| E-021 | 2026-08-04 | S4/S5 | Windows Error Reporting hardening test | PASS: `harden_process` makes `WerGetFlags(GetCurrentProcess)` include `WER_FAULT_REPORTING_FLAG_NOHEAP`; suite 46 + 12 + 18 = 76 tests, clippy/fmt/release PASS | ไม่พิสูจน์ elevation token หรือ third-party crash-dump capture |
| E-022 | 2026-08-04 | S4/S6 | Runtime/build posture inspection | Launcher token is Medium Integrity (`S-1-16-8192`); local release artifact is 2,719,744 bytes with pre-sign SHA-256 `4BC8…CB31`; release checklist created | Artifact is dirty-tree, unsigned, and not platform-certified |
| E-023 | 2026-08-24 | S5 | Python/uv E2E harness (`e2e/`, pywinauto+UIA, debug build + `RIGHTTYPE_E2E_ACCEPT_INJECTED`): Auto matrix บน Edge | PASS 16/16 (EN→TH ×10, TH→EN ×6); Manual Shift+Backspace PASS บน Notepad; พบว่า pywinauto ส่ง chars เป็น VK_PACKET และ boundary ต้องเป็น real `VK_SPACE` | ไม่พิสูจน์ Word จริง, release binary (hook ignore injected by design), หรือ human-speed input |
| E-024 | 2026-08-24 | S5 | Notepad WinUI synthetic reliability probe: original atomic inject vs chunked/paced variants | EN→TH drop Thai unicode units ~40–60% ทุก variant; TH→EN ASCII 16/16 ไม่เคย drop; ตัดสินว่าเป็นข้อจำกัดของ target (WinUI) ไม่ใช่ product — inject.rs revert กลับ single-batch เดิม | ไม่แทนการทดสอบ Word/human typing; flake อาจต่างบน native RichEdit |

## Risk register

| ID | Risk | Severity | Mitigation | Status |
| --- | --- | --- | --- | --- |
| R-001 | Live EN→Thai แก้ prefix อังกฤษผิด | High | D-004 boundary-only + production-policy corpus | MITIGATED IN CODE, E2E OPEN |
| R-002 | Secret exception ทำให้ privacy claim เกินจริง | Critical | D-001 + hard-deny contexts/patterns + strict full-segmentation exception | PARTIAL: raw wrong-layout BIP39 stream OPEN |
| R-003 | Manual selection ทำ rich/app-specific clipboard สูญหายหรือใช้งานไม่ได้ | High | D-003 fail-closed + restore-before-inject; full preservation v1.x | MITIGATED FOR PLAIN CLIPBOARD; metadata limitation DOCUMENTED |
| R-004 | Async manual action inject ผิด control | Critical | target identity/focus generation + bounded command + race tests | SAME-CONTEXT NOTEPAD PASS; FOCUS-RACE OPEN |
| R-005 | README/PLAN อ้าง feature ที่ยังไม่มี | Medium | S0 contract normalization + D-005 | MITIGATED; final UI/docs review OPEN |
| R-006 | Passing pure tests hides Windows integration failures | High | S5 Windows E2E matrix | NOTEPAD MANUAL COVERED; REMAINING MATRIX OPEN |

## Change log

- `2026-08-04` — สร้าง dynamic execution plan; ตั้ง S0 เป็น active; บันทึก decision gates และ implementation/spec mismatches จากโค้ดปัจจุบัน
- `2026-08-04` — ปิด D-001–D-005; รวม exact-layout boundary policy; เพิ่ม Suggest/config migration, selection undo, bounded persistence, fail-closed focus/process guards และ UI-thread toast; automated gates ผ่าน 63 tests แต่ Windows E2E/dependency audit ยังเปิด
- `2026-08-04` — Windows Notepad E2E พบและแก้ delayed clipboard-rendering กับ default-HKL (`0xLLLLLLLL`) resolver bugs; manual selection/restore/Undo ผ่าน; Auto/Suggest, Word/browser, transitions และ advisory audit ยังเป็น explicit gates
- `2026-08-23` — commit working tree ทั้งหมด (threat model, release checklist, boundary policy, data_dir, latency example); baseline gates ยืนยันบน `HEAD` แล้ว; external unblock checklist ยังเปิดเหมือนเดิม
- `2026-08-24` — US-QWERTY/Thai HKL unblock ปิด (`0x04090409` + `0x041E041E`); สร้าง Python/uv E2E harness; Auto two-direction ผ่าน Edge 16/16 (E-023); พิสูจน์ว่า Notepad-WinUI เป็น target ที่ไม่ reliable กับ Thai unicode burst (E-024) และ revert inject.rs; เพิ่ม debug-only E2E trace ใน hook.rs

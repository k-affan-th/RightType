# Keyboard Detection and More English Keyboards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** RightType detects the Thai and English keyboard in use on its own, supports Dvorak, Colemak and other-country English keyboards, and has a registry a third language can be added to as data.

**Architecture:** A new OS-free `layout::registry` turns a Windows keyboard handle (HKL) plus facts read from the registry into `(Language, LayoutId)`. `policy::supported_layout_id` and friends delegate to it, so the 193 `InputLayout` uses stay untouched. The hook notes the detected Thai and English tables on every key event (as it already does for English), and the Settings Thai picker is replaced by a read-only "Detected" line.

**Tech Stack:** Rust (lib + `winos` feature Windows binary), `windows` crate 0.58, cargo test / clippy / fmt as in `.github/workflows/rust.yml`.

**Spec:** `docs/superpowers/specs/2026-10-29-keyboard-detection-design.md`

## Global Constraints

- Thai tables stay Kedmanee, Pattachote, Manoonchai only; no new Thai keyboards.
- Unknown or unsupported keyboards resolve to `None` and RightType stays off for them; it never guesses a table.
- `InputLayout` (UsQwerty, ThaiKedmanee) is not changed in this plan; no multi-target decision logic, no third language.
- A `thai_layout` value in an existing config file is read and ignored, never an error; it is no longer written.
- Core modules (`src/layout/*`, `src/policy.rs`) stay OS-free; only `src/hook.rs` reads the Windows registry.
- Every new user-visible string has Thai and English text in `src/i18n.rs`.
- Must pass: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo clippy --features winos --all-targets -- -D warnings`, `cargo test --all-targets`.
- Do not claim support for Dvorak/Colemak in docs beyond what Task 5's audit confirmed.
- No version bump; changes go under `## [Unreleased]` in `CHANGELOG.md` (releasing is the maintainer's step, and `tests/release_metadata.rs` ties versions together).
- Spec deviation, decided here: "other-country English" is decided by the Windows layout file (US or UK key positions), not by a hand-kept country list, so only keyboards that really share those positions are supported. The spec text already says so.

## Review Focus

- Thai keyboard installed from a file with an unreadable layout file → RightType stays off for it, no wrong conversion.
- User switches Thai keyboard (Kedmanee ↔ Pattachote) mid-session → the next word uses the new table without restart.
- Old config containing `thai_layout = pattachote` → loads without error and does not force that table.
- Dvorak left-/right-handed and Colemak-DH are different key positions → must resolve to `None`, not to plain Dvorak/Colemak.
- Global `ThaiVariant`/`EnglishVariant` state shared by parallel tests → tests that set it must not race.

---

### Task 1: Dvorak and Colemak tables

**Files:**
- Create: `src/layout/dvorak.rs`, `src/layout/colemak.rs`
- Modify: `src/layout/mod.rs` (modules, exports, `LayoutId`, `EnglishVariant`, `english_layout`, `LayoutId::is_english`)

**Interfaces:**
- Produces: `layout::DvorakEn`, `layout::ColemakEn` (both `impl Layout`, unit structs), `LayoutId::{DvorakEn, ColemakEn}`, `EnglishVariant::{Dvorak, Colemak}`; `english_variant()`/`set_english_variant` round-trip all four variants (storage stays `AtomicU8`; replace the `== Uk` test with a `match`).

- [ ] **Step 1: Write failing tests** in each new file's `mod tests`:
  - `dvorak_known_keys`: `DvorakEn.char_of('s') == Some('o')`, `char_of(';') == Some('z')`, `char_of('q') == Some('\'')`, `char_of('Q') == Some('"')`, `char_of('-') == Some('[')`, `char_of('_') == Some('{')`, and `key_of('o') == Some('s')`.
  - `colemak_known_keys`: `ColemakEn.char_of('e') == Some('f')`, `char_of('p') == Some(';')`, `char_of(';') == Some('o')`, `char_of('n') == Some('k')`, `char_of('q') == Some('q')`, `char_of('E') == Some('F')`, `key_of('f') == Some('e')`.
  - In both: `round_trips_every_printable_key` — for each `c` in `'!'..='~'`, `key_of(char_of(c).unwrap()) == Some(c)`, and no two keys share an output (collect outputs into a set, length 94).
- [ ] **Step 2: Run** `cargo test --lib layout::` — Expected: compile error (types missing).
- [ ] **Step 3: Implement** `DvorakEn` and `ColemakEn` in their files. Data decisions: keys are US characters. Dvorak maps the unshifted sequence `-=qwertyuiop[]asdfghjkl;'zxcvbnm,./` to `[]',.pyfgcrl/=aoeuidhtns-;qjkxbmwvz`. Colemak differs from QWERTY only at e→f r→p t→g y→j u→l i→u o→y p→; s→r d→s f→t g→d j→n k→e l→i ;→o n→k. Shifted characters derive from the unshifted ones with the US shift pairs (letters uppercase, `-_ =+ [{ ]} ;: '" ,< .> /?`); digits and the number row symbols are identity.
- [ ] **Step 4: Extend `mod.rs`**: add `mod dvorak; mod colemak;`, `pub use`, new `LayoutId`/`EnglishVariant` variants, `english_layout()` arms, `is_english` arms. Add a `mod.rs` test `english_variant_round_trips_all` that sets each variant and reads it back.
- [ ] **Step 5: Run** `cargo test --lib layout::` — Expected: PASS. Then `cargo clippy --all-targets -- -D warnings`.
- [ ] **Step 6: Commit** `git add src/layout && git commit -m "Add Dvorak and Colemak English layout tables"`

---

### Task 2: Keyboard registry (core) and policy delegation

**Files:**
- Create: `src/layout/registry.rs`
- Modify: `src/layout/mod.rs` (`pub mod registry;`, `LayoutId::thai_variant()`/`english_variant()` helpers), `src/policy.rs` (remove `THAI_KEYBOARDS`, `set_thai_keyboards`, `thai_table_of`, `thai_keyboard_matches`; rewrite `supported_layout_id`, `flip_layout_id`, `is_preferred_layout`, `english_variant_of`), `src/lib.rs` if exports are listed there

**Interfaces:**
- Consumes: Task 1 `LayoutId`/`EnglishVariant`.
- Produces (all in `layout::registry`):
  - `pub enum Language { Thai, English }`
  - `pub struct Detected { pub language: Language, pub table: LayoutId }` (`Clone, Copy, PartialEq, Eq, Debug`)
  - `pub struct KeyboardFacts { pub thai: Vec<(u16, LayoutId)>, pub english: Vec<(u16, LayoutId)> }` (`Default, Clone`) — keyed by the HKL high word
  - `pub fn classify_layout_file(language: Language, file: &str, text: &str) -> Option<LayoutId>`
  - `pub fn set_facts(facts: KeyboardFacts)` and `pub fn detect(hkl: u32) -> Option<Detected>`
  - `pub fn detect_with(hkl: u32, facts: &KeyboardFacts) -> Option<Detected>` (pure, used by tests)
  - `LayoutId::thai_variant(self) -> Option<ThaiVariant>`, `LayoutId::english_variant(self) -> Option<EnglishVariant>`
- Policy keeps its public names: `supported_layout_id(hkl) -> Option<InputLayout>`, `flip_layout_id`, `is_preferred_layout`, `english_variant_of(hkl) -> Option<EnglishVariant>`; add `pub fn thai_variant_of(hkl: u32) -> Option<ThaiVariant>`.

- [ ] **Step 1: Write failing tests** in `registry.rs`:
  - `classify_thai`: `KBDTH0.DLL`/`KBDTH2.DLL` → Kedmanee; `KBDTH1.DLL`/`KBDTH3.DLL` → Pattachote; text containing `Manoonchai` (any case) → Manoonchai; `KBDUS.DLL` with `Language::Thai` → `None`.
  - `classify_english`: `KBDUS.DLL` → QwertyEn; `KBDUK.DLL` → QwertyUk; `KBDDV.DLL` → DvorakEn; `KBDUSL.DLL`, `KBDUSR.DLL` → `None`; text `Colemak` → ColemakEn; text `Colemak-DH` → `None`; `KBDGR.DLL` → `None`.
  - `detect_defaults`: with empty facts, `0x0409_0409`, `0x0000_0409`, `0x0C09_0C09`, `0x1409_1409` → QwertyEn; `0x0809_0809` → QwertyUk; `0x041E_041E`, `0x0000_041E` → Kedmanee; `0x0407_0407` (German) → `None`; `0xF00F_041E` with empty facts → `None`.
  - `detect_facts`: facts `thai: [(0xF001, Pattachote)]`, `english: [(0xF010, DvorakEn)]` → `0xF001_041E` is Pattachote, `0xF010_0409` is DvorakEn.
- [ ] **Step 2: Run** `cargo test --lib layout::registry` — Expected: compile error.
- [ ] **Step 3: Implement the registry.** `detect_with` dispatches on `hkl & 0x3FF` (0x1E Thai, 0x09 English, else `None`). A device word of 0 or equal to the language id means the language's default keyboard (Kedmanee; for English the existing US/UK id list `0x0409 0x0C09 0x1409` → US, `0x0809` → UK, copied from the old `english_variant_of`); otherwise look the device word up in `facts`; absent → `None`. `set_facts`/`detect` use a `static RwLock<KeyboardFacts>`.
- [ ] **Step 4: Rewire `policy.rs`** to call `registry::detect`: `supported_layout_id` returns `UsQwerty` for any English detection and `ThaiKedmanee` for any Thai detection (any classifiable keyboard, no longer only the one equal to the setting); `english_variant_of`/`thai_variant_of` map `Detected.table`; `is_preferred_layout(hkl)` = the detected table equals the current `layout::thai_variant()`/`english_variant()`; `flip_layout_id` maps `Detected.table` to its `LayoutId`, keeping its existing language-only fallback. Delete the old statics and update/delete their tests (`thai_table_of` test at the end of `policy.rs` moves to registry's `detect_facts`).
- [ ] **Step 5: Run** `cargo test --all-targets` — Expected: PASS (fix call sites the compiler flags; `hook.rs` still calls `policy::set_thai_keyboards`, so under `--features winos` it breaks until Task 3; plain `cargo test` does not build it).
- [ ] **Step 6: Commit** `git commit -am "Add keyboard registry; policy resolves keyboards through it"` (include `git add src/layout/registry.rs`).

---

### Task 3: Windows layer feeds the registry and notes both variants

**Files:**
- Modify: `src/hook.rs` (`thai_keyboards()` → `installed_keyboards()`, `note_english_variant` → `note_keyboard`, startup scan near line 3633, call site near line 2606, `activate_layout` ordering near line 3807)

**Interfaces:**
- Consumes: Task 2 `registry::{KeyboardFacts, classify_layout_file, set_facts}`, `policy::{english_variant_of, thai_variant_of}`.
- Produces: `fn installed_keyboards() -> KeyboardFacts` (Windows only), `fn note_keyboard(hkl: HKL)` which sets `layout::set_thai_variant` / `set_english_variant` from the handle when it is a supported keyboard of that language, and does nothing otherwise.

- [ ] **Step 1: Generalize the registry reader.** `installed_keyboards()` enumerates `installed_keyboard_ids()` (all `Keyboard Layouts` keys, not only `…041E`), keeps ids whose low word language is Thai (0x1E) or English (0x09), reads `Layout File` and `Layout Text`, calls `classify_layout_file`, and records `(0xF000 | Layout Id, table)` and the id's high word, exactly as `thai_keyboards()` does now. Remove the hard-coded `["0001041E","0002041E","0003041E"]` seed only if enumeration already returns them; otherwise keep it.
- [ ] **Step 2: `note_keyboard`** replaces `note_english_variant`: English via `policy::english_variant_of`, Thai via `policy::thai_variant_of`. Call it where `note_english_variant(effective_layout())` is called (per key event) and in the startup scan; the startup scan feeds `registry::set_facts(installed_keyboards())` first, then notes the active handle, or else the first installed English and Thai keyboards (as the English scan does now).
- [ ] **Step 3: Verify build and tests.** Run `cargo clippy --features winos --all-targets -- -D warnings` and `cargo test --features winos --all-targets` — Expected: PASS.
- [ ] **Step 4: Manual check on a Windows machine** (record results in the PR description): with Thai Kedmanee and Pattachote both installed, type a wrong-layout word, switch Thai keyboard, type another; both convert with their own table. With an unrelated language active (e.g. German) RightType does nothing.
- [ ] **Step 5: Commit** `git commit -am "Detect Thai and English keyboard from the active handle"`

---

### Task 4: Settings, config and strings

**Files:**
- Modify: `src/settings.rs` (Thai picker at ~700-720, ids ~294, ~1715, state sync ~1872, click handler ~2089), `src/config.rs` (lines ~58, 205, 332, 378-410, 608), `src/i18n.rs` (`HeadThaiKeyboard`, `NoteKeyboards`, new strings), `src/keymap.rs` / `src/practice.rs` only if they fail to compile
- Test: unit tests in `src/config.rs`

**Interfaces:**
- Consumes: Task 3 (variants are set by the hook).
- Produces: `fn detected_keyboards_line() -> String` in `settings.rs` using new `T::KbDetected` ("Detected: {thai} · {english}" / "ตรวจพบ: {thai} · {english}") and per-table name strings (`T::KbNameKedmanee`, …`Pattachote`, `Manoonchai`, `Us`, `Uk`, `Dvorak`, `Colemak`).

- [ ] **Step 1: Write failing config tests**: `legacy_thai_layout_is_ignored` — parsing a config containing `thai_layout = "pattachote"` succeeds and leaves `layout::thai_variant()` unchanged (set it to Kedmanee first; run in a test guarded by a `static Mutex` shared with other tests that touch the variant); `thai_layout_not_written` — the serialized config never contains `thai_layout`.
- [ ] **Step 2: Run** `cargo test --lib config` — Expected: FAIL.
- [ ] **Step 3: Config:** keep the parse of `thai_layout` (so old files load) but drop `config.rs:332` application and write `None`.
- [ ] **Step 4: Settings:** replace the three Thai segments and their ids/click branch with one label showing `detected_keyboards_line()`, refreshed in the same place the state sync at ~1872 runs and whenever the page is shown. Update `HeadThaiKeyboard` to "Keyboards"/"แป้นพิมพ์" and `NoteKeyboards` to say keyboards are detected automatically and list what is unsupported (stays off).
- [ ] **Step 5: Run** `cargo test --all-targets` and `cargo clippy --features winos --all-targets -- -D warnings` — Expected: PASS.
- [ ] **Step 6: Manual check:** open Settings → Keyboard; the line updates after switching keyboard and refocusing; no Thai picker remains.
- [ ] **Step 7: Commit** `git commit -am "Settings: show detected keyboards instead of a Thai keyboard picker"`

---

### Task 5: Dvorak and Colemak audit and tests

**Files:**
- Create: `tests/dvorak_colemak.rs`, `docs/superpowers/specs/2026-10-29-dvorak-colemak-audit.md`
- Modify: whatever the audit finds wrong (expected: hard-coded US-key assumptions in `src/code.rs`, `src/abbrev.rs`, `src/snippets.rs`, `src/policy.rs` symbol handling)

**Interfaces:**
- Consumes: Tasks 1–2; `policy::detect_token(token: &str, layout: InputLayout, en: &Dictionary, th: &Dictionary) -> Option<Detection>`, `dict::english()`, `dict::thai()`, `layout::{set_english_variant, en_to_th, th_to_en}`.
- Produces: the audit note (each place that assumes "typed character = US key", with verdict fixed / restricted / fine) that Task 6 reads.

- [ ] **Step 1: Audit.** `grep -rn "us_table\|QwertyEn\|is_ascii_alphabetic\|en_to_th\|th_to_en" src` and read every hit; write one row per site in the audit note. Known facts: the hook reads characters with `ToUnicodeEx` on the active handle (`hook.rs` `translate`), and injection writes Unicode code points, so neither assumes QWERTY.
- [ ] **Step 2: Write failing tests** in `tests/dvorak_colemak.rs`, one `#[test]` per variant, each taking a file-level `static LOCK: Mutex<()>` before setting the global variant and restoring `Us` at the end: for the Thai word `สวัสดี`, `th_to_en` under the variant gives the characters a Dvorak/Colemak typist would produce; `policy::detect_token(typed, InputLayout::UsQwerty, …)` returns a detection whose replacement is `สวัสดี`; for the English word `hello` typed on the Thai layout the reverse holds; a correct English word typed on the variant yields `None`.
- [ ] **Step 3: Run** `cargo test --test dvorak_colemak` — Expected: FAIL where the audit found a gap, PASS elsewhere.
- [ ] **Step 4: Fix** each gap the audit marks "fixed". For a gap that cannot be fixed safely, record "restricted" in the audit note with the exact feature affected (the docs in Task 6 must say so).
- [ ] **Step 5: Run** `cargo test --all-targets` and `cargo run --release --example false_positive_audit -- --check` — Expected: PASS, no new false positives on US.
- [ ] **Step 6: Commit** `git add tests docs src && git commit -m "Audit and test Dvorak and Colemak paths"`

---

### Task 6: Documentation

**Files:**
- Modify: `README.md`, `docs/GUIDE.md` (the "Supported keyboards" paragraph ~line 112 and the developer section ~line 283), `CHANGELOG.md`
- Test: `tests/release_metadata.rs` must still pass

**Interfaces:**
- Consumes: Task 5 audit note (restrictions), Task 4 strings.

- [ ] **Step 1: Add a "Supported keyboards" table** to GUIDE.md and a short Thai version to README.md, grouped by language: keyboard, found automatically (yes), how it is recognised (Windows layout file), limits from the audit note; plus a row set for unsupported cases (left/right-handed Dvorak, Colemak-DH, non-US/UK key positions such as German or French) and the line "RightType stays off and says so".
- [ ] **Step 2: Developer section:** "Adding a language" — registry rule in `layout::registry`, tables in `src/layout/`, dictionary in `assets/`, tests; state that `InputLayout` is still two-valued.
- [ ] **Step 3: CHANGELOG** under `[Unreleased]`: Added (Dvorak, Colemak, other-country English by layout file), Changed (Thai keyboard detected automatically; the Settings picker is gone).
- [ ] **Step 4: Run** `cargo test --test release_metadata` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -am "Document supported keyboards and automatic detection"`

---

### Task 7: Final verification

- [ ] **Step 1:** `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo clippy --features winos --all-targets -- -D warnings`, `cargo test --all-targets` — Expected: all clean.
- [ ] **Step 2:** `cargo run --release --example policy_latency`, `… false_positive_audit -- --check`, `… typing_benchmark -- --check` (as in CI) — Expected: pass.
- [ ] **Step 3:** On Windows with Dvorak and Colemak installed, run the manual checks from Tasks 3–4 plus: type a wrong-layout word on each; a left-handed Dvorak keyboard leaves RightType off.
- [ ] **Step 4:** Report which manual checks were done and which were not.

# Dual-Symbols & Long Sentences Auto-Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix dual-language symbol ambiguity (e.g. `.` on `Shift + ง` converting to `"` instead of `ใ`, and 10 other symbols) and improve long Thai sentence auto-detection recall on QWERTY keyboards from 56.2% to >95% without false positives on English text and code.

**Architecture:** 
1. Make manual flip and conversion layout-aware: Pass `effective_layout()` into the conversion pipeline so that single symbols and ambiguous punctuation typed in Thai layout convert via `th_to_en` (e.g., `.` -> `"`), while symbols typed in English layout convert via `en_to_th` (e.g., `.` -> `ใ`).
2. Add tolerant multi-word segmentation in `src/detect.rs` for long runs (>= 6 chars) typed on English layout: When >= 70% of characters are recognized Thai words and at least 2 known Thai words are present (guarded against known English words and identifiers), classify as `Evidence::FullSegmentation` so that sentences with names, slang, numbers, or minor typos auto-convert reliably.

**Tech Stack:** Rust (standard library, `righttype` engine)

**Spec:** Documented in conversation benchmarks (`examples/bench_issues.rs`) and issue feedback from v2.6.2.

## Global Constraints

- Must pass `cargo run --release --example false_positive_audit -- --check` with 0 false positives and quality gate PASS.
- Must preserve all existing tests (`cargo test --lib`).
- No regressions on numbers, currency (`1,250.50`), abbreviations (`ก.ค.`), or code editors.
- Hold arrow/cursor keys out of scope for this cycle.

## Review Focus

1. Single `.` typed while Thai Kedmanee is active -> must convert to `"` on `Shift + Backspace`.
2. Single `.` typed while US QWERTY is active -> must convert to `ใ` on `Shift + Backspace`.
3. Punctuation in numbers (`1.5`, `2,500`) and abbreviations (`ก.ค.`) -> must not be broken or turned into quotes.
4. Long sentence with an unknown proper noun (e.g. `ผมชื่อแอฟฟาน`) typed on QWERTY -> must auto-convert to Thai.
5. Common English words (`development`, `commit`) and code snippets -> must NEVER trigger Thai conversion (0 false positives).

---

### Task 1: Layout-Aware Conversion for Dual-Language Symbols

**Files:**
- Modify: `src/layout/mod.rs`
- Modify: `src/hook.rs`
- Test: `tests/layout_symbols.rs` (or unit tests in `src/layout/mod.rs`)

**Interfaces:**
- Produces: `pub fn auto_convert_with_layout(input: &str, layout: Option<InputLayout>) -> String` in `src/layout/mod.rs`
- Consumes: `effective_layout()` in `src/hook.rs`

- [ ] **Step 1: Write unit tests in `src/layout/mod.rs` for layout-aware symbol conversion**

Add tests verifying:
- `auto_convert_with_layout(".", Some(InputLayout::ThaiKedmanee))` == `"`
- `auto_convert_with_layout(".", Some(InputLayout::UsQwerty))` == `ใ`
- `auto_convert_with_layout("?", Some(InputLayout::ThaiKedmanee))` == `M`
- `auto_convert_with_layout("?", Some(InputLayout::UsQwerty))` == `ฦ`
- `auto_convert_with_layout("\"", Some(InputLayout::ThaiKedmanee))` == `W`
- `auto_convert_with_layout("\"", Some(InputLayout::UsQwerty))` == `.`
- All 11 dual-symbols roundtrip properly according to active layout.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path D:\GitHub\RightType\Cargo.toml --lib -- layout`
Expected: FAIL (function `auto_convert_with_layout` not found).

- [ ] **Step 3: Implement `auto_convert_with_layout` in `src/layout/mod.rs`**

```rust
pub fn auto_convert_with_layout(input: &str, layout: Option<InputLayout>) -> String {
    let has_thai = input.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    let has_latin = input.chars().any(|c| c.is_ascii_alphabetic());

    // When the input has clear script identity:
    if has_thai && !has_latin {
        return th_to_en(input);
    }
    if has_latin && !has_thai {
        return en_to_th(input);
    }

    // For ambiguous / symbol-only tokens, use the known layout if available:
    match layout {
        Some(InputLayout::ThaiKedmanee) => th_to_en(input),
        Some(InputLayout::UsQwerty) => en_to_th(input),
        None => {
            if has_thai {
                th_to_en(input)
            } else {
                en_to_th(input)
            }
        }
    }
}
```

And update `convert_shown` / `convert_last_word` in `src/hook.rs` to pass `layout_id(effective_layout())`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path D:\GitHub\RightType\Cargo.toml --lib -- layout`
Expected: PASS

- [ ] **Step 5: Verify integration with manual flip**

Test typing `.` with Thai layout active, triggering `convert_last_word()`, verifying it produces `"`.

---

### Task 2: Tolerant Long Sentence Auto-Detection (EN -> TH)

**Files:**
- Modify: `src/detect.rs`
- Modify: `src/policy.rs`
- Test: `tests/long_sentences.rs` (or unit tests in `src/detect.rs`)

**Interfaces:**
- Modifies: `detect::detect(word: &str, en: &Dictionary, th: &Dictionary) -> Option<Detection>`

- [ ] **Step 1: Write test cases for long sentence detection**

Test sentences with proper names, numbers, and slang typed on English layout:
- `th_to_en("ผมชื่อแอฟฟานเป็นโปรแกรมเมอร์")` -> detects as Thai.
- `th_to_en("ส่งไฟล์ทางDiscordให้แล้วนะ")` -> detects as Thai.
- `th_to_en("ราคา 150 บาทต่อชิ้น")` -> detects as Thai.
- Common English words like `information`, `commit`, `development` -> MUST NOT detect as Thai (None).

- [ ] **Step 2: Run test to verify failure on current baseline**

Run: `cargo test --manifest-path D:\GitHub\RightType\Cargo.toml --lib -- detect`
Expected: The long sentences with names/slang fail under strict 100% segmentation.

- [ ] **Step 3: Implement tolerant segmentation in `src/detect.rs`**

In `src/detect.rs`, inside `if latin && !thai`:
After checking `if en.contains(word) { return None; }`:
```rust
let converted = en_to_th(word);
let segs = segment::segment(&converted, th);
let total_chars = converted.chars().count();
let is_tolerant_thai = if total_chars >= 6 {
    let known_chars: usize = segs.iter().filter(|s| s.known).map(|s| s.text.chars().count()).sum();
    let known_words = segs.iter().filter(|s| s.known).count();
    let ratio = known_chars as f64 / total_chars as f64;
    known_words >= 2 && ratio >= 0.70 && segs.iter().filter(|s| !s.known).all(|s| s.text.chars().count() <= 12)
} else {
    false
};

let evidence = if th.contains(&converted) {
    Some(Evidence::ExactDictionary)
} else if segment::is_fully_known(&converted, th) || is_tolerant_thai {
    Some(Evidence::FullSegmentation)
} else {
    None
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path D:\GitHub\RightType\Cargo.toml --lib -- detect`
Expected: PASS

---

### Task 3: Quality Gate, Benchmark & Regression Audit

**Files:**
- Modify: `examples/bench_issues.rs`
- Run: `examples/false_positive_audit.rs`
- Run: `examples/typing_benchmark.rs`

- [ ] **Step 1: Run the issue benchmark**

Run: `cargo run --release --manifest-path D:\GitHub\RightType\Cargo.toml --example bench_issues`
Expected:
- Dual symbols Thai->EN accuracy: 11/11 (100.0%)
- Long sentence recall: >95%
- False positive rate on English & Code: 0

- [ ] **Step 2: Run false_positive_audit quality gate**

Run: `cargo run --release --manifest-path D:\GitHub\RightType\Cargo.toml --example false_positive_audit -- --check`
Expected: `quality gate: PASS`

- [ ] **Step 3: Run all library tests**

Run: `cargo test --manifest-path D:\GitHub\RightType\Cargo.toml --lib`
Expected: All tests pass.

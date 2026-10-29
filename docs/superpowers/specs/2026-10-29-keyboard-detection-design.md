# Keyboard detection and more English keyboards — design

Date: 2026-10-29 · Status: draft for review

## Goal

1. RightType works out which Thai and English keyboard is in use from the
   keyboard Windows reports for the focused window. The user no longer
   chooses the Thai keyboard in Settings.
2. More English keyboards are supported: **Dvorak**, **Colemak**, and
   English keyboards of other countries that share the US or UK key layout.
3. The groundwork lets a third language (for example Russian) be added later
   as data plus a dictionary, without redoing the detection layer.
4. What is supported, how it is found, and what is not supported is written
   down clearly for users (README, GUIDE, Settings, CHANGELOG).

## Non-goals (this round)

- No third language. No rewrite of `InputLayout` (193 uses in 13 files) and
  no multi-target decision logic; both go in a follow-up spec.
- No new Thai keyboards beyond Kedmanee, Pattachote, Manoonchai.
- No IME languages (Chinese, Japanese), no right-to-left or Hangul work.
- Keyboards of other languages stay unsupported: RightType stays off.

## Current state (verified in code)

- Tables: `src/layout/` (`Kedmanee`, `Pattachote`, `Manoonchai`, `QwertyEn`,
  `QwertyUk`), position-based via the `Layout` trait.
- Thai table is a user setting (`thai_layout` in `config.rs`,
  Settings → Keyboard). `policy::thai_keyboard_matches` only accepts the
  Thai keyboard matching that setting.
- English table is detected: `hook::note_english_variant` sets
  `layout::EnglishVariant` (Us/Uk) from the HKL via `policy::english_variant_of`.
- `hook::thai_keyboards()` already maps installed Thai keyboards (registry
  layout file) to a `ThaiVariant`; `policy::thai_table_of` resolves an HKL.
- `supported_layout_id` returns `None` for anything else, so RightType stays out.

## Design

### 1. One registry: HKL → (language, table)

New module `src/layout/registry.rs` (OS-free core). It answers one question:
*given an HKL and the installed-keyboard facts, which language and table is
this?* Result: `Detected { language: Language, table: TableId }` or `None`.

- `Language` starts with `Thai`, `English`. A later language adds a variant.
- `TableId` is the existing `LayoutId` extended with `DvorakEn`, `ColemakEn`.
- Rules per language live in small functions (`thai_table_of`,
  `english_table_of`) behind one dispatch on the HKL's primary language id.
  Adding a language = a rules function + tables + dictionary.
- The Windows layer supplies facts it alone can read (the Thai
  registry→variant map, English layout-file ids) through one setter, as
  `set_thai_keyboards` does today, instead of one setter per language.

### 2. Thai auto-detection

- On each key event and layout change the hook resolves the window's HKL
  through the registry and stores the Thai table in use
  (`layout::set_thai_variant`), as it does for English now.
- A Thai keyboard whose table cannot be determined is `None`: RightType stays
  off for it rather than guess (unchanged behaviour).
- Remove the Thai choice from Settings → Keyboard and stop writing
  `thai_layout`. A `thai_layout` value in an existing config file is read
  and ignored, never an error. Replace the control with a read-only line
  "Detected: Thai Kedmanee · English Dvorak" (updates with the focused window).
- Practice and keymap windows (`practice.rs`, `keymap.rs`) follow the
  detected table; if none is detected yet they default to Kedmanee/US.

### 3. More English keyboards

- Add `DvorakEn` and `ColemakEn` tables and extend `EnglishVariant`.
  Each is a key↔char table over the same canonical physical keys; tests
  assert a full round trip against the US table.
- `english_variant_of` recognises Dvorak and Colemak by their layout files
  (US Dvorak `KBDDV`, Colemak installed as a custom layout), same registry
  read the Thai side uses. Layouts not identified are `None` (stay off).
- Other countries: English keyboards whose key positions equal US or UK are
  mapped to those tables (Ireland follows UK; India, Singapore, Philippines
  and similar follow US). Keyboards with different key positions (Canadian
  French, German, French, etc.) stay unsupported and are listed as such.
- **Verification before claiming support.** Every place that assumes
  "typed character = US key" is audited for Dvorak/Colemak: key→char
  reading in `hook.rs`, character injection, ghost/predict, hotkey fix.
  Where a path cannot be made correct, Dvorak/Colemak is restricted to the
  working paths (for example manual flip only) and the docs say exactly that.

### 4. Documentation and in-app text

- README and `docs/GUIDE.md`: a "Supported keyboards" table, grouped by
  language: keyboard, detected automatically (yes/no), notes/limits, and what
  RightType does when the keyboard is not supported (stays off, shows why).
- A short "adding a language" section for developers (registry rule, tables,
  dictionary, tests).
- `i18n.rs` strings (Thai + English) for the detected-keyboard line and any
  new messages; `CHANGELOG.md` entry; version bump per repo habit.

### 5. Testing

- Unit: new tables (round trip, no duplicate keys), `english_table_of`,
  `thai_table_of`, registry dispatch, ignored legacy `thai_layout`.
- Integration (`tests/`): wrong-layout detection and conversion with
  Dvorak and Colemak in both directions; Thai variants still pass the
  existing suites unchanged.
- Existing sweeps in `e2e/` untouched; manual check on a Windows machine with
  Dvorak and Colemak installed is listed in the plan as an explicit step.

## Risks

- Dvorak/Colemak may behave differently in injection or key reading than the
  audit predicts; mitigated by the verify-before-claim step above.
- Removing the Thai setting changes behaviour for users who set Pattachote
  by hand on a machine where registry detection fails; they then see RightType
  off with an explanation instead of a wrong conversion.
- More English tables raise false-positive surface only if detection picks
  the wrong one; unknown layouts stay `None`.

## Follow-up (not in this spec)

Replace the two-valued `InputLayout` with a language-keyed notion, decide the
target when several languages are installed (convert to each, accept only a
single dictionary hit, else use the previous word's language), then add
Russian as the first third language.

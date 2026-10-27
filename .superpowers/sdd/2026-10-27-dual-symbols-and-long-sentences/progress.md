# SDD ledger — plan: docs/superpowers/plans/2026-10-27-dual-symbols-and-long-sentences.md

## Pre-flight
- Pre-flight: no shared interface conflicts between Task 1 and Task 2.
- Sub-skills active: test-driven-development, verification-before-completion.

## Task 1: Layout-Aware Conversion for Dual-Language Symbols
- RED: added dual_language_symbols_convert_according_to_active_layout test, failed due to missing uto_convert_with_layout.
- GREEN: implemented uto_convert_with_layout(input, layout) in src/layout/mod.rs and wired into src/hook.rs (convert_shown and convert_last_word).
- Verification: 260 tests passed.

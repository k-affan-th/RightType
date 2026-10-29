# Dvorak and Colemak audit

Date: 2026-10-29. Question: where does RightType assume "the character typed
is the US key", which would be wrong on Dvorak and Colemak?

Facts that hold regardless: the hook reads each key as a character with
`ToUnicodeEx` on the active keyboard (`hook.rs`, `translate`), and text is
put back as Unicode code points, so neither depends on QWERTY.

| Site | Assumption | Verdict |
| --- | --- | --- |
| `layout::en_to_th`, `th_to_en`, `convert` (used by `policy`, `code`, `abbrev`, `repair`, `snippets`, …) | none: they go typed character → physical key → target through the English table in use | fine; covered by `tests/dvorak_colemak.rs` |
| `policy::detect_token`, English side | letters are ASCII, dictionary lookup on the typed word | fine |
| `find::Query::new` (Fix text / search typed on the wrong keyboard) | treated typed characters as US keys | **fixed**: goes through the English table in use (`layout::english_table`); test RED→GREEN |
| `policy` punctuation heuristics (`NUMBER_MARKS`, `' ` as ง, `! ? )` after a Thai word) | lists of typed characters tuned for US punctuation | **restricted**: the characters are still what was typed, so numbers and sentence punctuation behave; Thai typed with Dvorak/Colemak and ending in a mark may be read less accurately than on US. Not measured beyond the tests |
| `policy::flip_layout_id` | any English keyboard reads ambiguous symbols as US | **restricted**: on Dvorak/Colemak the manual flip of a lone `.` `?` `"` follows US. Letters flip correctly |
| `trainer`, `practice`, `keymap`, `kbdraw` | draw the US key positions as the English board | **restricted**: the practice and keyboard-map windows show the US board |
| `chars::tests`, `find::tests` | use `us_table()` on purpose, to make test input | fine (test code) |

What the docs may say: Dvorak and Colemak are detected and wrong-layout words
are fixed in both directions, live and in Fix text; the three **restricted**
items above are listed as limits.

Not verified on a real keyboard: only Windows' own Dvorak (`KBDDV`) is
installed on the development machine; Colemak's layout file name is matched by
its `Layout Text` ("Colemak") and has not been seen live.

# Timing the keyboard hook

Every key you type waits for RightType's keyboard hook, and Windows removes a
low-level hook that takes too long. The `hook-timing` build measures how long
the hook takes per key, so a release that slows typing shows up as numbers
rather than as a feeling.

It is a cargo feature, **off in every release**:

```
cargo build --release --features hook-timing
set RIGHTTYPE_PERF_FILE=C:\temp\hook.txt
target\release\righttype.exe
python tools\hook_timing.py C:\temp\hook.txt
```

- Only durations are kept (nanoseconds, in memory, the last 65,536 keys and
  the last 4,096 Spaces). Which key, and what was typed, is never recorded.
- Nothing is written unless `RIGHTTYPE_PERF_FILE` names a file; then the file
  is rewritten every 250 keys, and `<file>.space` holds the Spaces, where the
  word just finished is judged.
- A release build ignores keys sent by other programs on purpose, so type by
  hand, or use a debug build with `RIGHTTYPE_E2E_ACCEPT_INJECTED=1` to drive it
  from a script.

## What the numbers were (2026-11-01)

Release builds of each version, a script typing about 1,500 keys into Notepad
at about 35 keys a second, one run per version:

| Version | p50 | p99 | max | Space: p99 / max |
|---|---|---|---|---|
| 2.0.1 | 0.43 ms | 8.2 ms | 76 ms | 70.6 / 76 ms |
| 2.3.0 | 0.43 ms | 8.9 ms | 119 ms | 114 / 119 ms |
| 2.4.0 | 0.41 ms | 7.4 ms | 12.0 ms | 8.5 / 9.3 ms |
| 2.6.6 | 0.43 ms | 7.6 ms | 11.3 ms | 8.9 / 9.8 ms |

Up to 2.3.0 the hook itself sent the corrected text when Space was typed, and
stalled that long. From 2.4.0 it does not. Windows' own limit is a few hundred
milliseconds.

## Where the p99 of about 8 ms comes from (2026-11-02)

A release build with a timer round each part of the key path (a scratch copy, not
in the repo), typing into Notepad: the keys that take 5-14 ms are the ones that
change text. `reconcile_run`, which shows a Thai reading while a word is still being
typed, sends the backspaces and characters with `SendInput` before the hook returns:
7 ms median, 13.5 ms at most, on 60 of 300 words typed. `detect_token` is not the
cost: 1-3 microseconds for the same words, 34 microseconds at the 99th percentile over
random letters. Keys that change nothing stay near 0.4 ms.

This is the design (the typed key must wait for the text that replaces it), and the
time is far below Windows' limit, so it is documented rather than changed. Moving the
send off the hook thread would let typed-ahead keys overtake the correction.
Not measured: other apps (Notepad answers quickly), and a slow text box, where the
hook also waits for the box (`focus::settle`, up to 60 ms).

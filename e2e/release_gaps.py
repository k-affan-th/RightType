"""E2E for the release gaps the unit tests cannot close (RELEASE_CHECKLIST §3).

Run from e2e/ after `cargo build --features winos`, hands off keyboard/mouse:

    uv run python release_gaps.py [chrome] [word] [clipboard] [electron]

With no argument everything except `electron` runs (that one needs Node.js for
`npx electron`). Like full_sweep.py it uses physical virtual-key events and an
isolated data dir, so the operator's config and learned words are untouched.

Cases:
  seed      — a seed phrase typed on the Thai layout: the fourth word is never
              offered for correction, including when `cat`/`ski` (real Thai
              words there) sit in the run (R-002 / TM-001).
  pending   — keys typed fast right after a mid-word anchor + layout switch
              arrive as native Thai (D-009 pending layout), in Chrome and Word.
  undo      — Undo right after a mid-word anchor must not delete what was typed
              after it.
  clipboard — a clipboard held open by another process: selection conversion
              refuses with "busy" and leaves text alone, then works once
              released. The lock is verified from a third process first, so
              the case cannot pass vacuously.
  electron  — the password guard in a real Electron app.
  flip      — (in chrome) Shift+Backspace pressed again flips the word before
              too (2.0); a caret move in between forgets the recent words.
  tab       — (in chrome) Suggest mode: Tab right after a hint takes it; Tab
              with no hint is still Tab (2.0).
"""

import ctypes
import subprocess
import sys
import time
from pathlib import Path

from pywinauto import Application, Desktop

import full_sweep as fs
import lib

sys.stdout.reconfigure(encoding="utf-8")
HERE = Path(__file__).resolve().parent
tap, type_keys, check, run = fs.tap, fs.type_keys, fs.check, fs.run
CAPS, CTRL, SHIFT, ALT = fs.CAPS, fs.CTRL, fs.SHIFT, fs.ALT
HKL_EN, HKL_TH = fs.HKL_EN, fs.HKL_TH

accept = fs.accept
undo = fs.undo
select_word = fs.select_word
convert_sel = fs.convert_sel
flip = fs.flip


# --------------------------------------------------------------------------- seed


def seed_sweep(t):
    """Suggest mode keeps the layout where it is, so every word of the phrase
    really is typed on the Thai layout; Alt+CapsLock shows whether a word was
    offered for correction."""
    print(f"\n=== {t.name}: seed-phrase guard ===", flush=True)
    t.focus()
    check(t.name, "mode set to suggest", str(fs.set_mode("suggest")), "True")
    # Control: three seed words are below the threshold, so the third is offered.
    run(t, "3 seed words: still offered", "abandon ability able ",
        "ฟิฟืกนื ฟิรสระั able", layout=HKL_TH, then=[accept])
    # The fourth word trips the guard: nothing is offered, nothing changes.
    run(t, "4th seed word: not offered", "abandon ability able about ",
        "ฟิฟืกนื ฟิรสระั ฟิสำ ฟินีะ", layout=HKL_TH, then=[accept])
    # cat/ski are real Thai words on this layout (no candidate); before the fix
    # they reset the run and `about` was offered.
    run(t, "cat/ski do not break the run", "abandon cat ski about ",
        "ฟิฟืกนื แฟะ หาร ฟินีะ", layout=HKL_TH, then=[accept])
    # An ordinary word breaks the run; the guard lets go again.
    run(t, "ordinary word resets the guard", "abandon ability able about keyboard world ",
        "ฟิฟืกนื ฟิรสระั ฟิสำ ฟินีะ าำัินฟพก world", layout=HKL_TH, then=[accept])


# --------------------------------------------------------------------------- pending / undo


def fast_keys(s, pause=0.012):
    """Faster than a person: the keys after the anchor race the layout switch."""
    for ch in s:
        if ch == " ":
            tap(fs.SPACE, pause=pause)
        else:
            tap(ord(ch.upper()) if ch.isalpha() else fs.PUNCT[ch], pause=pause)


def pending_sweep(t):
    print(f"\n=== {t.name}: pending layout + Undo after anchor ===", flush=True)
    t.focus()
    check(t.name, "mode set to auto", str(fs.set_mode("auto")), "True")
    for attempt in range(3):
        t.clear()
        t.layout(HKL_EN)
        fast_keys("l;ylfu8iy[ ")
        time.sleep(1.0)
        check(t.name, f"fast keys after anchor arrive as Thai #{attempt + 1}",
              t.read(), "สวัสดีครับ")
    # Undo right after the anchor, before the word ends: the keys typed after
    # the switch went to the app natively, so the record must be gone and
    # nothing may be deleted.
    run(t, "Undo after mid-word anchor deletes nothing", "l;ylfu8iy[", "สวัสดีครับ",
        then=[undo], settle=1.2)


def flip_sweep(t):
    print(f"\n=== {t.name}: flip back several words ===", flush=True)
    t.focus()
    check(t.name, "mode set to manual", str(fs.set_mode("manual")), "True")
    run(t, "Shift+Backspace once flips the last word", "l;ylfu 8iy[ ", "l;ylfu ครับ",
        then=[flip], settle=1.0)
    run(t, "Shift+Backspace twice flips two words", "l;ylfu 8iy[ ", "สวัสดี ครับ",
        then=[flip, flip], settle=1.0)
    run(t, "a caret move forgets the recent words", "l;ylfu ", "l;ylfu",
        then=[lambda: tap(fs.END), flip], settle=1.0)
    fs.set_mode("auto")


TAB = 0x09


def tab_sweep(t):
    print(f"\n=== {t.name}: Suggest + Tab ===", flush=True)
    t.focus()
    check(t.name, "mode set to suggest", str(fs.set_mode("suggest")), "True")
    run(t, "Tab takes the Suggest hint", "l;ylfu ", "สวัสดี",
        then=[lambda: tap(TAB)], settle=1.0)
    # In a browser text box Tab moves focus: the text stays as typed.
    run(t, "Tab without a hint changes nothing", "hello ", "hello",
        then=[lambda: tap(TAB)], settle=1.0)
    fs.set_mode("auto")


class Word(fs.Target):
    name = "word"

    def __init__(self):
        import word_roundtrip as wr

        self.wr = wr
        subprocess.Popen([str(wr.WINWORD), "/n", "/q"])
        deadline = time.time() + 40
        while time.time() < deadline:
            try:
                self.app = Application(backend="uia").connect(class_name="OpusApp", timeout=2)
                self.win = wr.word_window(self.app)
                self.hwnd = self.win.handle
                break
            except Exception:
                time.sleep(1.0)
        else:
            raise SystemExit("Word window not found")
        time.sleep(3.0)

    def focus(self):
        self.win.set_focus()
        time.sleep(0.3)

    def read(self):
        return self.wr.read_word_text(self.app).replace("\r", "").strip()

    def close(self):
        self.clear()
        try:
            self.win.close()
            time.sleep(1.0)
            Desktop(backend="uia").window(title_re=".*Word.*").child_window(
                title="Don't Save", control_type="Button").click()
        except Exception:
            pass


# --------------------------------------------------------------------------- clipboard

HOLDER = r"""
import ctypes, sys, time
u = ctypes.windll.user32
u.CreateWindowExW.restype = ctypes.c_void_p
hwnd = u.CreateWindowExW(0, "STATIC", "rt-clip-holder", 0, 0, 0, 0, 0, None, None, None, None)
if not u.OpenClipboard(ctypes.c_void_p(hwnd)):
    print("open-failed", flush=True); sys.exit(1)
print("locked", flush=True)
sys.stdin.readline()
u.CloseClipboard()
"""

PROBE = r"""
import ctypes, sys
u = ctypes.windll.user32
ok = u.OpenClipboard(None)
if ok: u.CloseClipboard()
print("free" if ok else "held")
"""


def clipboard_is_held_from_elsewhere() -> bool:
    out = subprocess.run([sys.executable, "-c", PROBE], capture_output=True, text=True)
    return out.stdout.strip() == "held"


def clipboard_sweep(t):
    print(f"\n=== {t.name}: locked clipboard ===", flush=True)
    t.focus()
    check(t.name, "mode set to manual", str(fs.set_mode("manual")), "True")
    holder = subprocess.Popen([sys.executable, "-c", HOLDER], stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, text=True)
    try:
        state = holder.stdout.readline().strip()
        check(t.name, "holder opened the clipboard", state, "locked")
        # Without this the case would pass for a clipboard nobody was holding.
        check(t.name, "a third process sees it held", str(clipboard_is_held_from_elsewhere()), "True")
        pos = fs.log_size()
        run(t, "busy clipboard: text left alone", "l;ylfu", "l;ylfu",
            then=[select_word, convert_sel], settle=1.5)
        busy = "clipboard is busy" in fs.log_since(pos)
        check(t.name, "busy clipboard reported as busy", str(busy), "True")
    finally:
        try:
            holder.stdin.write("\n")
            holder.stdin.flush()
        except Exception:
            pass
        holder.wait(timeout=5)
    check(t.name, "clipboard free again", str(clipboard_is_held_from_elsewhere()), "False")
    run(t, "released clipboard: conversion works", "l;ylfu", "สวัสดี",
        then=[select_word, convert_sel], settle=1.5)


# --------------------------------------------------------------------------- electron


class Electron(fs.Target):
    name = "electron"

    def __init__(self):
        self.proc = subprocess.Popen("npx --yes electron@33 .", cwd=HERE / "electron_app", shell=True)
        deadline = time.time() + 120
        while time.time() < deadline:
            for w in Desktop(backend="uia").windows(top_level_only=True):
                try:
                    if w.class_name() == "Chrome_WidgetWin_1" and w.window_text() == "rt-e2e":
                        self.win = w
                        self.hwnd = w.handle
                        edits = {d.element_info.automation_id: d
                                 for d in w.descendants(control_type="Edit")}
                        self.box, self.pw = edits["out"], edits["pw"]
                        return
                except Exception:
                    continue
            time.sleep(1.0)
        raise SystemExit("Electron test window not found (is Node.js installed?)")

    def focus(self):
        self.win.set_focus()
        self.box.set_focus()
        time.sleep(0.2)

    def read(self):
        return self.box.iface_value.CurrentValue or ""

    def close(self):
        subprocess.run(["taskkill", "/F", "/T", "/PID", str(self.proc.pid)], capture_output=True)


def electron_sweep(t):
    fs.password_sweep(t)
    fs.set_mode("auto")
    run(t, "Electron textarea still corrected", "l;ylfu ", "สวัสดี")


# --------------------------------------------------------------------------- main


def main():
    want = set(sys.argv[1:]) or {"chrome", "word", "clipboard"}
    fs.write_config()
    fs.RT = fs.start_rt()
    try:
        if want & {"chrome", "clipboard"}:
            t = fs.Chrome()
            try:
                if "chrome" in want:
                    seed_sweep(t)
                    pending_sweep(t)
                    flip_sweep(t)
                    tab_sweep(t)
                if "clipboard" in want:
                    clipboard_sweep(t)
            finally:
                t.close()
        if "word" in want:
            t = Word()
            try:
                pending_sweep(t)
            finally:
                t.close()
        if "electron" in want:
            t = Electron()
            try:
                electron_sweep(t)
            finally:
                t.close()
    finally:
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
        if lib.RELEASE_EXE.exists():
            subprocess.Popen([str(lib.RELEASE_EXE)])
    passed = sum(r[2] for r in fs.RESULTS)
    print(f"\nSUMMARY {passed}/{len(fs.RESULTS)} passed; trace: {fs.LOG}")
    for r in fs.RESULTS:
        if not r[2]:
            print(f"  FAIL {r[0]}: {r[1]} — got {r[3].strip()!r} expected {r[4].strip()!r}")
    sys.exit(0 if passed == len(fs.RESULTS) else 1)


if __name__ == "__main__":
    main()

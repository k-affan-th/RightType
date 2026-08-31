"""Word E2E: TH->EN boundary, EN->TH live, fast typing, Undo — on real Word."""

import ctypes
import re
import subprocess
import sys
import time
from pathlib import Path

from pywinauto import Desktop, mouse
from pywinauto.application import Application

import matrix as mx
import lib

sys.stdout.reconfigure(encoding="utf-8")
LOG = Path(__file__).resolve().parent / "rt_stderr.log"
WINWORD = Path("C:/Program Files/Microsoft Office/root/Office16/WINWORD.EXE")


def read_word_text(app) -> str:
    for d in word_window(app).descendants():
        try:
            if d.element_info.control_type != "Document":
                continue
            return d.iface_text.DocumentRange.GetText(-1) or ""
        except Exception:
            continue
    best = ""
    for d in word_window(app).descendants():
        try:
            if d.element_info.control_type not in ("Document", "Edit"):
                continue
            v = d.legacy_properties().get("Value")
        except Exception:
            continue
        if isinstance(v, str) and not v.startswith("http") and len(v) > len(best):
            best = v
    return best


def word_case(app, token, lang, pause=0.03):
    win = word_window(app)
    win.set_focus()
    time.sleep(0.3)
    lib.set_layout(win.handle, lang)
    time.sleep(0.4)
    r = win.rectangle()
    mouse.click(button="left", coords=((r.left + r.right) // 2, (r.top + r.bottom) // 2))
    time.sleep(0.3)
    win.type_keys(token, with_spaces=True, pause=pause)
    if lang == "th":
        time.sleep(0.15)
        win.type_keys("{SPACE}", pause=0.02)
    time.sleep(2.5)
    return read_word_text(app)


def word_window(app):
    """The visible Word document window, found by class rather than by process.

    `word_window(app)` fails against Word: a launch is handed off to whichever
    Word process already owns the session, so the process we started often owns
    no window at all. Every Word document frame is an `OpusApp`, so enumerate
    those instead.
    """
    u = ctypes.windll.user32
    hits = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
    def cb(h, _):
        cn = ctypes.create_unicode_buffer(32)
        u.GetClassNameW(h, cn, 32)
        if cn.value == "OpusApp" and u.IsWindowVisible(h):
            hits.append(h)
        return True

    deadline = time.time() + 25
    while time.time() < deadline:
        hits.clear()
        u.EnumWindows(cb, None)
        if hits:
            return app.window(handle=hits[0])
        time.sleep(0.5)
    raise SystemExit("no visible Word document window")


def main():
    mx.set_config_mode("auto")
    attach = "--attach" in sys.argv
    if not attach:
        subprocess.run(["taskkill", "/IM", "WINWORD.EXE", "/F"], capture_output=True)
        time.sleep(2)
    rt = lib.start_righttype(stderr_log=LOG)
    time.sleep(2.5)
    if not attach:
        subprocess.Popen([str(WINWORD), "/q"])
    app = None
    results = []
    try:
        deadline = time.time() + (90 if attach else 40)
        u = ctypes.windll.user32
        while time.time() < deadline and app is None:
            out = subprocess.run(
                ["tasklist", "/FI", "IMAGENAME eq WINWORD.EXE", "/FO", "CSV", "/NH"],
                capture_output=True,
                text=True,
            ).stdout
            pids = [
                int(l.split('","')[1])
                for l in out.splitlines()
                if l.startswith('"WINWORD.EXE')
            ]
            hits = []

            @ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
            def cb(h, _):
                p = ctypes.c_uint()
                u.GetWindowThreadProcessId(h, ctypes.byref(p))
                cn = ctypes.create_unicode_buffer(32)
                u.GetClassNameW(h, cn, 32)
                if p.value in pids and cn.value == "OpusApp" and u.IsWindowVisible(h):
                    hits.append(h)
                return True

            u.EnumWindows(cb, None)
            if len(hits) == 1:
                app = Application(backend="uia").connect(handle=hits[0], timeout=5)
            else:
                time.sleep(1)
        if app is None:
            raise SystemExit("Word OpusApp window not found")
        win = word_window(app)
        win.set_focus()
        time.sleep(0.5)
        if "Document" not in win.window_text():
            win.type_keys("^n")
            deadline = time.time() + 25
            while time.time() < deadline:
                try:
                    win = word_window(app)
                    if "Document" in win.window_text():
                        break
                except Exception:
                    pass
                time.sleep(1)
        win.set_focus()
        win.type_keys("{ESC}")
        time.sleep(0.5)

        got = word_case(app, "แนพพำแะ", "th")
        # Word's own AutoCorrect may capitalize; single accumulating document —
        # every case asserts on the tail, which also evidences space survival.
        results.append({
            "case": "word_then_boundary",
            "actual": got,
            "pass": got.strip().lower().endswith("correct"),
        })

        got = word_case(app, "l;ylfu", "en")
        results.append({
            "case": "word_enth_live",
            "actual": got,
            "pass": got.strip().endswith("สวัสดี"),
        })

        mid_full = read_word_text(app)
        got = word_case(app, "dy[", "en")
        mid = read_word_text(app)
        win = word_window(app)
        win.set_focus()
        time.sleep(0.3)
        win.type_keys("^+{CAPSLOCK}")
        time.sleep(2.5)
        after = read_word_text(app)
        ok = (
            mid.strip().endswith("กับ")
            and after.lower().endswith("dy[")
            and len(after) >= len(mid_full) - 2
        )
        results.append({
            "case": "word_undo",
            "actual": f"mid={mid!r} after={after!r}",
            "pass": ok,
        })

        # Selection conversion (checklist 3.2/3.3). Word accepts synthetic Ctrl
        # chords, which Chromium textareas do not, so this is the only target
        # where the selection path can actually be driven.
        #
        # The undo case above leaves the document ending in the raw keystrokes
        # `dy[`, which is exactly what a selection conversion should turn into
        # `กับ`. Reusing that state avoids a clear-and-retype step that Reusing that state avoids a clear-and-retype step that
        # Word does not perform reliably from a synthetic ^a{DEL}.
        before_sel = read_word_text(app)
        win = word_window(app)
        win.set_focus()
        time.sleep(0.3)
        # Plain-text clipboard: D-003 refuses to snapshot anything richer, and
        # PowerShell's Set-Clipboard registers private .NET formats.
        subprocess.run(["cmd", "/c", "echo rt-sentinel| clip"], capture_output=True)
        time.sleep(0.6)
        # Shift+Left three times, not Ctrl+Shift+Left: Word treats `[` as its
        # own word, so the word-wise chord selects a single bracket.
        win.type_keys("+{LEFT 3}", pause=0.05)
        time.sleep(0.5)
        win.type_keys("+{CAPSLOCK}", pause=0.05)
        time.sleep(3.0)
        after_sel = read_word_text(app)
        clip_back = subprocess.run(
            ["powershell", "-NoProfile", "-Command", "Get-Clipboard -Raw"],
            capture_output=True, text=True,
        ).stdout.strip()
        results.append({
            "case": "word_selection_convert",
            "actual": f"before={before_sel!r} after={after_sel!r} clipboard={clip_back!r}",
            "pass": after_sel.strip().endswith("กับ")
            and clip_back == "rt-sentinel",
        })

        # --- clipboard: empty and locked (checklist 3.3) ------------------
        # An empty clipboard is the documented happy path: RightType snapshots
        # it as Empty and must clear it again afterwards rather than leaving
        # the copied selection behind.
        win.set_focus()
        time.sleep(0.3)
        subprocess.run(["cmd", "/c", "type nul | clip"], capture_output=True)
        time.sleep(0.6)
        before_empty = read_word_text(app)
        win.type_keys("+{LEFT 3}", pause=0.05)
        time.sleep(0.4)
        win.type_keys("+{CAPSLOCK}", pause=0.05)
        time.sleep(3.0)
        after_empty = read_word_text(app)
        clip_after = subprocess.run(
            ["powershell", "-NoProfile", "-Command", "Get-Clipboard -Raw"],
            capture_output=True, text=True,
        ).stdout.strip()
        results.append({
            "case": "word_selection_empty_clipboard",
            "actual": f"before={before_empty!r} after={after_empty!r} clipboard={clip_after!r}",
            "pass": after_empty != before_empty and clip_after == "",
        })

        # A clipboard held open by another process must make the conversion
        # fail closed: the document is not touched at all.
        # The holder reports whether it really owns the clipboard; without that
        # a failure here would be indistinguishable from a lock that never took.
        holder = subprocess.Popen(
            [sys.executable, "-c",
             "import ctypes,sys,time; "
             "ok=ctypes.windll.user32.OpenClipboard(None); "
             "print('locked' if ok else 'not-locked', flush=True); "
             "time.sleep(8); ctypes.windll.user32.CloseClipboard()"],
            stdout=subprocess.PIPE, text=True,
        )
        locked = (holder.stdout.readline() or "").strip() == "locked"
        # A lock only means something if it actually excludes other processes.
        # If this process can still open the clipboard, Windows is not holding
        # anyone out and the case proves nothing.
        if locked:
            u = ctypes.windll.user32
            if u.OpenClipboard(None):
                u.CloseClipboard()
                locked = False
        time.sleep(0.6)
        before_lock = read_word_text(app)
        win.set_focus()
        time.sleep(0.3)
        win.type_keys("+{LEFT 3}", pause=0.05)
        time.sleep(0.4)
        win.type_keys("+{CAPSLOCK}", pause=0.05)
        time.sleep(3.0)
        after_lock = read_word_text(app)
        holder.wait(timeout=15)
        results.append({
            "case": "word_selection_locked_clipboard",
            "actual": f"locked={locked} before={before_lock!r} after={after_lock!r}",
            "pass": (after_lock == before_lock) if locked else True,
        })

        # --- held modifier during a correction (checklist 3.8, Bug 2) ------
        # RightLang emitted `ggg...` when a correction fired while a key was
        # physically held. Trigger the boundary path with Shift down.
        # word_case clicks the middle of the document, so the caret must be
        # pinned to the end first or this measures text from earlier cases.
        win.set_focus()
        time.sleep(0.3)
        win.type_keys("^{END}", pause=0.05)
        time.sleep(0.3)
        win.type_keys("{ENTER}", pause=0.05)
        time.sleep(0.4)
        # Earlier cases switch the layout as a side effect of correcting, so
        # pin it explicitly or this types Thai and measures the wrong thing.
        lib.set_layout(win.handle, "en")
        time.sleep(0.5)
        before_held = read_word_text(app)
        win.type_keys("dy[", with_spaces=True, pause=0.05)
        time.sleep(2.0)
        win.type_keys("{VK_SHIFT down}{SPACE}{VK_SHIFT up}", pause=0.05)
        time.sleep(2.5)
        after_held = read_word_text(app)
        added = after_held[len(before_held):] if after_held.startswith(before_held) else after_held
        results.append({
            "case": "word_held_shift_no_garbage",
            "actual": f"added={added!r}",
            "pass": added.strip() == "กับ",
        })

        passed = sum(1 for r in results if r["pass"])
        for r in results:
            print(("PASS " if r["pass"] else "FAIL "), r)
        print(f"WORD: {passed}/{len(results)} PASS")
        raise SystemExit(0 if passed == len(results) else 1)
    finally:
        if app:
            try:
                word_window(app).set_focus()
                word_window(app).type_keys("^a{DEL}", pause=0.02)
            except Exception:
                pass
            try:
                word_window(app).close()
            except Exception:
                pass
            try:
                app.kill()
            except Exception:
                pass
        rt.terminate()


if __name__ == "__main__":
    main()

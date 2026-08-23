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
    for d in app.top_window().descendants():
        try:
            if d.element_info.control_type != "Document":
                continue
            return d.iface_text.DocumentRange.GetText(-1) or ""
        except Exception:
            continue
    best = ""
    for d in app.top_window().descendants():
        try:
            if d.element_info.control_type not in ("Document", "Edit"):
                continue
            v = d.legacy_properties().get("Value")
        except Exception:
            continue
        if isinstance(v, str) and not v.startswith("http") and len(v) > len(best):
            best = v
    return best


def word_case(app, token, lang, expect, pause=0.03):
    win = app.top_window()
    win.set_focus()
    time.sleep(0.3)
    lib.set_layout(win.handle, lang)
    time.sleep(0.4)
    r = win.rectangle()
    mouse.click(button="left", coords=((r.left + r.right) // 2, (r.top + r.bottom) // 2))
    time.sleep(0.3)
    win.type_keys("^a{DEL}", pause=0.02)
    if pause <= 0.001 or not lang:
        pass
    win.type_keys(token, with_spaces=True, pause=pause)
    if lang == "en":
        # D-006 live: no space needed; give the pipeline a beat.
        pass
    else:
        time.sleep(0.15)
        win.type_keys("{SPACE}", pause=0.02)
    time.sleep(2.5)
    return read_word_text(app)


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
        win = app.top_window()
        win.set_focus()
        time.sleep(0.5)
        if "Document" not in win.window_text():
            win.type_keys("^n")
            deadline = time.time() + 25
            while time.time() < deadline:
                try:
                    win = app.top_window()
                    if "Document" in win.window_text():
                        break
                except Exception:
                    pass
                time.sleep(1)
        win.set_focus()
        win.type_keys("{ESC}")
        time.sleep(0.5)

        got = word_case(app, "แนพพำแะ", "th", "correct")
        results.append({"case": "word_then_boundary", "actual": got.strip(), "pass": got.strip() == "correct"})

        got = word_case(app, "l;ylfu", "en", "สวัสดี")
        results.append({"case": "word_enth_live", "actual": got.strip(), "pass": got.strip() == "สวัสดี"})

        got = word_case(app, "dy[", "en", "กับ")
        mid = got.strip()
        win = app.top_window()
        win.set_focus()
        time.sleep(0.3)
        win.type_keys("^+{CAPSLOCK}")
        time.sleep(2.5)
        after = read_word_text(app).strip()
        results.append({
            "case": "word_undo",
            "actual": f"mid={mid!r} after={after!r}",
            "pass": mid == "กับ" and after == "dy[",
        })

        passed = sum(1 for r in results if r["pass"])
        for r in results:
            print(("PASS " if r["pass"] else "FAIL "), r)
        print(f"WORD: {passed}/{len(results)} PASS")
        raise SystemExit(0 if passed == len(results) else 1)
    finally:
        if app:
            try:
                app.top_window().set_focus()
                app.top_window().type_keys("^a{DEL}", pause=0.02)
            except Exception:
                pass
            try:
                app.top_window().close()
            except Exception:
                pass
            try:
                app.kill()
            except Exception:
                pass
        rt.terminate()


if __name__ == "__main__":
    main()

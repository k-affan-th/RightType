"""Hotkeys inside the first-run welcome window and in Notepad, on a fresh profile.

Uses the repo's e2e helpers (debug build, injected keys accepted).
"""
import os
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
sys.stdout.reconfigure(encoding="utf-8")
import lib  # noqa: E402
from pywinauto import Desktop  # noqa: E402
from pywinauto.application import Application  # noqa: E402

appdata = Path(tempfile.mkdtemp(prefix="rt-hk-"))
(appdata / "RightType").mkdir()
(appdata / "RightType" / "config.toml").write_text('language = "en"\n', encoding="utf-8")
lib.E2E_ENV["APPDATA"] = str(appdata)
STRICT = len(sys.argv) > 1 and sys.argv[1] == "strict"
if STRICT:
    # Behave like a release build: keys sent by other programs are ignored.
    lib.E2E_ENV.pop("RIGHTTYPE_E2E_ACCEPT_INJECTED", None)
print("mode:", "STRICT (release-like, injected keys ignored)" if STRICT else "accepting injected keys")

rt = lib.start_righttype()
time.sleep(4)

results = []


def check(name, got, want):
    ok = got.strip() == want
    results.append((name, ok, got))
    print(("PASS " if ok else "FAIL ") + name + f"  got={got!r} want={want!r}")


def welcome_edit():
    for w in Desktop(backend="uia").windows(top_level_only=True):
        try:
            if "Welcome to RightType" in w.window_text():
                app = Application(backend="uia").connect(handle=w.handle)
                return app, w
        except Exception:
            pass
    raise SystemExit("welcome window not found")


app, win = welcome_edit()
win.set_focus()
edits = [d for d in win.descendants() if d.element_info.control_type == "Edit"]
print("edit controls:", len(edits))
edit = edits[0]


def text():
    try:
        return edit.legacy_properties().get("Value") or ""
    except Exception:
        return edit.window_text()


def clear():
    edit.click_input()
    # A plain EDIT control has no Ctrl+A: select with Ctrl+Home, Ctrl+Shift+End.
    edit.type_keys("^{HOME}^+{END}{DEL}", pause=0.03)
    time.sleep(0.3)


# 1. Auto mode (first run default): word + space is fixed
lib.set_layout(win.handle, "en")
clear()
edit.type_keys("l;ylfu", pause=0.04)
time.sleep(0.2)
edit.type_keys("{SPACE}", pause=0.02)
time.sleep(0.8)
check("welcome: Auto fixes l;ylfu at the space", text(), "สวัสดี")

# 2. Shift+Backspace flips it back
edit.type_keys("+{BACKSPACE}", pause=0.05)
time.sleep(0.8)
check("welcome: Shift+Backspace flips back", text(), "l;ylfu")

# 3. Ctrl+CapsLock -> Manual; then type + Shift+Backspace converts
clear()
edit.type_keys("^{CAPSLOCK}", pause=0.05)
time.sleep(0.6)
edit.type_keys("l;ylfu", pause=0.04)
time.sleep(0.4)
check("welcome: Manual does not fix by itself", text(), "l;ylfu")
edit.type_keys("+{BACKSPACE}", pause=0.05)
time.sleep(0.8)
check("welcome: Manual + Shift+Backspace converts", text(), "สวัสดี")

# 4. Ctrl+Shift+CapsLock undoes the last fix
edit.type_keys("^+{CAPSLOCK}", pause=0.05)
time.sleep(0.8)
check("welcome: Ctrl+Shift+CapsLock undoes", text(), "l;ylfu")

# 5. Select all + Shift+CapsLock converts the selection
edit.type_keys("^{HOME}^+{END}", pause=0.05)
edit.type_keys("+{CAPSLOCK}", pause=0.05)
time.sleep(1.0)
check("welcome: Shift+CapsLock converts the selection", text(), "สวัสดี")

# restore: CapsLock must be off
import ctypes  # noqa: E402

caps = ctypes.windll.user32.GetKeyState(0x14) & 1
print("CapsLock state after run:", caps)

rt.terminate()
bad = [r for r in results if not r[1]]
print(f"\n{len(results) - len(bad)}/{len(results)} passed")
sys.exit(1 if bad else 0)

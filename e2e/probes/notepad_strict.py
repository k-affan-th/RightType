"""Release-like (injected keys ignored): what still works in a foreign app (Notepad)."""
import sys, tempfile, time, ctypes
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
sys.stdout.reconfigure(encoding="utf-8")
import lib
appdata = Path(tempfile.mkdtemp(prefix="rt-np-"))
(appdata / "RightType").mkdir()
(appdata / "RightType" / "config.toml").write_text('language = "en"\nonboarded = true\n', encoding="utf-8")
lib.E2E_ENV["APPDATA"] = str(appdata)
lib.E2E_ENV.pop("RIGHTTYPE_E2E_ACCEPT_INJECTED", None)
rt = lib.start_righttype()
time.sleep(3)
doc = Path(tempfile.mkdtemp(prefix="rt-doc-")) / "t.txt"
doc.write_text("", encoding="utf-8")
app = lib.start_notepad(doc)
win = app.top_window()
time.sleep(1)
res = []
def check(name, got, want):
    ok = got.strip() == want
    res.append(ok); print(("PASS " if ok else "FAIL ") + name + f"  got={got!r} want={want!r}")
def text():
    return lib.read_text_value(app) or ""
def caps(): return ctypes.windll.user32.GetKeyState(0x14) & 1
win.set_focus()
lib.set_layout(win.handle, "en")
win.type_keys("l;ylfu", pause=0.04)
time.sleep(0.5)
check("typed text sent by another program is NOT fixed (by design)", text(), "l;ylfu")
win.type_keys("^a", pause=0.05)
win.type_keys("+{CAPSLOCK}", pause=0.05)
time.sleep(1.2)
check("Shift+CapsLock converts the selection", text(), "สวัสดี")
before = caps()
win.type_keys("^{CAPSLOCK}", pause=0.05)
time.sleep(0.6)
check("Ctrl+CapsLock does not turn CapsLock on", str(caps()), str(before))
win.type_keys("{END}", pause=0.05)
win.type_keys("+{BACKSPACE}", pause=0.05)
time.sleep(0.8)
check("Shift+Backspace is not a plain Backspace", text(), "สวัสดี")
if caps(): win.type_keys("{CAPSLOCK}", pause=0.05)
print("CapsLock at end:", caps())
rt.terminate()
lib.subprocess.run(["taskkill", "/IM", "notepad.exe", "/F"], capture_output=True)
print(f"{sum(res)}/{len(res)} passed")

"""Dump Word UIA tree (types + values + text pattern)."""

import ctypes
import subprocess
import sys
import time
from pathlib import Path

from pywinauto import Desktop
from pywinauto.application import Application

import lib

sys.stdout.reconfigure(encoding="utf-8")
subprocess.run(["taskkill", "/IM", "WINWORD.EXE", "/F"], capture_output=True)
time.sleep(2)
rt = lib.start_righttype()
time.sleep(2)
subprocess.Popen(
    [r"C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE", "/q"]
)
u = ctypes.windll.user32
app = None
deadline = time.time() + 40
while time.time() < deadline and app is None:
    out = subprocess.run(
        ["tasklist", "/FI", "IMAGENAME eq WINWORD.EXE", "/FO", "CSV", "/NH"],
        capture_output=True,
        text=True,
    ).stdout
    pids = [int(l.split('","')[1]) for l in out.splitlines() if l.startswith('"WINWORD.EXE')]
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

win = app.top_window()
win.set_focus()
time.sleep(0.4)
if "Document" not in win.window_text():
    win.type_keys("^n")
    time.sleep(3)
win.set_focus()
win.type_keys("l;ylfu ")
time.sleep(2)
print("title:", win.window_text())
for d in win.descendants():
    try:
        ct = d.element_info.control_type
    except Exception:
        continue
    if ct in ("Document", "Edit", "Custom", "Pane", "Text"):
        v = ""
        t = ""
        try:
            v = str(d.legacy_properties().get("Value"))[:30]
        except Exception:
            pass
        try:
            t = d.iface_text.DocumentRange.GetText(-1)[:40].replace("\r", "\\r")
        except Exception:
            t = "<-"
        if v or t not in ("", "<-"):
            print(f"{ct:9} val={v!r} txt={t!r} cls={d.element_info.class_name}")
try:
    subprocess.run(["taskkill", "/IM", "WINWORD.EXE", "/F"], capture_output=True)
except Exception:
    pass
rt.terminate()

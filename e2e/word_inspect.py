"""Dump Word's windows right after a clean launch."""

import ctypes
import subprocess
import sys
import time

import lib

sys.stdout.reconfigure(encoding="utf-8")
subprocess.run(["taskkill", "/IM", "WINWORD.EXE", "/F"], capture_output=True)
time.sleep(2)

subprocess.Popen(
    [r"C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE", "/q", "/a", "/n"]
)
time.sleep(12)

out = subprocess.run(
    ["tasklist", "/FI", "IMAGENAME eq WINWORD.EXE", "/FO", "CSV", "/NH"],
    capture_output=True,
    text=True,
).stdout
pids = [int(l.split('","')[1]) for l in out.splitlines() if l.startswith('"WINWORD.EXE')]
print("pids:", pids)
u = ctypes.windll.user32
found = []


@ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
def cb(h, _):
    pid = ctypes.c_uint()
    u.GetWindowThreadProcessId(h, ctypes.byref(pid))
    if pid.value in pids:
        found.append(h)
    return True


u.EnumWindows(cb, None)
for h in found:
    cn = ctypes.create_unicode_buffer(64)
    u.GetClassNameW(h, cn, 64)
    t = ctypes.create_unicode_buffer(128)
    u.GetWindowTextW(h, t, 128)
    print(f"hwnd={hex(h)} class={cn.value!r} title={t.value!r} visible={bool(u.IsWindowVisible(h))}")
subprocess.run(["taskkill", "/IM", "WINWORD.EXE", "/F"], capture_output=True)

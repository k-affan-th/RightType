"""Inspect RightType's top-level windows (class/title/hwnd)."""

import ctypes
import sys
import time

import lib

sys.stdout.reconfigure(encoding="utf-8")
rt = lib.start_righttype()
time.sleep(2.5)

u = ctypes.windll.user32
out = subprocess_out = lib.subprocess.run(
    ["tasklist", "/FI", "IMAGENAME eq righttype.exe", "/FO", "CSV", "/NH"],
    capture_output=True,
    text=True,
).stdout
pids = [int(l.split('","')[1]) for l in out.splitlines() if l.startswith('"righttype.exe')]
print("pids:", pids)
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
    t = ctypes.create_unicode_buffer(64)
    u.GetWindowTextW(h, t, 64)
    print(f"hwnd={hex(h)} class={cn.value!r} title={t.value!r} visible={u.IsWindowVisible(h)}")

rt.terminate()

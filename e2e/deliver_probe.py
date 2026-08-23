"""Deliverability check: post power/WTS/custom messages at the NGW window."""

import ctypes
import sys
import time
from pathlib import Path

import lib

sys.stdout.reconfigure(encoding="utf-8")
LOG = Path(__file__).resolve().parent / "rt_stderr.log"
u = ctypes.windll.user32

rt = lib.start_righttype(stderr_log=LOG)
time.sleep(2.5)

out = lib.subprocess.run(
    ["tasklist", "/FI", "IMAGENAME eq righttype.exe", "/FO", "CSV", "/NH"],
    capture_output=True,
    text=True,
).stdout
pid = int(out.splitlines()[0].split('","')[1])
hwnd = None


@ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
def cb(h, _):
    p = ctypes.c_uint()
    u.GetWindowThreadProcessId(h, ctypes.byref(p))
    if p.value == pid:
        cn = ctypes.create_unicode_buffer(64)
        u.GetClassNameW(h, cn, 64)
        if cn.value == "NativeWindowsGuiWindow":
            nonlocal_local.append(h)

    return True


nonlocal_local = []
u.EnumWindows(cb, None)
hwnd = nonlocal_local[0]
print("posting to", hex(hwnd))

tests = [
    (0x0113, 0x777),  # WM_TIMER with unique id — pure deliverability marker
    (0x0218, 0x0012),  # WM_POWERBROADCAST resume-automatic
    (0x02B1, 0x0008),  # WTS unlock
]
for msg, wp in tests:
    ok = u.SendMessageW(hwnd, msg, wp, 0)
    alive = bool(u.IsWindow(hwnd))
    print(f"send {msg:#06x} w={wp:#x} -> ret={ok} iswin={alive} err={ctypes.get_last_error()}")
    time.sleep(0.3)
time.sleep(1.0)

print("--- trace tail ---")
for line in LOG.read_text(encoding="utf-8", errors="replace").splitlines()[-12:]:
    print(line)
rt.terminate()

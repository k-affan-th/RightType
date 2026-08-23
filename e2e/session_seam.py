"""Session-transition seam probe: drive WM_POWERBROADCAST / WTS messages at the
tray window directly, verify the hook reinstalls and still corrects after."""

import ctypes
import sys
import time
from pathlib import Path

import matrix as m
import lib

sys.stdout.reconfigure(encoding="utf-8")
LOG = Path(__file__).resolve().parent / "rt_stderr.log"

u = ctypes.windll.user32
WM_POWERBROADCAST = 0x0218
WM_WTSSESSION_CHANGE = 0x02B1
PBT_APMRESUMEAUTOMATIC = 0x0012
PBT_APMRESUMESUSPEND = 0x0007
WTS_SESSION_UNLOCK = 0x8


def find_rt_windows() -> list[int]:
    import subprocess

    out = subprocess.run(
        ["tasklist", "/FI", "IMAGENAME eq righttype.exe", "/FO", "CSV", "/NH"],
        capture_output=True,
        text=True,
    ).stdout
    pids = [int(line.split('","')[1]) for line in out.splitlines() if line.startswith('"righttype.exe')]
    found: list[int] = []
    EnumWindows = u.EnumWindows
    GetWindowThreadProcessId = u.GetWindowThreadProcessId

    @ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
    def cb(hwnd, _l):
        pid = ctypes.c_uint()
        GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value in pids:
            found.append(hwnd)
        return True

    EnumWindows(cb, None)
    return found


def post(hwnd: int, msg: int, wparam: int):
    u.PostMessageW(hwnd, msg, wparam, 0)


def main():
    m.set_config_mode("auto")
    rt = lib.start_righttype(stderr_log=LOG)
    time.sleep(2.5)
    hwnds = find_rt_windows()
    print(f"righttype windows: {len(hwnds)}")
    app = None
    httpd = None
    try:
        app, httpd = lib.start_edge(m.HERE / "target.html")

        # baseline correction
        got1 = m.type_live(app, "dy[", "กับ").strip()
        r1 = got1 == "กับ"
        print(f"baseline live      : {got1!r} pass={r1}")

        # simulate resume from sleep (the pair Windows actually sends)
        for h in hwnds:
            post(h, WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC)
            post(h, WM_POWERBROADCAST, PBT_APMRESUSPEND := PBT_APMRESUMESUSPEND)
        time.sleep(2.0)

        got2 = m.type_live(app, "l;ylfu", "สวัสดี").strip()
        r2 = got2 == "สวัสดี"
        print(f"after resume seam  : {got2!r} pass={r2}")

        # simulate unlock
        for h in hwnds:
            post(h, WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK)
        time.sleep(2.0)

        got3 = m.type_live(app, "dy[", "กับ").strip()
        r3 = got3 == "กับ"
        print(f"after unlock seam  : {got3!r} pass={r3}")

        print(f"righttype alive    : {rt.poll() is None}")
        results = [
            {"case": "seam_baseline", "pass": r1},
            {"case": "seam_after_resume", "pass": r2},
            {"case": "seam_after_unlock", "pass": r3},
            {"case": "process_alive", "pass": rt.poll() is None},
        ]
        ok = all(r["pass"] for r in results)
        print("SEAM:", "PASS" if ok else "FAIL")
        raise SystemExit(0 if ok else 1)
    finally:
        if app:
            lib.close_app(app)
        if httpd:
            httpd.shutdown()
        rt.terminate()


if __name__ == "__main__":
    main()

"""UAC resilience: trigger an elevation prompt (auto-dismisses), then prove the
hook still corrects afterwards. The secure desktop itself is not observable."""

import subprocess
import sys
import time
from pathlib import Path

import matrix as m
import lib

sys.stdout.reconfigure(encoding="utf-8")
HERE = Path(__file__).resolve().parent
LOG = HERE / "rt_stderr.log"

m.set_config_mode("auto")
rt = lib.start_righttype(stderr_log=LOG)
time.sleep(2.0)

# Fire a harmless elevation request; nobody consents -> auto-cancels (~120 s max).
elev = subprocess.Popen(
    ["powershell.exe", "-NoProfile", "-Command",
     "Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile','-Command','exit'"],
    creationflags=subprocess.CREATE_NEW_CONSOLE,
)
print("UAC prompt fired; secure desktop should block input briefly...")
time.sleep(15.0)


def wait_desktop(timeout: int = 200) -> bool:
    """The consent screen auto-cancels after its timeout; wait for input to return."""
    from pywinauto import mouse as _m

    t0 = time.time()
    while time.time() - t0 < timeout:
        try:
            _m._set_cursor_pos((5, 5))
            _m._set_cursor_pos((10, 10))
            return True
        except RuntimeError:
            time.sleep(3)
    return False


desktop_back = wait_desktop()
alive_during = rt.poll() is None
subprocess.run(["taskkill", "/PID", str(elev.pid), "/F"], capture_output=True)
time.sleep(1.0)
print(f"desktop restored={desktop_back} righttype alive={alive_during}")

app = None
httpd = None
try:
    app, httpd = lib.start_edge(HERE / "target.html")
    got = m.type_live(app, "dy[", "กับ").strip()
    ok = alive_during and got == "กับ"
    print(f"alive={alive_during} post-UAC correction={got!r}")
    print({"case": "uac_resilience", "pass": ok})
    raise SystemExit(0 if ok else 1)
finally:
    if app:
        lib.close_app(app)
    if httpd:
        httpd.shutdown()
    rt.terminate()

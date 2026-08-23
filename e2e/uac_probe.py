"""UAC resilience v2: baseline -> UAC -> post, with live-eval traces on both."""

import subprocess
import sys
import time
from pathlib import Path

import matrix as m
import lib
from pywinauto import mouse as _mouse

sys.stdout.reconfigure(encoding="utf-8")
HERE = Path(__file__).resolve().parent
LOG = HERE / "rt_stderr.log"


def wait_desktop(timeout: int = 200) -> bool:
    t0 = time.time()
    while time.time() - t0 < timeout:
        try:
            _mouse._set_cursor_pos((5, 5))
            _mouse._set_cursor_pos((10, 10))
            return True
        except RuntimeError:
            time.sleep(3)
    return False


m.set_config_mode("auto")
rt = lib.start_righttype(stderr_log=LOG)
time.sleep(2.5)
app = None
httpd = None
results = {}
try:
    app, httpd = lib.start_edge(HERE / "target.html")

    pre = m.type_live(app, "dy[", "กับ").strip()
    results["pre_uac_correction"] = pre == "กับ"
    print(f"PRE : {pre!r} pass={results['pre_uac_correction']}")

    elev = subprocess.Popen(
        ["powershell.exe", "-NoProfile", "-Command",
         "Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile','-Command','exit'"],
        creationflags=subprocess.CREATE_NEW_CONSOLE,
    )
    print(">>> UAC incoming — please click YES <<<")
    time.sleep(15)
    desktop_back = wait_desktop()
    subprocess.run(["taskkill", "/PID", str(elev.pid), "/F"], capture_output=True)
    time.sleep(1.0)
    results["desktop_restored"] = desktop_back
    results["process_alive"] = rt.poll() is None
    print(f"desktop={desktop_back} alive={results['process_alive']}")

    app2 = None
    try:
        post = m.type_live(app, "l;ylfu", "สวัสดี").strip()
        results["post_uac_correction"] = post == "สวัสดี"
        print(f"POST: {post!r} pass={results['post_uac_correction']}")
    finally:
        if app:
            lib.close_app(app)

    ok = all(results.values())
    print({"results": results, "pass": ok})
    raise SystemExit(0 if ok else 1)
finally:
    if httpd:
        httpd.shutdown()
    rt.terminate()

"""Focused undo probe with trace capture."""

import sys
import time
from pathlib import Path

import matrix as m
import lib

sys.stdout.reconfigure(encoding="utf-8")
LOG = Path(__file__).resolve().parent / "rt_stderr.log"

m.set_config_mode("auto")
rt = lib.start_righttype(stderr_log=LOG)
time.sleep(2.0)
app = None
httpd = None
try:
    app, httpd = lib.start_edge(m.HERE / "target.html")
    m.type_live(app, "dy[", "กับ")
    print("mid:", repr(m.read_out(app)))
    app.top_window().set_focus()
    time.sleep(0.4)
    app.top_window().type_keys("^+{CAPSLOCK}")
    time.sleep(2.0)
    print("after:", repr(m.read_out(app)))
finally:
    if app:
        lib.close_app(app)
    if httpd:
        httpd.shutdown()
    rt.terminate()
    time.sleep(0.3)

print("--- trace tail ---")
for line in LOG.read_text(encoding="utf-8", errors="replace").splitlines()[-14:]:
    print(line)

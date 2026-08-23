"""RightType round-trip against Notepad or Edge, on a dedicated per-run document.

Usage:
  uv run notepad_roundtrip.py --text "l;ylfu" --lang en --expect "สวัสดี"
  uv run notepad_roundtrip.py --app edge --text "l;ylfu" --lang en --expect "สวัสดี"
"""

import argparse
import json
import os
import sys
import tempfile
import time
from pathlib import Path

from lib import (
    close_app,
    read_text_value,
    set_layout,
    start_edge,
    start_notepad,
    start_righttype,
    type_text,
)

sys.stdout.reconfigure(encoding="utf-8")
HERE = Path(__file__).resolve().parent

p = argparse.ArgumentParser()
p.add_argument("--app", choices=["notepad", "edge"], default="notepad")
p.add_argument("--text", required=True)
p.add_argument("--lang", choices=["en", "th"], default="en")
p.add_argument("--expect", required=True)
args = p.parse_args()

rt = start_righttype()
time.sleep(2.0)
app = None
httpd = None
result = {"typed": args.text, "expected": args.expect, "actual": "<no run>", "pass": False}
tmp_files = []
try:
    if args.app == "edge":
        app, httpd = start_edge(HERE / "edge_target.html")
    else:
        doc = Path(tempfile.gettempdir()) / f"rt_e2e_{os.getpid()}_{int(time.time())}.txt"
        doc.write_text("", encoding="utf-8")
        tmp_files.append(doc)
        app = start_notepad(doc=doc)

    win = app.top_window()
    set_layout(win.handle, args.lang)
    win.set_focus()
    time.sleep(1.0)
    type_text(win, args.text + " ")
    time.sleep(3.0)
    actual = read_text_value(app, largest=(args.app == "edge"))
    result = {
        "typed": args.text,
        "expected": args.expect + " ",
        "actual": actual,
        "pass": actual.strip() == args.expect,
    }
finally:
    if app:
        close_app(app)
    if httpd:
        httpd.shutdown()
    rt.terminate()
    time.sleep(0.3)
    for f in tmp_files:
        try:
            f.unlink(missing_ok=True)
        except Exception:
            pass

print(json.dumps(result, ensure_ascii=False))
raise SystemExit(0 if result["pass"] else 1)

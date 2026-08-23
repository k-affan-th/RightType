"""Interactive transition probe: baseline -> user locks/unlocks (+optional sleep)
-> watch session traces -> post-corrections."""

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
time.sleep(2.5)
app = None
httpd = None
results = {}
try:
    app, httpd = lib.start_edge(HERE / "target.html")

    pre = m.type_live(app, "dy[", "กับ").strip()
    results["baseline"] = pre == "กับ"
    print(f"BASELINE: {pre!r} pass={results['baseline']}")

    mark = len(m.trace_lines())
    print(">>> LOCK NOW: Win+L then log back in (optionally sleep+wake). 5 min window <<<")
    sys.stdout.flush()

    deadline = time.time() + 300
    reinstall_events = []
    while time.time() < deadline:
        new = [l for l in m.trace_lines()[mark:] if "session:" in l]
        for l in new:
            if l not in reinstall_events:
                reinstall_events.append(l)
        if any("unlock" in e or "power resume" in e for e in reinstall_events):
            break
        time.sleep(2)

    results["reinstall_evidence"] = bool(reinstall_events)
    print("events:", reinstall_events if reinstall_events else "NONE SEEN")

    got1 = m.type_live(app, "l;ylfu", "สวัสดี").strip()
    results["post_enth"] = got1 == "สวัสดี"
    got2 = m.type_token(app, "แนพพำแะ", "th")
    got2v = m.read_out(app).strip()
    results["post_then"] = got2v == "correct"

    print(f"POST live={got1!r} boundary={got2v!r}")
    results["process_alive"] = rt.poll() is None
    ok = all(results.values())
    print({"results": results, "pass": ok})
    raise SystemExit(0 if ok else 1)
finally:
    if app:
        lib.close_app(app)
    if httpd:
        httpd.shutdown()
    rt.terminate()

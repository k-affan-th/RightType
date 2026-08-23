"""Inspect Chrome target controls: Value pattern vs Text pattern."""

import sys
import time

import lib
import matrix as mx

click_element = mx.click_element
set_layout = lib.set_layout

sys.stdout.reconfigure(encoding="utf-8")
rt = lib.start_righttype()
time.sleep(2)
app = None
httpd = None
try:
    app, httpd = lib.start_edge(lib.HERE / "target.html", exe=lib.CHROME_EXE)
    win = app.top_window()
    set_layout(win.handle, "en")
    win.set_focus()
    time.sleep(0.4)
    click_element(win, "Edit", "largest")
    win.type_keys("l;ylfu ", pause=0.04)
    time.sleep(2.0)
    for d in app.top_window().descendants():
        try:
            ct = d.element_info.control_type
        except Exception:
            continue
        if ct not in ("Document", "Edit"):
            continue
        try:
            v = d.legacy_properties().get("Value")
        except Exception:
            v = None
        t = ""
        try:
            t = d.iface_text.DocumentRange.GetText(-1)[:40]
        except Exception as e:
            t = f"<no textpattern {type(e).__name__}>"
        r = d.rectangle()
        print(f"{ct:9} area={max(0,r.width())*max(0,r.height()):8} value={str(v)[:30]!r} text={t!r}")
finally:
    if app:
        lib.close_app(app)
    if httpd:
        httpd.shutdown()
    rt.terminate()

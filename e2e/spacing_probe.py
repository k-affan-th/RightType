"""Space-semantics probe: mixed EN/TH sequences — is any space eaten?

A) hello ␣ + l;ylfu(live)      -> expected 'hello สวัสดี'
B) l;ylfu(live->switch TH) ␣?  -> 'สวัสดี' then 'ะ้ำ'+␣ (TH->EN) -> 'สวัสดี the'
C) hello ␣ + l;ylfu + ␣        -> 'hello สวัสดี ' (trailing kept)
"""

import sys
import time

import matrix as mx
import lib

sys.stdout.reconfigure(encoding="utf-8")
HERE = lib.HERE


def fresh(app, lang="en"):
    win = app.top_window()
    win.set_focus()
    time.sleep(0.2)
    win.type_keys("{F5}")
    time.sleep(1.5)
    lib.set_layout(win.handle, lang)
    mx.click_element(win, "Edit", "largest")
    time.sleep(0.3)


def chars(app, s, pause=0.05):
    win = app.top_window()
    win.type_keys(s, with_spaces=True, pause=pause)
    time.sleep(0.6)


def space(app):
    app.top_window().type_keys("{SPACE}", pause=0.02)
    time.sleep(0.4)


def read():
    time.sleep(1.8)
    return lib.read_browser_text(app)


mx.set_config_mode("auto")
rt = lib.start_righttype(stderr_log=lib.HERE / "rt_stderr.log")
time.sleep(2.5)
app = None
httpd = None
try:
    app, httpd = lib.start_edge(HERE / "target.html")

    # A) english word, space, then thai-intended live commit
    fresh(app)
    chars(app, "hello")
    space(app)
    chars(app, "l;ylfu")
    got_a = read()
    print(f"A en␣thai   : {got_a!r}  want='hello สวัสดี'")

    # B) thai live first (switches layout), then TH->EN word via boundary
    fresh(app)
    chars(app, "l;ylfu")
    got_b1 = read()
    space(app)
    chars(app, "ะ้ำ")
    space(app)
    got_b = read()
    print(f"B thai␣the  : {got_b!r}  want~='สวัสดี the' (intermediate={got_b1!r})")

    # C) english, space, thai live, trailing space
    fresh(app)
    chars(app, "hello")
    space(app)
    chars(app, "l;ylfu")
    space(app)
    got_c = read()
    print(f"C en␣thai␣  : {got_c!r}  want='hello สวัสดี '")

    def chk(name, got, want):
        print(("PASS " if got == want else "FAIL "), name, f"got={got!r} want={want!r}")

    chk("A", got_a, "hello สวัสดี")
    chk("B", got_b.replace(" ", ""), "สวัสดีthe")
    chk("C", got_c.rstrip() == "hello สวัสดี" and got_c != got_a, True)
finally:
    if app:
        lib.close_app(app)
    if httpd:
        httpd.shutdown()
    rt.terminate()

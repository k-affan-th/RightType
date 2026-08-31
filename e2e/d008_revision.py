"""D-008 E2E: revisable rendering in a real app.

What only a real run can prove: that `reconcile_run` moves live text in a real
edit control with real backspaces, that a withdrawn reading leaves the raw
keystrokes behind, and that a word boundary arriving while RightType owns the
run does not make the boundary path correct the same text a second time.

Two deliberate differences from the rest of the harness, because an E2E run must
not damage the machine it runs on:

- Edge is launched additively. `lib.start_edge` force-kills `msedge.exe`, which
  would take the user's open tabs with it.
- `config.toml` is the user's real settings (learned words, blocked apps,
  onboarding). Only the mode line is touched, and the original is always
  restored.

Usage: uv run d008_revision.py
"""

import functools
import http.server
import re
import socketserver
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

from pywinauto import Application, Desktop, mouse

import lib

sys.stdout.reconfigure(encoding="utf-8")
HERE = Path(__file__).resolve().parent
LOG = HERE / "rt_d008.log"
APPDATA_CONFIG = Path(lib.os.environ["APPDATA"]) / "RightType" / "config.toml"


def force_auto_mode():
    """Switch the live config to Auto; return the original text for restoring."""
    APPDATA_CONFIG.parent.mkdir(parents=True, exist_ok=True)
    original = APPDATA_CONFIG.read_text(encoding="utf-8") if APPDATA_CONFIG.exists() else None
    text = original if original is not None else 'enabled = true\n'
    if re.search(r'(?m)^mode\s*=', text):
        text = re.sub(r'(?m)^mode\s*=\s*"[a-z]+"', 'mode = "auto"', text)
    else:
        text = 'mode = "auto"\n' + text
    if not re.search(r'(?m)^enabled\s*=', text):
        text = 'enabled = true\n' + text
    APPDATA_CONFIG.write_text(text, encoding="utf-8")
    return original


def restore_config(original):
    if original is None:
        APPDATA_CONFIG.unlink(missing_ok=True)
    else:
        APPDATA_CONFIG.write_text(original, encoding="utf-8")


TITLE_MARK = "rt-e2e"


def find_target_window():
    """Our window, identified by the page title rather than by window diffing.

    Diffing "which Chrome_WidgetWin_1 windows are new" is not reliable on a real
    desktop: VS Code shares that class, browsers open extra windows, and a window
    left over from a previous run makes the next run read the wrong document.
    """
    for w in Desktop(backend="uia").windows(top_level_only=True):
        try:
            if w.class_name() == "Chrome_WidgetWin_1" and TITLE_MARK in w.window_text():
                return w
        except Exception:
            pass
    return None


def start_edge_additive(html: Path):
    """`lib.start_edge` minus the taskkill: leaves existing browsers alone."""
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(HERE))
    httpd = socketserver.TCPServer(("127.0.0.1", 0), handler)
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{port}/{html.name}"

    proc = subprocess.Popen(
        [
            str(lib.EDGE_EXE),
            "--new-window",
            url,
            f"--user-data-dir={tempfile.gettempdir()}/rt_d008_{int(time.time())}",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-sync",
            "--window-size=900,600",
        ]
    )
    deadline = time.time() + 30
    app = None
    while time.time() < deadline:
        w = find_target_window()
        if w is not None:
            app = Application(backend="uia").connect(handle=w.handle, timeout=5)
            break
        time.sleep(0.5)
    if app is None:
        httpd.shutdown()
        raise SystemExit(f"no Edge window titled {TITLE_MARK!r} after launch")
    win = app.top_window()
    win.wait("visible ready", timeout=20)
    time.sleep(3.0)
    r = win.rectangle()
    mouse.click(button="left", coords=((r.left + r.right) // 2, (r.top + r.bottom) // 2))
    return app, httpd, proc


def kill_edge(proc):
    """Close only the browser this run started, by process tree."""
    subprocess.run(
        ["taskkill", "/F", "/T", "/PID", str(proc.pid)], capture_output=True
    )


def read_textarea(win) -> str:
    """Read the page's textarea from the window we identified by title.

    `lib.read_browser_text` walks `app.top_window()`, which on a multi-window
    browser process can resolve to a different window between calls and then
    reports an empty document for text that is plainly on screen.
    """
    import re

    best = ""
    best_len = -1
    for d in win.descendants():
        try:
            if d.element_info.control_type not in ("Document", "Edit", "EditText"):
                continue
            v = d.legacy_properties().get("Value")
        except Exception:
            continue
        if not isinstance(v, str):
            continue
        if re.match(r"^(https?:)?//|^127\.0\.0\.1|edge://|chrome://", v):
            continue
        if len(v) > best_len:
            best_len = len(v)
            best = v
    return best


def clear_field(win, app) -> bool:
    """Empty the textarea and prove it is empty.

    `lib.type_text` clears with ^a{DEL} and trusts it. On a busy desktop that
    silently no-ops often enough to turn a run into nonsense (each case then
    appends to the previous one), so evidence from this script is only worth
    anything if the clear is verified.
    """
    for _ in range(4):
        win.set_focus()
        time.sleep(0.25)
        win.type_keys("^a{DEL}", pause=0.05)
        time.sleep(0.35)
        if read_textarea(win) == "":
            return True
    return False


def type_keys_only(win, keys: str):
    """Type without clearing; boundary characters must be real virtual keys."""
    win.set_focus()
    if keys.endswith(" "):
        win.type_keys(keys[:-1], with_spaces=True, pause=0.04)
        time.sleep(0.2)
        win.type_keys("{SPACE}", pause=0.03)
    else:
        win.type_keys(keys, with_spaces=True, pause=0.04)


def describe_ui(win) -> str:
    """Everything the round-trip check could have been looking at."""
    rows = []
    try:
        r = win.rectangle()
        rows.append(f"window {win.window_text()!r} rect={r}")
    except Exception as exc:
        rows.append(f"window rect unavailable: {exc}")
    try:
        for d in win.descendants():
            try:
                ct = d.element_info.control_type
                if ct not in ("Document", "Edit", "EditText"):
                    continue
                v = d.legacy_properties().get("Value")
                rows.append(f"  {ct} name={d.window_text()!r} value={v!r}")
            except Exception:
                continue
    except Exception as exc:
        rows.append(f"descendants unavailable: {exc}")
    return "\n".join(rows)


def wait_until_round_trip(win, app, timeout: float = 25.0) -> bool:
    """Prove the harness can type into the page and read it back.

    The UIA tree for a freshly loaded page is not immediately readable, so the
    first case or two used to be measured against an empty read and reported as
    a product failure. A single character is below MIN_LIVE_COMMIT_CHARS, so
    RightType never touches this probe.
    """
    # Focus once. Calling set_focus every iteration can block for a long time
    # when the shell is busy, which turns a failed probe into a hung run.
    try:
        win.set_focus()
        time.sleep(0.5)
    except Exception as exc:
        print(f"warm-up: set_focus failed ({exc}); falling back to a click")
    try:
        r = win.rectangle()
        mouse.click(
            button="left", coords=((r.left + r.right) // 2, (r.top + r.bottom) // 2)
        )
        time.sleep(0.3)
    except Exception as exc:
        print(f"warm-up: click failed ({exc})")

    last = "<never read>"
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            win.type_keys("^a{DEL}", pause=0.05)
            win.type_keys("x", pause=0.05)
            time.sleep(0.4)
            last = read_textarea(win)
            if last == "x":
                win.type_keys("^a{DEL}", pause=0.05)
                time.sleep(0.3)
                last = read_textarea(win)
                if last == "":
                    return True
        except Exception as exc:
            last = f"<exception: {exc}>"
        time.sleep(0.6)

    print(f"warm-up: last read was {last!r}")
    print(describe_ui(win))
    return False


def trace():
    if not LOG.exists():
        return []
    return LOG.read_text(encoding="utf-8", errors="replace").splitlines()


# (name, keystrokes, expected on-screen text, must the reconciler have run)
CASES = [
    ("english-untouched", "different", "different", False),
    ("ambiguous-held", "wri", "wri", False),
    ("typo-revised-back", "adavnce", "adavnce", True),
    ("thai-rendered-live", "l;ylfu", "สวัสดี", True),
    ("boundary-while-owned", "l;ylfu ", "สวัสดี ", True),
]


def main():
    original_config = force_auto_mode()
    rt = lib.start_righttype(stderr_log=LOG)
    if find_target_window() is not None:
        raise SystemExit(
            f"a window titled {TITLE_MARK!r} is already open — close it first so "
            "this run cannot read a stale document"
        )
    app, httpd, edge = start_edge_additive(HERE / "target.html")
    win = app.top_window()
    lib.set_layout(win.handle, "en")

    if not wait_until_round_trip(win, app):
        kill_edge(edge)
        httpd.shutdown()
        rt.terminate()
        restore_config(original_config)
        raise SystemExit("harness could not round-trip text through the page")

    results = []
    try:
        for name, keys, expected, needs_reconcile in CASES:
            lib.set_layout(win.handle, "en")
            if not clear_field(win, app):
                results.append((name, False))
                print(f"SKIP  {name:<22} could not clear the field — no evidence")
                continue
            before = len(trace())
            type_keys_only(win, keys)
            time.sleep(0.9)
            got = read_textarea(win)
            lines = trace()[before:]
            reconciled = any("reconcile" in ln for ln in lines)
            ok = got == expected and (reconciled or not needs_reconcile)
            results.append((name, ok))
            print(
                f"{'PASS' if ok else 'FAIL'}  {name:<22} typed {keys!r:<12} "
                f"expected {expected!r:<14} got {got!r} reconciled={reconciled}"
            )
            if not ok:
                for ln in lines[-14:]:
                    print("      trace:", ln)
    finally:
        lib.screenshot("d008")
        kill_edge(edge)
        httpd.shutdown()
        rt.terminate()
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
        restore_config(original_config)

    passed = sum(1 for _, ok in results if ok)
    print(f"\nD-008 revision matrix: {passed}/{len(results)}")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())

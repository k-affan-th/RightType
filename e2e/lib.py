"""Shared helpers: launch RightType/Notepad/Edge, switch layout, read text, screenshot.

Hard-won constraints for anyone extending this harness:

- RightType ignores injected keys unless a DEBUG build runs with
  RIGHTTYPE_E2E_ACCEPT_INJECTED set (release hard-codes it off by design).
- pywinauto type_keys sends characters as VK_PACKET unicode events; the hook
  classifies those as Char even for spaces, so token boundaries must be sent
  as a real virtual key via "{SPACE}".
- Win11 Notepad (WinUI box) intermittently drops non-ASCII KEYEVENTF_UNICODE
  bursts from ANY sender (~40-60% here); Edge/Chromium is lossless (16/16).
  Use --app edge for automated runs; treat Notepad results as manual-only.
- Each run targets a dedicated temp document so Notepad session-restore cannot
  leak previous-run content into assertions.
"""

import os
import subprocess
import tempfile
import time
from datetime import datetime
from pathlib import Path

import mss
from pywinauto import Desktop
from pywinauto.application import Application

REPO = Path(__file__).resolve().parents[1]
EXE = REPO / "target" / "debug" / "righttype.exe"
RELEASE_EXE = REPO / "target" / "release" / "righttype.exe"
SHOTS = Path(__file__).resolve().parent / "shots"

E2E_ENV = {**os.environ, "RIGHTTYPE_E2E_ACCEPT_INJECTED": "1"}

WM_INPUTLANGCHANGEREQUEST = 0x0050
HKL_EN_US = 0x04090409
HKL_TH_KEDMANEE = 0x041E041E

LAYOUTS = {"en": HKL_EN_US, "th": HKL_TH_KEDMANEE}


def start_righttype(stderr_log=None):
    if not EXE.exists():
        raise SystemExit(f"missing {EXE} — build with: cargo build --features winos")
    subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
    time.sleep(0.8)
    kw = {"cwd": str(REPO), "env": E2E_ENV}
    if stderr_log:
        kw["stderr"] = open(stderr_log, "w", encoding="utf-8")
    return subprocess.Popen([str(EXE)], **kw)


def start_notepad(doc=None):
    subprocess.run(["taskkill", "/IM", "notepad.exe", "/F"], capture_output=True)
    time.sleep(1)

    def notepad_windows():
        out = {}
        for w in Desktop(backend="uia").windows(top_level_only=True):
            try:
                if w.class_name() == "Notepad":
                    out[w.handle] = w
            except Exception:
                pass
        return out

    before = set(notepad_windows())
    cmd = ["notepad.exe"] + ([str(doc)] if doc else [])
    subprocess.Popen(cmd)
    deadline = time.time() + 15
    while time.time() < deadline:
        fresh = [h for h in notepad_windows() if h not in before]
        if len(fresh) == 1:
            return Application(backend="uia").connect(handle=fresh[0], timeout=5)
        time.sleep(0.5)
    raise SystemExit("new Notepad window not found after launch")


def set_layout(hwnd, lang: str):
    import ctypes

    hkl = LAYOUTS[lang]
    ctypes.windll.user32.PostMessageW(int(hwnd), WM_INPUTLANGCHANGEREQUEST, 0, hkl)
    time.sleep(0.4)


def type_text(window, text: str):
    window.set_focus()
    window.type_keys("^a{DEL}", pause=0.02)
    if text.endswith(" "):
        window.type_keys(text[:-1], with_spaces=True, pause=0.03)
        time.sleep(0.15)
        window.type_keys("{SPACE}", pause=0.02)
    else:
        window.type_keys(text, with_spaces=True, pause=0.03)


def read_text_value(app, largest: bool = False) -> str:
    best_val = ""
    best_score = -1
    for d in app.top_window().descendants():
        try:
            ct = d.element_info.control_type
            if ct not in ("Document", "Edit", "EditText"):
                continue
            v = d.legacy_properties().get("Value")
        except Exception:
            continue
        if not isinstance(v, str):
            continue
        if largest:
            try:
                r = d.rectangle()
                score = max(0, r.width()) * max(0, r.height())
            except Exception:
                continue
        else:
            score = len(v)
        if v and score > best_score:
            best_score = score
            best_val = v
    return best_val


def screenshot(name: str) -> Path:
    SHOTS.mkdir(exist_ok=True)
    path = SHOTS / f"{datetime.now():%H%M%S}_{name}.png"
    with mss.mss() as s:
        s.shot(mon=-1, output=str(path))
    return path


EDGE_EXE = Path(os.environ.get("PROGRAMFILES(X86)", "")) / "Microsoft" / "Edge" / "Application" / "msedge.exe"
HERE = Path(__file__).resolve().parent


def start_edge(html: Path):
    import functools
    import http.server
    import socketserver
    import threading

    subprocess.run(["taskkill", "/IM", "msedge.exe", "/F"], capture_output=True)
    time.sleep(1)

    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(HERE))
    httpd = socketserver.TCPServer(("127.0.0.1", 0), handler)
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{port}/{html.name}"

    def browser_windows():
        out = {}
        for w in Desktop(backend="uia").windows(top_level_only=True):
            try:
                if w.class_name() == "Chrome_WidgetWin_1":
                    out[w.handle] = w
            except Exception:
                pass
        return out

    before = set(browser_windows())
    subprocess.Popen(
        [
            str(EDGE_EXE),
            "--new-window",
            url,
            f"--user-data-dir={tempfile.gettempdir()}/rt_e2e_edge_{int(time.time())}",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-sync",
            "--window-size=900,600",
        ]
    )
    deadline = time.time() + 25
    while time.time() < deadline:
        fresh = [h for h in browser_windows() if h not in before]
        if len(fresh) == 1:
            app = Application(backend="uia").connect(handle=fresh[0], timeout=5)
            break
        time.sleep(0.5)
    else:
        httpd.shutdown()
        raise SystemExit("new Edge window not found after launch")

    from pywinauto import mouse

    win = app.top_window()
    win.wait("visible ready", timeout=15)
    time.sleep(3.0)
    r = win.rectangle()
    mouse.click(button="left", coords=((r.left + r.right) // 2, (r.top + r.bottom) // 2))
    return app, httpd


def close_app(app):
    try:
        win = app.top_window()
        win.set_focus()
        win.type_keys("^a{DEL}", pause=0.02)
    except Exception:
        pass
    try:
        app.top_window().close()
    except Exception:
        pass
    try:
        app.kill()
    except Exception:
        pass


def list_windows():
    return [w.window_text() for w in Desktop(backend="uia").windows()]

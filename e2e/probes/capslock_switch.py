"""CapsLock as the language key (caps_mode = "tap"): tap, hold, rollover,
the Caps hotkeys, a password field, a blocked app and a focus change.

Drives the REAL desktop with real key events (keybd_event, so down/up timing
is exact). Run from e2e/ right after `cargo build --features winos`:

    uv run python probes/capslock_switch.py

Safety: refuses to start while Notepad or RightType is already running (it
must not touch windows it did not start); checks that its own window has
focus before every key it sends and aborts otherwise; on any exit it lets go
of held keys, puts CapsLock back as it found it and closes only the windows
it started. Cases it cannot automate reliably print MANUAL.
"""
import ctypes
import os
import subprocess
import sys
import tempfile
import time
from ctypes import wintypes
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
sys.stdout.reconfigure(encoding="utf-8")
import lib  # noqa: E402
from pywinauto import Desktop  # noqa: E402
from pywinauto.application import Application  # noqa: E402

user32 = ctypes.WinDLL("user32", use_last_error=True)
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowThreadProcessId.restype = wintypes.DWORD
user32.GetKeyboardLayout.argtypes = [wintypes.DWORD]
user32.GetKeyboardLayout.restype = ctypes.c_void_p
user32.GetKeyState.argtypes = [ctypes.c_int]
user32.GetKeyState.restype = ctypes.c_short
user32.GetAsyncKeyState.argtypes = [ctypes.c_int]
user32.GetAsyncKeyState.restype = ctypes.c_short
user32.MapVirtualKeyW.argtypes = [wintypes.UINT, wintypes.UINT]
user32.MapVirtualKeyW.restype = wintypes.UINT
user32.keybd_event.argtypes = [wintypes.BYTE, wintypes.BYTE, wintypes.DWORD, ctypes.c_size_t]
user32.GetAncestor.argtypes = [wintypes.HWND, wintypes.UINT]
user32.GetAncestor.restype = wintypes.HWND
user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]

VK_CAPITAL = 0x14
VK_CONTROL = 0x11
VK_MENU = 0x12
VK_TAB = 0x09
VK_DELETE = 0x2E
VK_K = 0x4B
VK_A = 0x41
KEYEVENTF_KEYUP = 0x0002
LANG_EN = 0x0409
LANG_TH = 0x041E
THAI_K = "า"  # Kedmanee `k` types า
THAI_SHIFT_K = "ษ"  # and Shift+K ษ
VK_SHIFT = 0x10

HOLD_MS = 500


class Abort(Exception):
    pass


# ---------------------------------------------------------------- guards

def running(image):
    out = subprocess.run(
        ["tasklist", "/FI", f"IMAGENAME eq {image}", "/NH"], capture_output=True, text=True
    ).stdout
    return image.lower() in out.lower()


for image in ("notepad.exe", "righttype.exe"):
    if running(image):
        raise SystemExit(f"{image} is already running: close it first (this probe only "
                         "touches windows it starts)")

ORIGINAL_CAPS = user32.GetKeyState(VK_CAPITAL) & 1
HELD = set()
OWN_WINDOWS = {}  # name -> hwnd (top level)


def _global_caps():
    # This thread has no message queue, so GetKeyState's toggle bit follows
    # the desktop's CapsLock state (checked by toggling it).
    return user32.GetKeyState(VK_CAPITAL) & 1


def foreground_root():
    fg = user32.GetForegroundWindow()
    if not fg:
        return None
    return user32.GetAncestor(fg, 2) or fg  # GA_ROOT


def require_focus(name):
    want = OWN_WINDOWS[name]
    got = foreground_root()
    if got != want:
        raise Abort(f"focus lost: expected {name} ({want:#x}), foreground is "
                    f"{(got or 0):#x}")


def key(vk, up, focus=None):
    if focus:
        require_focus(focus)
    scan = user32.MapVirtualKeyW(vk, 0)
    user32.keybd_event(vk, scan, KEYEVENTF_KEYUP if up else 0, 0)
    if up:
        HELD.discard(vk)
    else:
        HELD.add(vk)


def tap(vk, focus, hold_ms=40):
    key(vk, False, focus)
    time.sleep(hold_ms / 1000)
    key(vk, True, focus)


def release_all():
    for vk in list(HELD):
        scan = user32.MapVirtualKeyW(vk, 0)
        user32.keybd_event(vk, scan, KEYEVENTF_KEYUP, 0)
        HELD.discard(vk)


def layout_of(hwnd):
    tid = user32.GetWindowThreadProcessId(hwnd, None)
    return (user32.GetKeyboardLayout(tid) or 0) & 0xFFFF


# ---------------------------------------------------------------- setup

appdata = Path(tempfile.mkdtemp(prefix="rt-caps-"))
(appdata / "RightType").mkdir()
CONFIG = appdata / "RightType" / "config.toml"
BASE_CONFIG = (
    'language = "en"\n'
    "onboarded = true\n"
    'mode = "manual"\n'
    "caret_hints = true\n"
    "shortcuts_in_english = false\n"
    "hold_for_accents = false\n"
    "ctrl_hold_shortcuts = false\n"
    "caret_list = false\n"
    "guard_switch = false\n"
    "restart_after_crash = false\n"
    f"caps_hold_ms = {HOLD_MS}\n"
)
lib.E2E_ENV["APPDATA"] = str(appdata)
LOG = appdata / "righttype-stderr.log"
doc = appdata / "caps-probe.txt"
doc.write_text("", encoding="utf-8")

rt = None
edge_app = None
edge_httpd = None
notepad_pid = None
results = []


def start_rt(extra=""):
    global rt
    stop_rt()
    CONFIG.write_text(BASE_CONFIG + extra, encoding="utf-8")
    if not lib.EXE.exists():
        raise SystemExit(f"missing {lib.EXE}: cargo build --features winos")
    rt = subprocess.Popen([str(lib.EXE)], cwd=str(lib.REPO), env=lib.E2E_ENV,
                          stderr=open(LOG, "a", encoding="utf-8"))
    time.sleep(1.5)
    if rt.poll() is not None:
        raise SystemExit("righttype.exe exited at once: target/debug holds the core-only "
                         "build. Rebuild with: cargo build --features winos")
    time.sleep(2.5)


def stop_rt():
    global rt
    if rt is not None and rt.poll() is None:
        rt.terminate()
        try:
            rt.wait(5)
        except Exception:
            rt.kill()
    rt = None


def notepad_windows():
    out = {}
    for w in Desktop(backend="uia").windows(top_level_only=True):
        try:
            if w.class_name() == "Notepad":
                out[w.handle] = w
        except Exception:
            pass
    return out


def start_notepad():
    global notepad_pid
    before = set(notepad_windows())
    subprocess.Popen(["notepad.exe", str(doc)])
    deadline = time.time() + 20
    while time.time() < deadline:
        fresh = [h for h in notepad_windows() if h not in before]
        if len(fresh) == 1:
            pid = wintypes.DWORD()
            user32.GetWindowThreadProcessId(fresh[0], ctypes.byref(pid))
            notepad_pid = pid.value
            OWN_WINDOWS["notepad"] = fresh[0]
            return Application(backend="uia").connect(handle=fresh[0], timeout=5)
        time.sleep(0.5)
    raise Abort("new Notepad window not found")


def window_class(hwnd):
    buf = ctypes.create_unicode_buffer(128)
    user32.GetClassNameW(wintypes.HWND(hwnd), buf, 128)
    return buf.value


def focus(name, app, settle=0.4):
    want = OWN_WINDOWS[name]
    for _ in range(4):
        try:
            app.window(handle=want).set_focus()
        except Exception:
            pass
        time.sleep(settle)
        if foreground_root() == want:
            return
    got = foreground_root() or 0
    raise Abort(f"could not bring {name} to the front (foreground is {got:#x}, "
                f"class {window_class(got)!r})")


def check(name, got, want):
    ok = got == want
    results.append((name, "PASS" if ok else "FAIL"))
    print(("PASS " if ok else "FAIL ") + name + f"  got={got!r} want={want!r}", flush=True)


def manual(name, why):
    results.append((name, "MANUAL"))
    print(f"MANUAL {name}  ({why})", flush=True)


def lang_name(lang):
    return {LANG_EN: "en", LANG_TH: "th"}.get(lang, hex(lang))


def notepad_text(app):
    return lib.read_text_value(app)


def clear_notepad(app):
    focus("notepad", app)
    key(VK_CONTROL, False, "notepad")
    tap(VK_A, "notepad", 30)
    key(VK_CONTROL, True, "notepad")
    tap(VK_DELETE, "notepad", 30)
    time.sleep(0.3)


def set_lang(name, lang):
    lib.set_layout(OWN_WINDOWS[name], lang)
    time.sleep(0.3)


def read_mode():
    for line in CONFIG.read_text(encoding="utf-8").splitlines():
        if line.startswith("mode"):
            return line.split("=", 1)[1].strip().strip('"')
    return None


def read_enabled():
    for line in CONFIG.read_text(encoding="utf-8").splitlines():
        if line.startswith("enabled"):
            return line.split("=", 1)[1].strip()
    return None


def restore_caps():
    if _global_caps() != ORIGINAL_CAPS:
        # RightType is stopped by now, so this is a plain CapsLock press.
        scan = user32.MapVirtualKeyW(VK_CAPITAL, 0)
        user32.keybd_event(VK_CAPITAL, scan, 0, 0)
        user32.keybd_event(VK_CAPITAL, scan, KEYEVENTF_KEYUP, 0)
        time.sleep(0.2)


def put_caps_back_now(focus_name):
    """Undo a CapsLock toggle a case made, with RightType running: a hold
    toggles it again in tap mode. Only called while our window is in front."""
    if _global_caps() != ORIGINAL_CAPS:
        key(VK_CAPITAL, False, focus_name)
        time.sleep(0.75)
        key(VK_CAPITAL, True, focus_name)
        time.sleep(0.3)


# ---------------------------------------------------------------- cases

def run_cases():
    global edge_app, edge_httpd
    np = start_notepad()
    hwnd = OWN_WINDOWS["notepad"]
    time.sleep(1.5)
    start_rt('caps_mode = "tap"\n')

    # 1. tap switches
    clear_notepad(np)
    set_lang("notepad", "en")
    caps0 = _global_caps()
    tap(VK_CAPITAL, "notepad", 80)
    time.sleep(0.5)
    check("tap switches en->th", lang_name(layout_of(hwnd)), "th")
    tap(VK_CAPITAL, "notepad", 80)
    time.sleep(0.5)
    check("tap switches th->en", lang_name(layout_of(hwnd)), "en")
    check("tap leaves CapsLock alone", _global_caps(), caps0)

    # 2. hold toggles CapsLock once, layout unchanged
    set_lang("notepad", "en")
    caps0 = _global_caps()
    key(VK_CAPITAL, False, "notepad")
    time.sleep(0.7)
    key(VK_CAPITAL, True, "notepad")
    time.sleep(0.4)
    check("hold toggles CapsLock once", _global_caps(), 1 - caps0)
    check("hold keeps the layout", lang_name(layout_of(hwnd)), "en")
    put_caps_back_now("notepad")
    check("second hold puts CapsLock back", _global_caps(), ORIGINAL_CAPS)

    # 3. rollover: Caps down, k at 60 ms, Caps up -> one Thai า, layout Thai
    for run in range(3):
        clear_notepad(np)
        set_lang("notepad", "en")
        key(VK_CAPITAL, False, "notepad")
        time.sleep(0.06)
        key(VK_K, False, "notepad")
        time.sleep(0.03)
        key(VK_K, True, "notepad")
        time.sleep(0.03)
        key(VK_CAPITAL, True, "notepad")
        time.sleep(0.6)
        check(f"rollover letter is in new layout (run {run + 1})", notepad_text(np).strip(),
              THAI_K)
        check(f"rollover leaves Thai on (run {run + 1})", lang_name(layout_of(hwnd)), "th")
    check("rollover leaves CapsLock alone", _global_caps(), ORIGINAL_CAPS)

    # 3b. rollover with Shift: the key keeps the Shift it was pressed with,
    # even when Shift is let go (or the next key's Shift comes) before the
    # key is typed again.
    for label, shift_first, shift_up_early in (
        ("rollover with Shift held", True, False),
        ("rollover with Shift let go at once", True, True),
        ("rollover, then Shift for the next key", False, False),
    ):
        clear_notepad(np)
        set_lang("notepad", "en")
        key(VK_CAPITAL, False, "notepad")
        time.sleep(0.05)
        if shift_first:
            key(VK_SHIFT, False, "notepad")
            time.sleep(0.01)
        key(VK_K, False, "notepad")
        # Within the replay delay, before the key comes out: Shift let go,
        # or (no Shift with the key) Shift pressed for the next one.
        time.sleep(0.002)
        if shift_first and shift_up_early:
            key(VK_SHIFT, True, "notepad")
        elif not shift_first:
            key(VK_SHIFT, False, "notepad")
        time.sleep(0.06)
        key(VK_K, True, "notepad")
        if VK_SHIFT in HELD:
            key(VK_SHIFT, True, "notepad")
        key(VK_CAPITAL, True, "notepad")
        time.sleep(0.6)
        check(label, notepad_text(np).strip(), THAI_SHIFT_K if shift_first else THAI_K)

    # 4. Ctrl+CapsLock still cycles the mode (manual -> auto), no CapsLock
    clear_notepad(np)
    set_lang("notepad", "en")
    mode0 = read_mode()
    caps0 = _global_caps()
    key(VK_CONTROL, False, "notepad")
    tap(VK_CAPITAL, "notepad", 60)
    key(VK_CONTROL, True, "notepad")
    time.sleep(1.5)
    check("Ctrl+CapsLock still cycles mode", (mode0, read_mode()), ("manual", "auto"))
    check("Ctrl+CapsLock leaves CapsLock and layout",
          (_global_caps(), lang_name(layout_of(hwnd))), (caps0, "en"))

    # 5. Ctrl+Alt+CapsLock turns RightType off; Caps is then plain CapsLock
    key(VK_CONTROL, False, "notepad")
    key(VK_MENU, False, "notepad")
    tap(VK_CAPITAL, "notepad", 60)
    key(VK_MENU, True, "notepad")
    key(VK_CONTROL, True, "notepad")
    time.sleep(1.5)
    check("Ctrl+Alt+CapsLock still turns RightType off", read_enabled(), "false")
    caps0 = _global_caps()
    tap(VK_CAPITAL, "notepad", 80)
    time.sleep(0.4)
    check("RightType off: CapsLock is plain", (_global_caps(), lang_name(layout_of(hwnd))),
          (1 - caps0, "en"))
    tap(VK_CAPITAL, "notepad", 80)  # back (still plain)
    time.sleep(0.3)
    key(VK_CONTROL, False, "notepad")
    key(VK_MENU, False, "notepad")
    tap(VK_CAPITAL, "notepad", 60)
    key(VK_MENU, True, "notepad")
    key(VK_CONTROL, True, "notepad")
    time.sleep(1.5)
    check("Ctrl+Alt+CapsLock turns RightType back on", read_enabled(), "true")

    # 6. focus change mid-press: the release does nothing, the next tap works
    edge_app, edge_httpd = lib.start_edge(lib.HERE / "target.html")
    edge_hwnd = edge_app.top_window().handle
    OWN_WINDOWS["edge"] = edge_hwnd
    focus("edge", edge_app)
    set_lang("edge", "en")
    focus("notepad", np)
    set_lang("notepad", "en")
    caps0 = _global_caps()
    key(VK_CAPITAL, False, "notepad")
    time.sleep(0.05)
    focus("edge", edge_app, 0.12)  # programmatic switch while Caps is held
    key(VK_CAPITAL, True, "edge")
    time.sleep(0.5)
    check("focus change mid-press: release switches nothing",
          (lang_name(layout_of(hwnd)), lang_name(layout_of(edge_hwnd)), _global_caps()),
          ("en", "en", caps0))
    tap(VK_CAPITAL, "edge", 80)
    time.sleep(0.5)
    check("focus change mid-press: next tap still switches", lang_name(layout_of(edge_hwnd)),
          "th")
    set_lang("edge", "en")

    # 6b. the same with Alt+Tab, only when Alt+Tab is certain to land on
    # our Edge window (it was active just before Notepad).
    focus("edge", edge_app)
    focus("notepad", np)
    key(VK_CAPITAL, False, "notepad")
    time.sleep(0.08)
    key(VK_MENU, False, "notepad")
    key(VK_TAB, False, "notepad")
    time.sleep(0.04)
    # The task switcher is in front from here until Alt is let go.
    key(VK_TAB, True)
    key(VK_MENU, True)
    time.sleep(0.6)
    if foreground_root() != edge_hwnd:
        key(VK_CAPITAL, True)
        raise Abort("Alt+Tab did not land on the probe's Edge window")
    key(VK_CAPITAL, True, "edge")
    time.sleep(0.5)
    check("Alt+Tab mid-press: release switches nothing",
          (lang_name(layout_of(hwnd)), lang_name(layout_of(edge_hwnd)), _global_caps()),
          ("en", "en", caps0))
    put_caps_back_now("edge")
    tap(VK_CAPITAL, "edge", 80)
    time.sleep(0.5)
    check("Alt+Tab mid-press: next tap still switches", lang_name(layout_of(edge_hwnd)), "th")

    # 6c. rollover in a browser text box (the page's textarea)
    set_lang("edge", "en")
    out = None
    for d in edge_app.top_window().descendants(control_type="Edit"):
        try:
            if not d.element_info.element.CurrentIsPassword:
                out = d
                break
        except Exception:
            pass
    if out is None:
        manual("rollover in Edge", "text box not found via UIA")
    else:
        out.click_input()
        time.sleep(0.5)
        require_focus("edge")
        key(VK_CONTROL, False, "edge")
        tap(VK_A, "edge", 30)
        key(VK_CONTROL, True, "edge")
        tap(VK_DELETE, "edge", 30)
        time.sleep(0.3)
        key(VK_CAPITAL, False, "edge")
        time.sleep(0.06)
        key(VK_K, False, "edge")
        time.sleep(0.03)
        key(VK_K, True, "edge")
        time.sleep(0.03)
        key(VK_CAPITAL, True, "edge")
        time.sleep(0.6)
        try:
            got = out.legacy_properties().get("Value") or ""
        except Exception:
            got = lib.read_browser_text(edge_app)
        check("rollover in Edge: letter in new layout", (got.strip(),
              lang_name(layout_of(edge_hwnd))), (THAI_K, "th"))
        set_lang("edge", "en")

    # 7. password field: Caps switches the language, no capitals
    pw = None
    for d in edge_app.top_window().descendants(control_type="Edit"):
        try:
            if d.element_info.element.CurrentIsPassword:
                pw = d
                break
        except Exception:
            pass
    if pw is None:
        manual("password field switches without capitals", "password box not found via UIA")
    else:
        pw.click_input()
        time.sleep(0.6)
        require_focus("edge")
        set_lang("edge", "en")
        caps0 = _global_caps()
        tap(VK_CAPITAL, "edge", 80)
        time.sleep(0.5)
        check("password field switches without capitals",
              (lang_name(layout_of(edge_hwnd)), _global_caps()), ("th", caps0))
        if _global_caps() != caps0:
            tap(VK_CAPITAL, "edge", 80)  # old build: it was a plain toggle
            time.sleep(0.3)
        set_lang("edge", "en")

    # 8. "disabled" mode: tap switches, a hold does nothing
    focus("notepad", np)
    start_rt('caps_mode = "disabled"\n')
    focus("notepad", np)
    set_lang("notepad", "en")
    caps0 = _global_caps()
    key(VK_CAPITAL, False, "notepad")
    time.sleep(0.7)
    key(VK_CAPITAL, True, "notepad")
    time.sleep(0.4)
    check("disabled mode: hold does nothing", (_global_caps(), lang_name(layout_of(hwnd))),
          (caps0, "en"))
    if _global_caps() != caps0:
        stop_rt()
        restore_caps()
    else:
        tap(VK_CAPITAL, "notepad", 80)
        time.sleep(0.5)
        check("disabled mode: tap switches", lang_name(layout_of(hwnd)), "th")
        set_lang("notepad", "en")

    # 9. a blocked app and an app switched off keep plain CapsLock
    for label, extra in (
        ("blocklisted app keeps plain CapsLock", 'custom_blacklist = ["notepad.exe"]\n'),
        ("app switched off keeps plain CapsLock", '[app_modes]\n"notepad.exe" = "off"\n'),
    ):
        # `[app_modes]` must come last in the file: caps_mode goes before it.
        start_rt('caps_mode = "tap"\n' + extra)
        focus("notepad", np)
        set_lang("notepad", "en")
        caps0 = _global_caps()
        tap(VK_CAPITAL, "notepad", 80)
        time.sleep(0.5)
        check(label, (_global_caps(), lang_name(layout_of(hwnd))), (1 - caps0, "en"))
        tap(VK_CAPITAL, "notepad", 80)
        time.sleep(0.4)
    manual("full-screen app keeps plain CapsLock",
           "needs a real full-screen game or presentation in front")


def cleanup():
    release_all()
    stop_rt()
    restore_caps()
    if edge_app is not None:
        try:
            edge_app.kill()
        except Exception:
            pass
    if edge_httpd is not None:
        try:
            edge_httpd.shutdown()
        except Exception:
            pass
    if notepad_pid:
        subprocess.run(["taskkill", "/PID", str(notepad_pid), "/F"], capture_output=True)


aborted = None
try:
    run_cases()
except Abort as e:
    aborted = str(e)
except KeyboardInterrupt:
    aborted = "interrupted"
finally:
    cleanup()

print("\nstderr log:", LOG)
print("CapsLock restored:", _global_caps() == ORIGINAL_CAPS)
if aborted:
    print("ABORTED:", aborted)
counts = {k: sum(1 for _, r in results if r == k) for k in ("PASS", "FAIL", "MANUAL")}
print(f"{counts['PASS']} passed, {counts['FAIL']} failed, {counts['MANUAL']} manual")
sys.exit(1 if aborted or counts["FAIL"] else 0)

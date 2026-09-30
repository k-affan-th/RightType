"""Real-app E2E for CI (GitHub's Windows runners) — and for anyone's desktop.

    python ci_sweep.py            # all targets
    python ci_sweep.py page omnibox notepad

On anyone's PC without a checkout: CI uploads RightType-selftest-<sha>, a zip
with the diagnostic build, these scripts and test-on-my-pc.bat. Unzip it,
double-click the .bat; it writes RightType-test-report.txt and
RightType-test-trace.txt next to itself. Apps that are not installed are
skipped.

Physical virtual-key events (as a person types) go into real apps with the
debug build of RightType, and the text is read back through UI Automation:

  page     — a text box in a web page (Chrome).
  omnibox  — the browser's address bar, with a history entry that makes it
             complete what is typed inline (the selected completion after the
             caret used to swallow the first Backspace of a correction).
  notepad  — Windows Notepad.
  hang     — proof for the keyboard hook: a window that takes 3 s to answer
             accessibility requests gets focus and a few keys; then the page
             must still be corrected. RightType's focus check asks that window
             on the thread that also runs the keyboard hook, and Windows drops
             a hook that does not answer in time.

Needs Thai Kedmanee and US English installed (CI adds Thai first). Exits 1 if
any case fails, except those listed in KNOWN_FAILING, which are reported but
do not fail the run until they are fixed (and then say so, as XPASS).
"""

import ctypes
import ctypes.wintypes as wt
import subprocess
import threading
import sys
import time
from pathlib import Path

import full_sweep as fs
import lib

tap, type_keys, CTRL, BACK, SHIFT = fs.tap, fs.type_keys, fs.CTRL, fs.BACK, fs.SHIFT
HKL_EN, HKL_TH = fs.HKL_EN, fs.HKL_TH
user32 = ctypes.windll.user32
ENTER = 0x0D

# Cases known to fail, by "target: name". Reported, not fatal, until fixed.
# (Windows 11 Notepad drops Thai injected as Unicode, E-024, but CI's
# Notepad is the classic editor, so nothing is listed for it here.)
KNOWN_FAILING: set[str] = set()
EDGE_ROUNDS = 4
CURRENT = [None]  # the running RightType process

flip = lambda: tap(BACK, SHIFT)  # noqa: E731


def installed_layouts():
    n = user32.GetKeyboardLayoutList(0, None)
    buf = (ctypes.c_void_p * n)()
    user32.GetKeyboardLayoutList(n, buf)
    return [(b or 0) & 0xFFFFFFFF for b in buf]


def rt_health(proc):
    """Is RightType still running, and does its UI thread (which also runs
    the keyboard hook) still answer? If it hangs, print every thread's stack
    with the Windows debugger when the runner has one."""
    code = proc.poll()
    if code is not None:
        return f"exited with code {code & 0xFFFFFFFF:#010x}"
    found = []
    proto = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)

    def each(hwnd, _):
        pid = wt.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value == proc.pid:
            found.append(hwnd)
        return True

    user32.EnumWindows(proto(each), 0)
    if not found:
        return "running, no window found"
    result = ctypes.c_size_t()
    ok = user32.SendMessageTimeoutW(found[0], 0, 0, 0, 0x0002, 3000, ctypes.byref(result))
    if ok:
        return "running and responsive"
    cdb = next((p for p in [
        r"C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe",
        r"C:\Program Files\Windows Kits\10\Debuggers\x64\cdb.exe",
    ] if Path(p).exists()), None)
    if cdb is None:
        return "running but NOT responding (no cdb on this machine for stacks)"
    out = subprocess.run(
        [cdb, "-pv", "-p", str(proc.pid), "-y", str(Path(str(fs.EXE)).parent),
         "-c", "~*kn 60; qd"],
        capture_output=True, text=True, errors="replace", timeout=180)
    return "running but NOT responding; thread stacks:\n" + out.stdout[-30000:]


# --------------------------------------------------------------------------- targets


class Page(fs.Chrome):
    name = "page"

    def __init__(self):
        if not lib.CHROME_EXE.exists():
            raise SkipTarget("Google Chrome is not installed here")
        super().__init__()


class HangPage(Page):
    name = "hang"


class Omnibox(fs.Target):
    """The address bar, after the test URL was typed there once, so typing
    its first letters again makes the browser complete the rest inline."""

    name = "omnibox"

    def __init__(self):
        if not lib.CHROME_EXE.exists():
            raise SkipTarget("Google Chrome is not installed here")
        self.app, self.httpd = lib.start_edge(lib.HERE / "target.html", lib.CHROME_EXE)
        self.win = self.app.top_window()
        self.hwnd = self.win.handle
        port = self.httpd.server_address[1]
        # Typed (not just visited) URLs are the ones completed inline.
        self.win.set_focus()
        self.layout(HKL_EN)
        tap(ord("L"), CTRL)
        type_keys(f"giupo-rt.localhost:{port}/target.html")
        tap(ENTER)
        time.sleep(3)
        self.box = next(
            d for d in self.win.descendants(control_type="Edit")
            if "address" in (d.element_info.name or "").lower()
        )

    def focus(self):
        self.win.set_focus()
        tap(ord("L"), CTRL)
        time.sleep(0.3)

    def clear(self):
        self.focus()
        tap(ord("A"), CTRL)
        tap(fs.DELETE)
        time.sleep(0.3)

    def read(self):
        return self.box.iface_value.CurrentValue or ""

    def close(self):
        tap(0x1B)  # Esc: leave the address bar as it was
        lib.close_app(self.app)
        self.httpd.shutdown()


class EdgeOmnibox(Omnibox):
    """Edge's address bar as it comes: no history, but Bing's search
    suggestions drop down (and arrive late) while you type."""

    name = "edge"

    def __init__(self):
        if not lib.EDGE_EXE.exists():
            raise SkipTarget("Microsoft Edge is not installed here")
        self.app, self.httpd = lib.start_edge(lib.HERE / "target.html", lib.EDGE_EXE)
        self.win = self.app.top_window()
        self.hwnd = self.win.handle
        self.box = next(
            d for d in self.win.descendants(control_type="Edit")
            if "address" in (d.element_info.name or "").lower()
        )


def fast_keys(s, pause=0.03):
    """A quick typist: the keys land while the app is still busy."""
    for ch in s:
        if ch == " ":
            vk = fs.SPACE
        elif ch.isalpha() or ch.isdigit():
            vk = ord(ch.upper())
        elif ch in fs.SHIFTED:
            base = fs.SHIFTED[ch]
            fs.tap(fs.PUNCT.get(base, ord(base)), fs.SHIFT, pause=pause)
            continue
        else:
            vk = fs.PUNCT[ch]
        if ch.isupper():
            fs.tap(vk, fs.SHIFT, pause=pause)
        else:
            fs.tap(vk, pause=pause)


def watch(t, name, keys, expect, typer=type_keys):
    """Type, then read the field every 0.25 s for 2 s, to see what changes
    after the keys (suggestions arriving late, say)."""
    t.clear()
    t.layout(HKL_EN)
    typer(keys)
    seen = []
    for _ in range(8):
        time.sleep(0.25)
        v = t.read()
        if not seen or seen[-1] != v:
            seen.append(v)
    print(f"  {t.name} {name}: {seen}", flush=True)
    return fs.check(t.name, name, seen[-1], expect)


class Notepad(fs.Target):
    name = "notepad"

    def __init__(self):
        self.app = lib.start_notepad()
        self.win = self.app.top_window()
        self.hwnd = self.win.handle
        kinds = sorted({d.element_info.class_name for d in self.win.descendants()})
        print(f"notepad window classes: {kinds}", flush=True)

    def focus(self):
        self.win.set_focus()
        time.sleep(0.2)

    def read(self):
        for d in self.win.descendants():
            if d.element_info.control_type in ("Document", "Edit"):
                try:
                    return d.iface_value.CurrentValue or ""
                except Exception:
                    try:
                        return d.iface_text.DocumentRange.GetText(-1) or ""
                    except Exception:
                        continue
        return ""

    def close(self):
        self.clear()
        try:
            self.app.kill()
        except Exception:
            pass


class SkipTarget(Exception):
    """The app this target needs is not on this machine."""


class Notepad11(Notepad):
    """Windows 11's Notepad (the Store app: a RichEdit box in a WinUI
    window), not the classic one CI has in System32. Thai typed there by
    RightType arrived with characters missing on a real PC."""

    name = "notepad11"
    APP = r"shell:AppsFolder\Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"

    def __init__(self):
        from pywinauto import Application, Desktop
        subprocess.run(["taskkill", "/IM", "notepad.exe", "/F"], capture_output=True)
        time.sleep(1)

        def windows():
            out = {}
            for w in Desktop(backend="uia").windows(top_level_only=True):
                try:
                    if w.class_name() == "Notepad":
                        out[w.handle] = w
                except Exception:
                    pass
            return out

        before = set(windows())
        subprocess.Popen(["explorer.exe", self.APP])
        deadline = time.time() + 30
        while time.time() < deadline:
            fresh = [h for h in windows() if h not in before]
            if fresh:
                break
            time.sleep(0.5)
        else:
            raise SkipTarget("Windows 11 Notepad is not installed here")
        time.sleep(2.0)
        self.app = Application(backend="uia").connect(handle=fresh[0], timeout=10)
        self.win = self.app.top_window()
        self.hwnd = self.win.handle
        kinds = sorted({d.element_info.class_name for d in self.win.descendants()})
        print(f"notepad11 window classes: {kinds}", flush=True)
        self.focus()

        class GUITHREADINFO(ctypes.Structure):
            _fields_ = [("cbSize", wt.DWORD), ("flags", wt.DWORD), ("hwndActive", wt.HWND),
                        ("hwndFocus", wt.HWND), ("hwndCapture", wt.HWND),
                        ("hwndMenuOwner", wt.HWND), ("hwndMoveSize", wt.HWND),
                        ("hwndCaret", wt.HWND), ("rcCaret", wt.RECT)]

        gui = GUITHREADINFO(cbSize=ctypes.sizeof(GUITHREADINFO))
        tid = user32.GetWindowThreadProcessId(self.hwnd, None)
        user32.GetGUIThreadInfo(tid, ctypes.byref(gui))
        cls = ctypes.create_unicode_buffer(64)
        user32.GetClassNameW(gui.hwndFocus, cls, 64)
        print(f"notepad11 keyboard focus window class: {cls.value!r}", flush=True)
        if "RichEditD2DPT" not in kinds:
            self.close()
            raise SkipTarget("the Notepad that opened is not the Windows 11 one")

    def focus(self):
        self.win.set_focus()
        for d in self.win.descendants():
            if d.element_info.class_name == "RichEditD2DPT":
                d.set_focus()
                break
        time.sleep(0.2)


def packets(text, pause=0.0):
    """Send `text` as KEYEVENTF_UNICODE key events (what RightType's
    injector sends): all at once when `pause` is 0, else one by one."""
    events = []
    for ch in text:
        for up in (False, True):
            i = fs.INPUT(type=1)
            i.u.ki = fs.KEYBDINPUT(0, ord(ch), 0x4 | (0x2 if up else 0), 0, 0)
            events.append(i)
    if pause:
        for i in range(0, len(events), 2):
            arr = (fs.INPUT * 2)(*events[i:i + 2])
            user32.SendInput(2, arr, ctypes.sizeof(fs.INPUT))
            time.sleep(pause)
    else:
        arr = (fs.INPUT * len(events))(*events)
        user32.SendInput(len(events), arr, ctypes.sizeof(fs.INPUT))
    time.sleep(0.8)


def unicode_events(text):
    out = []
    for ch in text:
        for up in (False, True):
            i = fs.INPUT(type=1)
            i.u.ki = fs.KEYBDINPUT(0, ord(ch), 0x4 | (0x2 if up else 0), 0, 0)
            out.append(i)
    return out


def backspace_events(n):
    out = []
    for _ in range(n):
        for up in (False, True):
            i = fs.INPUT(type=1)
            i.u.ki = fs.KEYBDINPUT(0x08, 0, 0x2 if up else 0, 0, 0)
            out.append(i)
    return out


def send(events):
    if events:
        arr = (fs.INPUT * len(events))(*events)
        user32.SendInput(len(events), arr, ctypes.sizeof(fs.INPUT))


def replace_typed(keys, thai, how):
    """Type `keys` for real, then replace them with `thai` the way `how`
    says: RightType's single batch, or a variant of it."""
    type_keys(keys)
    time.sleep(0.3)
    n = len(keys)
    if how == "one batch (RightType)":
        send(backspace_events(n) + unicode_events(thai))
    elif how == "two calls":
        send(backspace_events(n))
        send(unicode_events(thai))
    elif how == "two calls, 30 ms apart":
        send(backspace_events(n))
        time.sleep(0.03)
        send(unicode_events(thai))
    elif how == "one call per character":
        send(backspace_events(n))
        for ch in thai:
            send(unicode_events(ch))
    time.sleep(0.8)


def notepad11_probe(t):
    """Evidence, with RightType not running: what does Windows 11 Notepad
    keep of Thai sent the way RightType sends it? Printed, not judged."""
    word = "วันนี้"
    variants = [
        ("ASCII burst, English layout", HKL_EN, lambda: packets("hello"), "hello"),
        ("Thai burst, English layout", HKL_EN, lambda: packets(word), word),
        ("Thai one by one, English layout", HKL_EN, lambda: packets(word, 0.15), word),
        ("Thai burst, Thai layout", HKL_TH, lambda: packets(word), word),
        ("Thai one by one, Thai layout", HKL_TH, lambda: packets(word, 0.15), word),
        # What RightType does at the anchor: the last mark right as it asks
        # the window to switch to the Thai layout.
        ("last mark sent with a layout switch", HKL_EN,
         lambda: (packets(word[:-1]), user32.PostMessageW(t.hwnd, 0x0050, 0, HKL_TH),
                  packets(word[-1])), word),
        ("Thai typed on the Thai keyboard", HKL_TH, lambda: type_keys(";yoouh"), word),
    ]
    for how in ["one batch (RightType)", "two calls", "two calls, 30 ms apart",
                "one call per character"]:
        variants.append((f"typed keys replaced: {how}", HKL_EN,
                         lambda how=how: replace_typed("l;ylf", "สวัสดี", how), "สวัสดี"))
    for how in ["one batch (RightType)", "two calls, 30 ms apart"]:
        variants.append((f"typed keys replaced by English: {how}", HKL_EN,
                         lambda how=how: replace_typed("l;ylf", "hello", how), "hello"))
    for name, hkl, act, expect in variants:
        got = []
        for _ in range(3):
            t.clear()
            t.layout(hkl)
            act()
            got.append(t.read().strip())
        kept = sum(g == expect for g in got)
        print(f"  probe notepad11 {name}: {kept}/3 intact {got}", flush=True)


# --------------------------------------------------------------------------- cases


class HangWindow:
    """A top-level window whose thread takes `hang` seconds to answer
    WM_GETOBJECT (an accessibility request), like an app busy with its own
    work. Anyone asking it who has focus waits that long."""

    def __init__(self, hang=3.0):
        self.hang = hang
        self.hwnd = None
        ready = threading.Event()
        threading.Thread(target=self._run, args=(ready,), daemon=True).start()
        if not ready.wait(10):
            raise SystemExit("hang window did not start")

    def _run(self, ready):
        LRESULT = ctypes.c_ssize_t
        WNDPROC = ctypes.WINFUNCTYPE(LRESULT, wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM)
        user32.DefWindowProcW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
        user32.DefWindowProcW.restype = LRESULT

        def proc(h, m, w, l):
            if m == 0x003D:  # WM_GETOBJECT
                time.sleep(self.hang)
            return user32.DefWindowProcW(h, m, w, l)

        self._proc = WNDPROC(proc)

        class WNDCLASSW(ctypes.Structure):
            _fields_ = [("style", wt.UINT), ("lpfnWndProc", WNDPROC), ("cbClsExtra", ctypes.c_int),
                        ("cbWndExtra", ctypes.c_int), ("hInstance", wt.HINSTANCE), ("hIcon", wt.HICON),
                        ("hCursor", wt.HANDLE), ("hbrBackground", wt.HBRUSH),
                        ("lpszMenuName", wt.LPCWSTR), ("lpszClassName", wt.LPCWSTR)]

        hinst = ctypes.windll.kernel32.GetModuleHandleW(None)
        wc = WNDCLASSW(lpfnWndProc=self._proc, hInstance=hinst, lpszClassName="RtHangWindow")
        user32.RegisterClassW(ctypes.byref(wc))
        user32.CreateWindowExW.restype = wt.HWND
        self.hwnd = user32.CreateWindowExW(0, "RtHangWindow", "RightType hang test",
                                           0x10CF0000, 100, 100, 400, 200, None, None, hinst, None)
        ready.set()
        msg = wt.MSG()
        while user32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
            user32.TranslateMessage(ctypes.byref(msg))
            user32.DispatchMessageW(ctypes.byref(msg))

    def focus(self):
        tap(fs.ALT)  # an Alt press lets this process take the foreground
        user32.SetForegroundWindow(self.hwnd)

    def close(self):
        user32.PostMessageW(self.hwnd, 0x0010, 0, 0)  # WM_CLOSE


def hang_sweep(t):
    """Keys while a focused app is slow to answer must not cost the hook."""
    fs.run(t, "before the slow window", "l;ylfu ", "สวัสดี")
    w = HangWindow(hang=3.0)
    w.focus()
    time.sleep(0.3)
    for _ in range(6):  # typed while RightType's focus check waits on it
        tap(ord("X"), pause=0.25)
    time.sleep(3.5)
    # Edge's crash, forced: a word RightType converts mid-word is finished by
    # a space while RightType is still waiting inside an accessibility call
    # (its focus check on the slow window), so the space is handled inside
    # that call. In Edge this happened only now and then, and then RightType
    # died (0xC000041D). Here every word lands inside the wait.
    for _ in range(3):
        user32.SetForegroundWindow(t.hwnd)
        time.sleep(0.5)
        w.focus()
        time.sleep(0.3)
        type_keys("l;ylfu ")
        time.sleep(3.5)
    fs.check(t.name, "a converted word finished inside a slow accessibility call",
             rt_health(CURRENT[0]), "running and responsive")
    w.close()
    fs.run(t, "after the slow window", "l;ylfu ", "สวัสดี")


def clipboard_state():
    """The clipboard's sequence number and its text, to see whether
    anything wrote to it."""
    seq = user32.GetClipboardSequenceNumber()
    text = None
    k32 = ctypes.windll.kernel32
    k32.GlobalLock.restype = ctypes.c_void_p
    k32.GlobalLock.argtypes = [ctypes.c_void_p]
    user32.GetClipboardData.restype = ctypes.c_void_p
    for _ in range(20):
        if user32.OpenClipboard(None):
            h = user32.GetClipboardData(13)  # CF_UNICODETEXT
            if h:
                p = k32.GlobalLock(h)
                text = ctypes.wstring_at(p) if p else None
                k32.GlobalUnlock(ctypes.c_void_p(h))
            user32.CloseClipboard()
            break
        time.sleep(0.05)
    return seq, text


def put_on_clipboard(text):
    k32 = ctypes.windll.kernel32
    k32.GlobalAlloc.restype = ctypes.c_void_p
    k32.GlobalLock.restype = ctypes.c_void_p
    k32.GlobalLock.argtypes = [ctypes.c_void_p]
    data = (text + "\0").encode("utf-16-le")
    h = k32.GlobalAlloc(0x0002, len(data))
    ctypes.memmove(k32.GlobalLock(h), data, len(data))
    k32.GlobalUnlock(ctypes.c_void_p(h))
    for _ in range(20):
        if user32.OpenClipboard(None):
            user32.EmptyClipboard()
            user32.SetClipboardData.argtypes = [wt.UINT, ctypes.c_void_p]
            user32.SetClipboardData(13, h)
            user32.CloseClipboard()
            return
        time.sleep(0.05)
    raise SystemExit("could not open the clipboard")


def selection_leaves_clipboard_alone(t):
    """Converting a selection must not pass it through the clipboard:
    Windows keeps clipboard history, can sync it to other devices (an
    Android phone), and any program may watch it."""
    put_on_clipboard("rt-e2e clipboard sentinel")
    before = clipboard_state()
    fs.run(t, "convert selection", "ok", "นา",
           then=[fs.select_word, fs.convert_sel], settle=1.5)
    after = clipboard_state()
    fs.check(t.name, "convert selection leaves the clipboard alone",
             f"seq+{after[0] - before[0]} {after[1]!r}", f"seq+0 {before[1]!r}")


HUMAN = __import__("random").Random(20260930)  # same "typist" every run


def human_keys(s):
    """Keys at a person's uneven pace: mostly 60-160 ms apart, sometimes a
    quick burst (25 ms), sometimes a pause (300 ms)."""
    for ch in s:
        roll = HUMAN.random()
        pause = 0.025 if roll < 0.2 else 0.3 if roll > 0.95 else HUMAN.uniform(0.06, 0.16)
        fast_keys(ch, pause=pause)


# Sentences as people type them, some from real bug reports.
REAL_SENTENCES = [
    ("Thai then English, one line", ";yoouh there is ", "วันนี้ there is"),
    ("Thai sentence", "lj'wa]N,k.shsojvp ", "ส่งไฟล์มาให้หน่อย"),
    ("English then Thai", "hello l;ylfu8iy[ ", "hello สวัสดีครับ"),
    ("Thai then an English tech word", "l;ylfu8iy[ middleware ", "สวัสดีครับ middleware"),
    ("English computer words", "relogin logout ", "relogin logout"),
]


def realistic(t):
    for name, keys, expect in REAL_SENTENCES:
        fs.run(t, f"typed like a person: {name}", "", expect,
               then=[lambda keys=keys: human_keys(keys)], settle=1.2)


def sweep(t):
    run = fs.run
    run(t, "EN->TH word", "l;ylfu ", "สวัสดี")
    run(t, "letters-only Thai word", "giupo ", "เรียน")
    run(t, "EN->TH then Thai typed natively", "l;ylfu giupo ", "สวัสดี เรียน")
    run(t, "TH->EN word", "correct ", "correct", layout=HKL_TH)
    # Not Thai: it only starts like three short Thai words (พำ สน เร).
    run(t, "English computer word stays English", "relogin ", "relogin")
    # A wrong correction is undone with one Shift+Backspace, wherever it
    # happened: in the middle of a word, after a long word was handed to the
    # Thai layout, or at the space.
    run(t, "Shift+Backspace takes back a mid-word conversion", "l;ylfu", "l;ylfu", then=[flip])
    run(t, "Shift+Backspace takes back a long word", "l;ylfu8iy[", "l;ylfu8iy[", then=[flip])
    # A word Auto keeps as English (it spells Thai นา too); flipped by hand.
    run(t, "Shift+Backspace flips EN to TH", "ok", "นา", then=[flip])
    run(t, "Shift+Backspace undoes an automatic fix", "correct ", "แนพพำแะ",
        layout=HKL_TH, then=[flip])
    # Pressed once too often: with no older word to reach, the next press
    # puts the word back (it used to say "nothing to flip").
    run(t, "Shift+Backspace twice puts the word back", "reload ", "reload", then=[flip, flip])
    # Found on a real PC: keys typed quickly while a word was being rewritten
    # were lost. A person typing ~30 ms per key, word ended by a space.
    fs.run(t, "fast typing through a correction", "", "สวัสดีครับ",
           then=[lambda: fast_keys("l;ylfu8iy["), lambda: tap(fs.SPACE)])
    realistic(t)
    selection_leaves_clipboard_alone(t)


def edge_sweep(t):
    watch(t, "EN->TH word, suggestions open", "l;ylfu", "สวัสดี")
    watch(t, "EN->TH word, typed fast", "l;ylfu", "สวัสดี", typer=fast_keys)
    watch(t, "letters-only Thai word", "giupo", "เรียน")
    sweep(t)


def ui_timing():
    """How long each RightType window takes to build and to appear painted
    (the debug build traces it). Printed, not judged."""
    import re
    for what in ["settings", "stats", "fixer", "welcome"]:
        start = fs.LOG.stat().st_size if fs.LOG.exists() else 0
        proc = fs.start_rt({"RIGHTTYPE_SHOW": what})
        time.sleep(4)
        with open(fs.LOG, encoding="utf-8", errors="replace") as f:
            f.seek(start)
            lines = [m.group(0) for m in re.finditer(r"window [^:]+: built in \d+ ms, shown painted at \d+ ms", f.read())]
        print(f"  ui {what}: {lines or 'no timing line'}", flush=True)
        proc.kill()
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
        time.sleep(0.5)


def main():
    want = set(sys.argv[1:]) or {"page", "omnibox", "edge", "notepad", "notepad11", "hang"}
    # Thai must be loaded for this session (CI installs it just before).
    user32.LoadKeyboardLayoutW("0000041E", 0)
    user32.LoadKeyboardLayoutW("00000409", 0)
    layouts = [f"{h:08X}" for h in installed_layouts()]
    print(f"keyboard layouts: {layouts}", flush=True)
    if not {f"{HKL_EN:08X}", f"{HKL_TH:08X}"} <= set(layouts):
        raise SystemExit("US English and Thai Kedmanee must both be installed")

    fs.write_config(mode="auto", learn=False)
    # Edge runs several rounds: RightType froze there in some runs and not
    # others (after a word boundary handled inside a focus callback), and one
    # clean round proves nothing.
    targets = [("page", Page), ("omnibox", Omnibox)] + [("edge", EdgeOmnibox)] * EDGE_ROUNDS + [
        ("notepad", Notepad), ("notepad11", Notepad11), ("hang", HangPage)]
    sections = {}
    try:
        for key, make in targets:
            if key not in want:
                continue
            start = fs.LOG.stat().st_size if fs.LOG.exists() else 0
            # The address bar is seeded before RightType runs, so the URL is
            # typed exactly as written.
            try:
                if key in ("omnibox", "notepad11"):
                    subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
                    t = make()
                    if key == "notepad11":
                        notepad11_probe(t)
                    proc = fs.start_rt()
                    t.focus()
                else:
                    proc = fs.start_rt()
                    t = make()
            except SkipTarget as why:
                print(f"{key} skipped: {why}", flush=True)
                continue
            CURRENT[0] = proc
            # A target where RightType died leaves CapsLock on (its
            # Shift+CapsLock then reached Windows); do not let that fail
            # every target after it.
            if user32.GetKeyState(fs.CAPS) & 1:
                print("CapsLock was left on; turning it off", flush=True)
                tap(fs.CAPS)
            try:
                {"edge": edge_sweep, "hang": hang_sweep}.get(key, sweep)(t)
            finally:
                print(f"RightType after {key}: {rt_health(proc)}", flush=True)
                t.close()
                sections.setdefault(key, []).append((start, fs.LOG.stat().st_size))
    finally:
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)

    ui_timing()

    failed, known, fixed = [], [], []
    for target, name, ok, got, expect in fs.RESULTS:
        label = f"{target}: {name}"
        if label in KNOWN_FAILING:
            (fixed if ok else known).append(label)
        elif not ok:
            failed.append(f"{label} — got {got.strip()!r}, expected {expect.strip()!r}")
    passed = sum(r[2] for r in fs.RESULTS)
    # Each failing target's own part of the trace (the log is shared, so its
    # end belongs to whichever target ran last). Printed before the summary:
    # log viewers and the API return only the end of a long log, and the
    # summary is what has to be in it.
    data = fs.LOG.read_bytes() if failed else b""
    for key, spans in sections.items():
        if any(line.startswith(f"{key}:") for line in failed):
            for n, (start, end) in enumerate(spans, 1):
                part = data[start:end].decode("utf-8", errors="replace")
                print(f"\n--- RightType trace: {key} (round {n}) ---")
                print(part[-60000:])
    print(f"\nSUMMARY {passed}/{len(fs.RESULTS)} passed; trace: {fs.LOG}")
    for label in known:
        print(f"  KNOWN FAILING {label}")
    for label in fixed:
        print(f"  XPASS {label} — remove it from KNOWN_FAILING")
    for line in failed:
        print(f"  FAIL {line}")
    report = lib.HERE / "RightType-test-report.txt"
    lines = [f"SUMMARY {passed}/{len(fs.RESULTS)} passed"]
    lines += [f"FAIL {line}" for line in failed] + [f"KNOWN FAILING {x}" for x in known]
    lines += [f"{'PASS' if ok else 'FAIL'} {target}: {name}: got {got.strip()!r}"
              for target, name, ok, got, _ in fs.RESULTS]
    report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    try:
        (lib.HERE / "RightType-test-trace.txt").write_bytes(fs.LOG.read_bytes())
    except OSError:
        pass
    print(f"report: {report}", flush=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()

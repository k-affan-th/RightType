"""Real-app E2E for CI (GitHub's Windows runners) — and for anyone's desktop.

    python ci_sweep.py            # all targets
    python ci_sweep.py page omnibox notepad

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


class HangPage(Page):
    name = "hang"


class Omnibox(fs.Target):
    """The address bar, after the test URL was typed there once, so typing
    its first letters again makes the browser complete the rest inline."""

    name = "omnibox"

    def __init__(self):
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
        fs.tap(ord(ch.upper()) if ch.isalpha() else fs.PUNCT[ch], pause=pause)


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
    selection_leaves_clipboard_alone(t)


def edge_sweep(t):
    watch(t, "EN->TH word, suggestions open", "l;ylfu", "สวัสดี")
    watch(t, "EN->TH word, typed fast", "l;ylfu", "สวัสดี", typer=fast_keys)
    watch(t, "letters-only Thai word", "giupo", "เรียน")
    sweep(t)


def main():
    want = set(sys.argv[1:]) or {"page", "omnibox", "edge", "notepad", "hang"}
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
        ("notepad", Notepad), ("hang", HangPage)]
    sections = {}
    try:
        for key, make in targets:
            if key not in want:
                continue
            start = fs.LOG.stat().st_size if fs.LOG.exists() else 0
            # The address bar is seeded before RightType runs, so the URL is
            # typed exactly as written.
            if key == "omnibox":
                subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
                t = make()
                proc = fs.start_rt()
                t.focus()
            else:
                proc = fs.start_rt()
                t = make()
            CURRENT[0] = proc
            try:
                {"edge": edge_sweep, "hang": hang_sweep}.get(key, sweep)(t)
            finally:
                print(f"RightType after {key}: {rt_health(proc)}", flush=True)
                t.close()
                sections.setdefault(key, []).append((start, fs.LOG.stat().st_size))
    finally:
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)

    failed, known, fixed = [], [], []
    for target, name, ok, got, expect in fs.RESULTS:
        label = f"{target}: {name}"
        if label in KNOWN_FAILING:
            (fixed if ok else known).append(label)
        elif not ok:
            failed.append(f"{label} — got {got.strip()!r}, expected {expect.strip()!r}")
    passed = sum(r[2] for r in fs.RESULTS)
    print(f"\nSUMMARY {passed}/{len(fs.RESULTS)} passed; trace: {fs.LOG}")
    for label in known:
        print(f"  KNOWN FAILING {label}")
    for label in fixed:
        print(f"  XPASS {label} — remove it from KNOWN_FAILING")
    for line in failed:
        print(f"  FAIL {line}")
    # Each failing target's own part of the trace (the log is shared, so its
    # end belongs to whichever target ran last).
    data = fs.LOG.read_bytes() if failed else b""
    for key, spans in sections.items():
        if any(line.startswith(f"{key}:") for line in failed):
            for n, (start, end) in enumerate(spans, 1):
                part = data[start:end].decode("utf-8", errors="replace")
                print(f"\n--- RightType trace: {key} (round {n}) ---")
                print(part[-60000:])
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()

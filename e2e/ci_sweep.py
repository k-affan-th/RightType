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

Needs Thai Kedmanee and US English installed (CI adds Thai first). Exits 1 if
any case fails, except those listed in KNOWN_FAILING, which are reported but
do not fail the run until they are fixed (and then say so, as XPASS).
"""

import ctypes
import subprocess
import sys
import time

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

flip = lambda: tap(BACK, SHIFT)  # noqa: E731


def installed_layouts():
    n = user32.GetKeyboardLayoutList(0, None)
    buf = (ctypes.c_void_p * n)()
    user32.GetKeyboardLayoutList(n, buf)
    return [(b or 0) & 0xFFFFFFFF for b in buf]


# --------------------------------------------------------------------------- targets


class Page(fs.Chrome):
    name = "page"


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


def sweep(t):
    run = fs.run
    run(t, "EN->TH word", "l;ylfu ", "สวัสดี")
    run(t, "letters-only Thai word", "giupo ", "เรียน")
    run(t, "EN->TH then Thai typed natively", "l;ylfu giupo ", "สวัสดี เรียน")
    run(t, "TH->EN word", "correct ", "correct", layout=HKL_TH)
    # Not Thai: it only starts like three short Thai words (พำ สน เร).
    run(t, "English computer word stays English", "relogin ", "relogin")
    # A word Auto keeps as English (it spells Thai นา too); flipped by hand.
    run(t, "Shift+Backspace flips EN to TH", "ok", "นา", then=[flip])
    run(t, "Shift+Backspace undoes an automatic fix", "correct ", "แนพพำแะ",
        layout=HKL_TH, then=[flip])


def edge_sweep(t):
    watch(t, "EN->TH word, suggestions open", "l;ylfu", "สวัสดี")
    watch(t, "EN->TH word, typed fast", "l;ylfu", "สวัสดี", typer=fast_keys)
    watch(t, "letters-only Thai word", "giupo", "เรียน")
    sweep(t)


def main():
    want = set(sys.argv[1:]) or {"page", "omnibox", "edge", "notepad"}
    # Thai must be loaded for this session (CI installs it just before).
    user32.LoadKeyboardLayoutW("0000041E", 0)
    user32.LoadKeyboardLayoutW("00000409", 0)
    layouts = [f"{h:08X}" for h in installed_layouts()]
    print(f"keyboard layouts: {layouts}", flush=True)
    if not {f"{HKL_EN:08X}", f"{HKL_TH:08X}"} <= set(layouts):
        raise SystemExit("US English and Thai Kedmanee must both be installed")

    fs.write_config(mode="auto", learn=False)
    targets = [("page", Page), ("omnibox", Omnibox), ("edge", EdgeOmnibox), ("notepad", Notepad)]
    try:
        for key, make in targets:
            if key not in want:
                continue
            # The address bar is seeded before RightType runs, so the URL is
            # typed exactly as written.
            if key == "omnibox":
                subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
                t = make()
                fs.start_rt()
                t.focus()
            else:
                fs.start_rt()
                t = make()
            try:
                edge_sweep(t) if key == "edge" else sweep(t)
            finally:
                t.close()
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
    if failed:
        print("\n--- RightType trace ---")
        print(fs.LOG.read_text(encoding="utf-8", errors="replace")[-20000:])
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()

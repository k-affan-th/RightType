"""Full behavioural sweep of RightType with physical keystrokes.

Run from e2e/ after `cargo build --features winos`, hands off keyboard/mouse:

    uv run python full_sweep.py [chrome] [claude] [ui]

Every mode, hotkey and window, in real apps. Keys are sent as virtual-key
events (not VK_PACKET), so layout switches behave exactly as for a human.
Never sends Enter (the Claude composer would submit). Uses an isolated data
dir so the operator's config and learned words are untouched.
"""

import ctypes
import ctypes.wintypes as wt
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from pywinauto import Desktop

import lib

REPO = lib.REPO
EXE = lib.EXE
WORK = Path(tempfile.mkdtemp(prefix="rt_sweep_"))
DATA = WORK / "rtdata"
LOG = WORK / "rt.log"
DATA.mkdir()

user32 = ctypes.windll.user32
HKL_EN, HKL_TH = 0x04090409, 0x041E041E

# --------------------------------------------------------------------------- keys


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wt.WORD), ("wScan", wt.WORD), ("dwFlags", wt.DWORD),
                ("time", wt.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class _U(ctypes.Union):
    _fields_ = [("ki", KEYBDINPUT), ("pad", ctypes.c_byte * 32)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wt.DWORD), ("u", _U)]


def key(vk, up=False, ext=False):
    i = INPUT(type=1)
    flags = (0x2 if up else 0) | (0x1 if ext else 0)
    i.u.ki = KEYBDINPUT(vk, user32.MapVirtualKeyW(vk, 0), flags, 0, 0)
    user32.SendInput(1, ctypes.byref(i), ctypes.sizeof(INPUT))


SHIFT, CTRL, ALT, BACK, SPACE, CAPS, DELETE, HOME, END = 0x10, 0x11, 0x12, 0x08, 0x20, 0x14, 0x2E, 0x24, 0x23
EXTENDED = {DELETE, HOME, END}


def tap(vk, *mods, pause=0.07):
    for m in mods:
        key(m)
    key(vk, ext=vk in EXTENDED)
    key(vk, True, ext=vk in EXTENDED)
    for m in reversed(mods):
        key(m, True)
    time.sleep(pause)


PUNCT = {";": 0xBA, "=": 0xBB, ",": 0xBC, "-": 0xBD, ".": 0xBE, "/": 0xBF,
         "`": 0xC0, "[": 0xDB, "\\": 0xDC, "]": 0xDD, "'": 0xDE}
SHIFTED = {":": ";", "+": "=", "<": ",", "_": "-", ">": ".", "?": "/", "{": "[", "}": "]",
           '"': "'", "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6",
           "&": "7", "*": "8", "(": "9", ")": "0"}


def type_keys(s):
    """Physical keys named by their US character; ' ' is a real VK_SPACE."""
    for ch in s:
        if ch == " ":
            tap(SPACE)
        elif ch.isalpha():
            tap(ord(ch.upper()), SHIFT) if ch.isupper() else tap(ord(ch.upper()))
        elif ch.isdigit():
            tap(ord(ch))
        elif ch in PUNCT:
            tap(PUNCT[ch])
        elif ch in SHIFTED:
            base = SHIFTED[ch]
            tap(PUNCT.get(base, ord(base)), SHIFT)
        else:
            raise ValueError(ch)


# --------------------------------------------------------------------------- app


def write_config(mode="auto", learn=True, onboarded=True):
    (DATA / "config.toml").write_text(
        f'enabled = true\nmode = "{mode}"\nlearn = {str(learn).lower()}\n'
        f"custom_blacklist = []\nonboarded = {str(onboarded).lower()}\n",
        encoding="utf-8",
    )


def config_value(name):
    text = (DATA / "config.toml").read_text(encoding="utf-8")
    m = re.search(rf"^{name} = (.*)$", text, re.M)
    return m.group(1).strip('"') if m else None


def start_rt(extra_env=None):
    subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
    time.sleep(0.8)
    env = {**os.environ, "RIGHTTYPE_E2E_ACCEPT_INJECTED": "1", "RIGHTTYPE_E2E_DATA_DIR": str(DATA),
           **(extra_env or {})}
    proc = subprocess.Popen([str(EXE)], env=env, stderr=open(LOG, "a", encoding="utf-8"))
    time.sleep(2.5)
    if proc.poll() is not None:
        raise SystemExit("righttype exited — rebuild with cargo build --features winos")
    return proc


def log_size():
    return LOG.stat().st_size


def log_since(pos):
    with open(LOG, encoding="utf-8", errors="replace") as f:
        f.seek(pos)
        return f.read()


def set_mode(target):
    for _ in range(4):
        if config_value("mode") == target:
            return True
        tap(CAPS, CTRL)
        time.sleep(0.6)
    return config_value("mode") == target


# --------------------------------------------------------------------------- targets


class Target:
    name = "?"
    hwnd = 0

    def focus(self):
        raise NotImplementedError

    def read(self):
        raise NotImplementedError

    def clear(self):
        self.focus()
        tap(ord("A"), CTRL)
        tap(DELETE)
        time.sleep(0.3)

    def layout(self, hkl):
        user32.PostMessageW(self.hwnd, 0x0050, 0, hkl)
        time.sleep(0.4)


class Chrome(Target):
    name = "chrome"

    def __init__(self):
        self.app, self.httpd = lib.start_edge(lib.HERE / "target.html", lib.CHROME_EXE)
        self.win = self.app.top_window()
        self.hwnd = self.win.handle
        edits = {d.element_info.automation_id: d for d in self.win.descendants(control_type="Edit")}
        self.box, self.pw = edits["out"], edits["pw"]

    def focus(self):
        self.win.set_focus()
        self.box.set_focus()
        time.sleep(0.2)

    def read(self):
        return self.box.iface_value.CurrentValue or ""

    def close(self):
        lib.close_app(self.app)
        self.httpd.shutdown()


class Claude(Target):
    name = "claude"

    def __init__(self):
        for w in Desktop(backend="uia").windows(top_level_only=True):
            try:
                if w.window_text() != "Claude":
                    continue
                for d in w.descendants(control_type="Edit"):
                    if d.element_info.name == "Prompt":
                        self.win, self.box, self.hwnd = w, d, w.handle
                        if self.read().strip():
                            raise SystemExit("Claude composer has a draft — not touching it")
                        return
            except SystemExit:
                raise
            except Exception:
                continue
        raise SystemExit("Claude prompt box not found")

    def focus(self):
        self.win.set_focus()
        self.box.set_focus()
        time.sleep(0.2)

    def read(self):
        try:
            return self.box.iface_value.CurrentValue or ""
        except Exception:
            return self.box.window_text()

    def close(self):
        self.clear()


# --------------------------------------------------------------------------- cases

RESULTS = []


def check(target, name, got, expect):
    ok = got.strip() == expect.strip()
    RESULTS.append((target, name, ok, got, expect))
    print(f"[{'PASS' if ok else 'FAIL'}] {target:7} {name}: got {got.strip()!r}"
          + ("" if ok else f" expected {expect.strip()!r}"), flush=True)
    return ok


ONLY = [x for x in os.environ.get("ONLY", "").split("|") if x]


def run(t, name, keys, expect, layout=HKL_EN, then=(), settle=0.8):
    if ONLY and not any(x in name for x in ONLY):
        return True
    t.clear()
    t.layout(layout)
    type_keys(keys)
    for step in then:
        time.sleep(0.5)
        step()
    time.sleep(settle)
    return check(t.name, name, t.read(), expect)


undo = lambda: tap(CAPS, CTRL, SHIFT)       # noqa: E731
flip = lambda: tap(BACK, SHIFT)             # noqa: E731
accept = lambda: tap(CAPS, ALT)             # noqa: E731
select_word = lambda: tap(HOME, SHIFT)      # noqa: E731
convert_sel = lambda: tap(CAPS, SHIFT)      # noqa: E731


def text_sweep(t):
    print(f"\n=== {t.name}: Auto mode ===", flush=True)
    t.focus()
    check(t.name, "mode set to auto", str(set_mode("auto")), "True")
    run(t, "English sentence untouched", "the quick frontend codebase ", "the quick frontend codebase")
    run(t, "compounds stay English", "middleware workflow localhost ", "middleware workflow localhost")
    run(t, "Thai on EN layout (short)", "l;ylfu8iy[ ", "สวัสดีครับ")
    run(t, "Thai on EN layout (sentence)", "lj'wa]N,k.shsojvp ", "ส่งไฟล์มาให้หน่อย")
    run(t, "Thai then English", "l;ylfu8iy[ middleware ", "สวัสดีครับ middleware")
    run(t, "English then Thai", "hello l;ylfu8iy[ ", "hello สวัสดีครับ")
    run(t, "English on Thai layout", "correct ", "correct", layout=HKL_TH)
    run(t, "Thai typed natively on Thai layout", "l;ylfu ", "สวัสดี", layout=HKL_TH)
    run(t, "Undo after TH->EN fix", "correct ", "แนพพำแะ", layout=HKL_TH, then=[undo])
    # Undo is an explicit "I meant that": the word is now learned as Thai.
    run(t, "Undo teaches the word", "correct ", "แนพพำแะ", layout=HKL_TH)
    run(t, "Shift+Backspace after TH->EN fix", "hello ", "้ำสสน", layout=HKL_TH, then=[flip])
    run(t, "Undo after live Thai", "l;ylfu ", "l;ylfu", then=[undo])
    run(t, "Shift+Backspace after live Thai", "l;ylfu ", "l;ylfu", then=[flip])
    run(t, "Backspace inside a word", "hellp" , "hello", then=[lambda: (tap(BACK), type_keys("o"))])
    run(t, "typo stays English", "lsiten wroking ", "lsiten wroking")

    print(f"\n=== {t.name}: Manual mode ===", flush=True)
    t.focus()
    check(t.name, "mode set to manual", str(set_mode("manual")), "True")
    run(t, "Manual leaves text alone", "l;ylfu ", "l;ylfu")
    run(t, "Shift+Backspace mid-word", "l;ylfu", "สวัสดี", then=[flip])
    run(t, "Shift+Backspace after space", "l;ylfu ", "สวัสดี", then=[flip])
    run(t, "Shift+Backspace EN on TH layout", "correct", "correct", layout=HKL_TH, then=[flip])
    run(t, "Undo after manual flip", "l;ylfu ", "l;ylfu", then=[flip, undo])
    run(t, "flip then keep typing", "l;ylfu", "สวัสดีครับ", then=[flip, lambda: type_keys("8iy[")])
    run(t, "convert selection", "l;ylfu8iy[", "สวัสดีครับ", then=[select_word, convert_sel], settle=1.5)
    run(t, "undo selection conversion", "l;ylfu8iy[", "l;ylfu8iy[",
        then=[select_word, convert_sel, lambda: time.sleep(1.0), undo], settle=1.5)

    print(f"\n=== {t.name}: Suggest mode ===", flush=True)
    t.focus()
    check(t.name, "mode set to suggest", str(set_mode("suggest")), "True")
    run(t, "Suggest leaves text alone", "l;ylfu ", "l;ylfu")
    run(t, "Alt+CapsLock accepts", "l;ylfu ", "สวัสดี", then=[accept])
    run(t, "Suggest TH->EN accept", "world ", "world", layout=HKL_TH, then=[accept])
    run(t, "suggestion dropped after typing", "l;ylfu a", "l;ylfu a", then=[accept])
    run(t, "Undo after accept", "l;ylfu ", "l;ylfu", then=[accept, undo])

    print(f"\n=== {t.name}: master switch ===", flush=True)
    t.focus()
    set_mode("auto")
    tap(CAPS, CTRL, ALT)
    time.sleep(0.6)
    check(t.name, "panic switch turns OFF", config_value("enabled"), "false")
    run(t, "OFF: Auto does nothing", "l;ylfu ", "l;ylfu")
    t.focus()
    tap(CAPS, CTRL, ALT)
    time.sleep(0.6)
    check(t.name, "panic switch turns ON", config_value("enabled"), "true")
    run(t, "ON again: Auto works", "l;ylfu ", "สวัสดี")
    caps = user32.GetKeyState(CAPS) & 1
    check(t.name, "CapsLock never toggled by hotkeys", str(caps), "0")


def password_sweep(t):
    print(f"\n=== {t.name}: password field ===", flush=True)
    set_mode("auto")
    t.win.set_focus()
    t.pw.set_focus()
    time.sleep(1.0)
    t.layout(HKL_EN)
    pos = log_size()
    type_keys("l;ylfu ")
    time.sleep(1.0)
    touched = [l for l in log_since(pos).splitlines() if "reconcile" in l or "det=Some" in l]
    check(t.name, "password field is never analysed", str(len(touched)), "0")
    t.pw.set_focus()
    tap(ord("A"), CTRL)
    tap(DELETE)


def learning_sweep(t):
    print(f"\n=== {t.name}: learning ===", flush=True)
    set_mode("auto")
    run(t, "unknown token converted", "ohklt ", "น้าสะ")
    run(t, "Shift+Backspace reverts it", "ohklt ", "ohklt", then=[flip])
    run(t, "learned word now left alone", "ohklt ", "ohklt")
    learned = (DATA / "learned.txt").read_text(encoding="utf-8").split()
    check(t.name, "learned.txt has the word", str("ohklt" in learned), "True")


# --------------------------------------------------------------------------- UI


def find_window(title, timeout=8):
    deadline = time.time() + timeout
    while time.time() < deadline:
        for w in Desktop(backend="uia").windows(top_level_only=True):
            try:
                if w.window_text() == title and w.process_id() == RT.pid:
                    return w
            except Exception:
                pass
        time.sleep(0.4)
    return None


def ui_sweep():
    global RT
    print("\n=== UI windows ===", flush=True)
    # First run: the welcome window must appear, and dismissing it records it.
    write_config(mode="manual", learn=False, onboarded=False)
    RT = start_rt()
    w = find_window("Welcome to RightType")
    check("ui", "welcome window on first run", str(w is not None), "True")
    if w:
        texts = " | ".join(c.window_text() for c in w.descendants(control_type="Text"))
        check("ui", "welcome lists the Ctrl+CapsLock hotkey", str("Ctrl + CapsLock" in texts), "True")
        btn = [b for b in w.descendants(control_type="Button") if "start" in b.window_text().lower()]
        if btn:
            btn[0].click()
            time.sleep(1.0)
        check("ui", "Get started marks onboarded", config_value("onboarded"), "true")

    # Settings: change mode + learning, Apply persists and takes effect.
    RT = start_rt({"RIGHTTYPE_SHOW": "settings"})
    w = find_window("RightType — Settings")
    check("ui", "settings window opens", str(w is not None), "True")
    if w:
        radios = {r.window_text().split(" ")[0]: r for r in w.descendants(control_type="RadioButton")}
        check("ui", "settings shows current mode (Manual)", str(bool(radios["Manual"].is_selected())), "True")
        radios["Suggest"].click()
        boxes = {c.window_text(): c for c in w.descendants(control_type="CheckBox")}
        learn = boxes.get("Learn new words automatically")
        if learn and learn.get_toggle_state() == 0:
            learn.click()
        buttons = {b.window_text(): b for b in w.descendants(control_type="Button")}
        buttons["Apply"].click()
        time.sleep(1.0)
        check("ui", "Apply saves mode", config_value("mode"), "suggest")
        check("ui", "Apply saves learning", config_value("learn"), "true")
        radios["Auto"].click()
        buttons["Cancel"].click()
        time.sleep(1.0)
        check("ui", "Cancel discards changes", config_value("mode"), "suggest")
        check("ui", "Cancel closes the window", str(find_window("RightType — Settings", 1) is None), "True")

    RT = start_rt({"RIGHTTYPE_SHOW": "stats"})
    w = find_window("RightType — Statistics")
    check("ui", "stats window opens", str(w is not None), "True")
    if w:
        w.close()


# --------------------------------------------------------------------------- main

RT = None


def main():
    global RT
    want = set(sys.argv[1:]) or {"chrome", "claude", "ui"}
    write_config()
    RT = start_rt()
    try:
        if "chrome" in want:
            t = Chrome()
            try:
                text_sweep(t)
                password_sweep(t)
                learning_sweep(t)
            finally:
                t.close()
        if "claude" in want:
            (DATA / "learned.txt").write_text("", encoding="utf-8")
            write_config()
            RT = start_rt()
            t = Claude()
            try:
                text_sweep(t)
                learning_sweep(t)
            finally:
                t.close()
        if "ui" in want:
            ui_sweep()
    finally:
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
        # Hand the desktop back with the operator's own (release) instance.
        release = lib.RELEASE_EXE
        if release.exists():
            subprocess.Popen([str(release)])
    passed = sum(r[2] for r in RESULTS)
    print(f"\nSUMMARY {passed}/{len(RESULTS)} passed; trace: {LOG}")
    for r in RESULTS:
        if not r[2]:
            print(f"  FAIL {r[0]}: {r[1]} — got {r[3].strip()!r} expected {r[4].strip()!r}")


main()

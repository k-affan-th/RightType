"""Claude desktop composer E2E (run: `uv run python claude_composer.py` from e2e/,
after `cargo build --features winos`, with the Claude composer empty and hands off).

Drive the real RightType hook in the Claude desktop composer with physical
virtual-key events (not VK_PACKET), so the layout switch after an anchor makes
the remaining keys arrive as native Thai, exactly like a human typist.
Never sends Enter: the composer would submit."""
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import tempfile
import time
from pathlib import Path

from pywinauto import Desktop

REPO = Path(r"C:\Users\kaffa\Documents\GitHub\RightType")
EXE = REPO / "target" / "debug" / "righttype.exe"
SCRATCH = Path(tempfile.mkdtemp(prefix="rt_claude_"))
DATA = SCRATCH / "rtdata"
LOG = SCRATCH / "rt_claude.log"
DATA.mkdir()
# Isolated config: Auto mode, learning on, no onboarding window.
(DATA / "config.toml").write_text(
    'enabled = true\nmode = "auto"\nlearn = true\ncustom_blacklist = []\nonboarded = true\n',
    encoding="utf-8",
)

user32 = ctypes.windll.user32
INPUT_KEYBOARD = 1
KEYEVENTF_KEYUP = 0x2


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wt.WORD), ("wScan", wt.WORD), ("dwFlags", wt.DWORD),
                ("time", wt.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class _U(ctypes.Union):
    _fields_ = [("ki", KEYBDINPUT), ("pad", ctypes.c_byte * 32)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wt.DWORD), ("u", _U)]


def key(vk, up=False):
    scan = user32.MapVirtualKeyW(vk, 0)
    i = INPUT(type=INPUT_KEYBOARD)
    i.u.ki = KEYBDINPUT(vk, scan, KEYEVENTF_KEYUP if up else 0, 0, 0)
    user32.SendInput(1, ctypes.byref(i), ctypes.sizeof(INPUT))


def tap(vk, *mods):
    for m in mods:
        key(m)
    key(vk)
    key(vk, True)
    for m in reversed(mods):
        key(m, True)
    time.sleep(0.07)


VK_SHIFT, VK_CONTROL, VK_BACK, VK_SPACE, VK_CAPITAL, VK_DELETE = 0x10, 0x11, 0x08, 0x20, 0x14, 0x2E
PUNCT = {";": 0xBA, "=": 0xBB, ",": 0xBC, "-": 0xBD, ".": 0xBE, "/": 0xBF,
         "`": 0xC0, "[": 0xDB, "\\": 0xDC, "]": 0xDD, "'": 0xDE}
SHIFTED = {":": ";", "+": "=", "<": ",", "_": "-", ">": ".", "?": "/", "{": "[", "}": "]",
           '"': "'", "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6",
           "&": "7", "*": "8", "(": "9", ")": "0"}


def type_keys(s):
    """Physical keys named by their US character; ' ' is a real VK_SPACE."""
    for ch in s:
        if ch == " ":
            tap(VK_SPACE)
        elif ch.isalpha():
            if ch.isupper():
                tap(ord(ch), VK_SHIFT)
            else:
                tap(ord(ch.upper()))
        elif ch.isdigit():
            tap(ord(ch))
        elif ch in PUNCT:
            tap(PUNCT[ch])
        elif ch in SHIFTED:
            base = SHIFTED[ch]
            tap(PUNCT.get(base, ord(base)), VK_SHIFT)
        else:
            raise ValueError(ch)


def claude_prompt():
    for w in Desktop(backend="uia").windows(top_level_only=True):
        try:
            if w.window_text() != "Claude":
                continue
            for d in w.descendants(control_type="Edit"):
                if d.element_info.name == "Prompt":
                    return w, d
        except Exception:
            continue
    raise SystemExit("Claude prompt box not found")


def read(prompt):
    try:
        v = prompt.iface_value.CurrentValue
        if v is not None:
            return v
    except Exception:
        pass
    return prompt.window_text()


def set_layout(hwnd, hkl):
    user32.PostMessageW(hwnd, 0x0050, 0, hkl)
    time.sleep(0.4)


def clear(prompt):
    tap(ord("A"), VK_CONTROL)
    tap(VK_DELETE)
    time.sleep(0.3)


def main():
    win, prompt = claude_prompt()
    before = read(prompt).strip()
    if before:
        raise SystemExit(f"composer already has a draft ({len(before)} chars) - not touching it")

    subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
    time.sleep(0.8)
    env = {**os.environ, "RIGHTTYPE_E2E_ACCEPT_INJECTED": "1", "RIGHTTYPE_E2E_DATA_DIR": str(DATA)}
    proc = subprocess.Popen([str(EXE)], env=env, stderr=open(LOG, "w", encoding="utf-8"))
    time.sleep(2.5)
    assert proc.poll() is None, "debug app exited"

    win.set_focus()
    prompt.set_focus()
    time.sleep(0.4)
    hwnd = win.handle
    caps_before = user32.GetKeyState(VK_CAPITAL) & 1

    results = []

    def case(name, keys, expect, after=None):
        clear(prompt)
        set_layout(hwnd, 0x04090409)
        type_keys(keys)
        if after:
            after()
        time.sleep(0.8)
        got = read(prompt).rstrip("\n")
        ok = got.strip() == expect.strip()
        results.append((name, ok, got, expect))
        print(f"[{'PASS' if ok else 'FAIL'}] {name}: got {got!r} expected {expect!r}", flush=True)

    case("compound middleware stays English", "middleware ", "middleware")
    case("compound workflow stays English", "workflow ", "workflow")
    case("English sentence untouched", "the quick frontend codebase ", "the quick frontend codebase")
    case("Thai on EN layout: sawasdee khrap", "l;ylfu8iy[ ", "สวัสดีครับ")
    case("Thai on EN layout: long sentence", "lj'wa]N,k.shsojvp ", "ส่งไฟล์มาให้หน่อย")
    case("Thai on EN layout: wan nee wan jan", ";yoouh;yo0yomiN ", "วันนี้วันจันทร์")
    case("Thai then English in one line", "l;ylfu8iy[ middleware ", "สวัสดีครับ middleware")
    # Learning: an unknown token that reads as Thai gets converted...
    case("unknown token converted", "ohklt ", "น้าสะ")
    # ...Shift+Backspace right after flips it back and learns it...
    clear(prompt)
    set_layout(hwnd, 0x04090409)
    type_keys("ohklt ")
    time.sleep(0.5)
    tap(VK_BACK, VK_SHIFT)
    time.sleep(0.8)
    got = read(prompt).strip()
    ok = got == "ohklt"
    results.append(("Shift+Backspace reverts auto-fix", ok, got, "ohklt"))
    print(f"[{'PASS' if ok else 'FAIL'}] Shift+Backspace reverts auto-fix: got {got!r}", flush=True)
    time.sleep(0.5)
    case("learned token now left alone", "ohklt ", "ohklt")

    # Mode hotkey inside Claude: three presses cycle back to Auto, CapsLock untouched.
    clear(prompt)
    modes = []
    for _ in range(3):
        tap(VK_CAPITAL, VK_CONTROL)
        time.sleep(0.6)
        cfg = (DATA / "config.toml").read_text(encoding="utf-8")
        modes.append([l for l in cfg.splitlines() if l.startswith("mode")][0])
    caps_after = user32.GetKeyState(VK_CAPITAL) & 1
    ok = modes == ['mode = "suggest"', 'mode = "manual"', 'mode = "auto"'] and caps_after == caps_before
    results.append(("Ctrl+CapsLock cycles mode in Claude", ok, modes, "suggest/manual/auto"))
    print(f"[{'PASS' if ok else 'FAIL'}] Ctrl+CapsLock in Claude: {modes} caps {caps_before}->{caps_after}", flush=True)

    clear(prompt)
    set_layout(hwnd, 0x04090409)
    learned = (DATA / "learned.txt").read_text(encoding="utf-8").split()
    print("learned.txt:", learned)
    print(f"SUMMARY {sum(r[1] for r in results)}/{len(results)} passed; trace: {LOG}")
    proc.terminate()


main()

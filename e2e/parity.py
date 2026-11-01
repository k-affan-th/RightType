"""Do the replay (`sim::Engine`) and the real hook agree? (accuracy plan P2)

    cargo build --features winos                    # a debug build: only it accepts SendInput keys
    cargo run --release --example parity_sim > sim.tsv
    python e2e/parity.py sim.tsv [first [last]]

Types every sequence of examples/data/parity_sequences.txt into a fresh Notepad
document with the debug build running, and compares what Notepad holds with what
the replay says. A difference is either a bug in the replay, a bug in the hook, or
something the replay does not model (listed in docs/TYPING_BENCHMARK.md): look at
each before changing anything. Takes the keyboard for about two minutes: keep your
hands off it. See e2e/lib.py for why the hook needs a debug build for this.

Sequences use the key language of examples/parity_sim.rs.
"""

import ctypes
import os
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(__file__))
import lib  # noqa: E402

user32 = ctypes.windll.user32
SPECIAL = {"SP": 0x20, "ENTER": 0x0D, "TAB": 0x09, "BS": 0x08, "LEFT": 0x25, "HOME": 0x24, "ESC": 0x1B}
PUNCT = {",": 0xBC, ".": 0xBE, "/": 0xBF, ";": 0xBA, "'": 0xDE, "[": 0xDB, "]": 0xDD, "-": 0xBD, "=": 0xBB}
SHIFTED = {":": ";", "?": "/", '"': "'", "!": "1", "@": "2", "%": "5", "(": "9", ")": "0", "_": "-"}
# Windows 11 Notepad's own autocorrect changes these words with RightType not
# running at all (checked 2026-11-02: `github kubernetes cmoputer recieve` typed with
# RightType stopped come out `GitHub Kubernetes computer receive`). They are in the
# sequences because RightType must leave them alone, but what Notepad holds is
# not the replay's business, so a difference on exactly these is not reported.
NOTEPAD_AUTOCORRECTS = {"github{SP}", "kubernetes{SP}", "cmoputer{SP}", "recieve{SP}"}
VK_SHIFT = 0x10
VK_CONTROL = 0x11


def press(vk, shift=False):
    sc = user32.MapVirtualKeyW(vk, 0)
    if shift:
        user32.keybd_event(VK_SHIFT, user32.MapVirtualKeyW(VK_SHIFT, 0), 0, 0)
    user32.keybd_event(vk, sc, 0, 0)
    user32.keybd_event(vk, sc, 2, 0)
    if shift:
        user32.keybd_event(VK_SHIFT, user32.MapVirtualKeyW(VK_SHIFT, 0), 2, 0)


def vk_of(ch):
    """(virtual key, shift held) for a character as typed on US QWERTY."""
    if ch.isalpha():
        return ord(ch.upper()), ch.isupper()
    if ch.isdigit():
        return ord(ch), False
    if ch in PUNCT:
        return PUNCT[ch], False
    base = SHIFTED[ch]
    return vk_of(base)[0], True


def play(sequence, gap=0.04):
    rest = sequence
    while rest:
        if rest[0] == "{":
            end = rest.index("}")
            press(SPECIAL[rest[1:end]])
            rest = rest[end + 1 :]
        else:
            vk, shift = vk_of(rest[0])
            press(vk, shift)
            rest = rest[1:]
        time.sleep(gap)


def clear(window):
    window.set_focus()
    sc = user32.MapVirtualKeyW(VK_CONTROL, 0)
    user32.keybd_event(VK_CONTROL, sc, 0, 0)
    press(0x41)
    user32.keybd_event(VK_CONTROL, sc, 2, 0)
    time.sleep(0.1)
    press(0x2E)
    time.sleep(0.3)


def main():
    sim = {}
    for line in open(sys.argv[1], encoding="utf-8"):
        idx, seq, screen = line.rstrip("\n").split("\t", 2)
        sim[int(idx)] = (seq, screen.replace("\\n", "\n"))
    first = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    last = int(sys.argv[3]) if len(sys.argv) > 3 else len(sim) - 1
    if not lib.EXE.exists():
        raise SystemExit(f"missing {lib.EXE}: cargo build --features winos")
    subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
    time.sleep(0.8)
    data = tempfile.mkdtemp(prefix="rt-parity-")
    env = {**lib.E2E_ENV, "RIGHTTYPE_E2E_DATA_DIR": data}
    app_proc = subprocess.Popen([str(lib.EXE)], env=env)
    time.sleep(4)
    doc = os.path.join(tempfile.mkdtemp(prefix="rt-parity-doc-"), "t.txt")
    open(doc, "w").close()
    app = lib.start_notepad(doc)
    window = app.top_window()
    same = differ = skipped = 0
    try:
        for idx in range(first, last + 1):
            seq, expected = sim[idx]
            clear(window)
            lib.set_layout(window.handle, "en")
            time.sleep(0.3)
            play(seq)
            time.sleep(0.8)
            got = lib.read_text_value(app).replace("\r\n", "\n")
            if got.rstrip() == expected.rstrip():
                same += 1
            elif seq in NOTEPAD_AUTOCORRECTS:
                skipped += 1
            else:
                differ += 1
                print(f"DIFF {idx:3} {seq}\n       replay {expected!r}\n       hook   {got!r}", flush=True)
    finally:
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
        subprocess.run(["taskkill", "/IM", "notepad.exe", "/F"], capture_output=True)
        app_proc.poll()
    print(f"{same} sequences agree, {differ} differ, {skipped} differ only by Notepad's autocorrect")
    sys.exit(1 if differ else 0)


if __name__ == "__main__":
    main()

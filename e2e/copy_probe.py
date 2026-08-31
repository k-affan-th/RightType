"""Does Ctrl+A / Ctrl+C work in the page at all, with RightType NOT running?"""
import subprocess, sys, time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import lib
from d008_revision import BROWSERS, clear_field, find_target_window, kill_edge, start_edge_additive
sys.stdout.reconfigure(encoding="utf-8")
HERE = Path("C:/Users/kaffa/Documents/GitHub/RightType/.claude/worktrees/multilingual-keyboard-bug-41706e/e2e")

def clip():
    o = subprocess.run(["powershell","-NoProfile","-Command","Get-Clipboard -Raw"],capture_output=True,text=True)
    return (o.stdout or "").strip()

b = h = None
try:
    subprocess.run(["cmd","/c","echo sentinel-before| clip"],capture_output=True)
    _a, h, b = start_edge_additive(HERE/"target.html", BROWSERS[sys.argv[1]])
    win = find_target_window()
    clear_field(win, None)
    win.type_keys("l;ylfu", with_spaces=True, pause=0.05)
    time.sleep(0.6)
    win.type_keys("^a", pause=0.08); time.sleep(0.5)
    win.type_keys("^c", pause=0.08); time.sleep(1.2)
    print("no-righttype ctrl+c ->", repr(clip()))
finally:
    if b: kill_edge(b)
    if h: h.shutdown()

"""Native password-field guard: WinForms ES_PASSWORD box must be hard-denied."""

import subprocess
import sys
import threading
import time
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")
HERE = Path(__file__).resolve().parent
LOG = HERE / "rt_stderr.log"

WINFORMS_PS = r"""
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.Form
$f.Text = 'rt-native-pw'
$f.Size = '500,200'
$tb = New-Object System.Windows.Forms.TextBox
$tb.UseSystemPasswordChar = $true
$tb.Font = 'Consolas 20'
$tb.Dock = 'Fill'
$f.Controls.Add($tb)
$f.Topmost = $true
[void]$f.ShowDialog()
"""

target = subprocess.Popen(
    ["powershell.exe", "-NoProfile", "-STA", "-Command", WINFORMS_PS],
    creationflags=subprocess.CREATE_NEW_CONSOLE,
)
time.sleep(4.0)

from pywinauto import Desktop

win = Desktop(backend="uia").window(title_re="rt-native-pw", top_level_only=True)
win.wait("visible ready", timeout=15)
win.set_focus()
time.sleep(0.5)
edit = win.child_window(control_type="Edit")
edit.type_keys("l;ylfu ", pause=0.05)
time.sleep(2.5)

trace = LOG.read_text(encoding="utf-8", errors="replace")
dets = trace.count("det=Some") - 0
value_masked = True
print("native pw typed; detections in trace:", dets)

subprocess.run(["taskkill", "/PID", str(target.pid), "/F"], capture_output=True)
ok = dets == 0
print({"case": "guard_native_es_password", "pass": ok})
raise SystemExit(0 if ok else 1)

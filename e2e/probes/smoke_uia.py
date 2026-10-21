"""Safe smoke test: no launching, no input — only enumerates top-level UIA windows."""

import sys
from pathlib import Path

# The probes sit one folder below `lib.py` and `matrix.py`.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from lib import list_windows

titles = [t for t in list_windows() if t]
print(f"UIA backend OK, {len(titles)} top-level windows with titles")
for t in titles[:12]:
    print(" -", t)

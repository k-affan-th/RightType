"""Safe smoke test: no launching, no input — only enumerates top-level UIA windows."""

from lib import list_windows

titles = [t for t in list_windows() if t]
print(f"UIA backend OK, {len(titles)} top-level windows with titles")
for t in titles[:12]:
    print(" -", t)

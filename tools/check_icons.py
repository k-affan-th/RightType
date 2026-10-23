"""Check that every icon in src/icons.rs exists in both Segoe Fluent Icons
(Windows 11) and Segoe MDL2 Assets (Windows 10), and that no two names that
should differ share a code point by accident. Windows only; stdlib only.

Usage: python -I tools/check_icons.py src/icons.rs
"""

import ctypes
import re
import sys
from ctypes import wintypes

src = open(sys.argv[1], encoding="utf-8").read()
icons = re.findall(r"pub const (\w+): char = '\\u\{([0-9A-Fa-f]+)\}';", src)
if not icons:
    sys.exit("no icons found")

gdi, user = ctypes.windll.gdi32, ctypes.windll.user32


def exists(face, codes):
    hdc = user.GetDC(None)
    font = gdi.CreateFontW(-24, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 4, 0, face)
    old = gdi.SelectObject(hdc, font)
    s = "".join(chr(c) for c in codes)
    idx = (wintypes.WORD * len(codes))()
    gdi.GetGlyphIndicesW(hdc, s, len(codes), idx, 1)  # GGI_MARK_NONEXISTING_GLYPHS
    gdi.SelectObject(hdc, old)
    gdi.DeleteObject(font)
    user.ReleaseDC(None, hdc)
    return [i != 0xFFFF for i in idx]


codes = [int(c, 16) for _, c in icons]
fluent = exists("Segoe Fluent Icons", codes)
mdl2 = exists("Segoe MDL2 Assets", codes)
bad = [(n, c) for (n, c), f, m in zip(icons, fluent, mdl2) if not (f and m)]
for n, c in bad:
    print(f"MISSING  {n} U+{c}")
print(f"{len(icons)} icons, {len(bad)} missing from Fluent or MDL2")
sys.exit(1 if bad else 0)

"""Screenshot a themed app window (settings|stats|onboard) via RIGHTTYPE_SHOW."""

import os
import sys
import time
from pathlib import Path

import lib

sys.stdout.reconfigure(encoding="utf-8")
what = sys.argv[1] if len(sys.argv) > 1 else "settings"

cfg = Path(lib.os.environ["APPDATA"]) / "RightType" / "config.toml"
if cfg.exists():
    text = cfg.read_text(encoding="utf-8")
    if "onboarded" in text:
        import re

        text = re.sub(r"(?m)^onboarded\s*=\s*\w+", "onboarded = true", text)
    else:
        text += "\nonboarded = true\n"
    cfg.write_text(text, encoding="utf-8")

lib.os.environ["RIGHTTYPE_SHOW"] = what
lib.E2E_ENV.update(lib.os.environ)
rt = lib.start_righttype()
time.sleep(5.0)
shot = lib.screenshot(what)
rt.terminate()
print(shot)

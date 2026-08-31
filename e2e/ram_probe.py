"""Runtime evidence for release-checklist row 3.10.

`harden_process` and `lock_region` are best-effort by design and report
nothing, so "the controls are active" was previously an assertion about source
code rather than about a running process. Under the E2E flag they now say what
Windows actually returned; this starts the real binary and reads that back.

Usage: uv run ram_probe.py
"""

import sys
import time
from pathlib import Path

import lib

sys.stdout.reconfigure(encoding="utf-8")
LOG = Path(__file__).resolve().parent / "rt_ram.log"


def main():
    LOG.unlink(missing_ok=True)
    proc = lib.start_righttype(stderr_log=str(LOG))
    try:
        time.sleep(3.0)
    finally:
        proc.terminate()
        time.sleep(1.0)

    lines = [
        ln for ln in LOG.read_text(encoding="utf-8", errors="replace").splitlines()
        if "ram:" in ln
    ]
    for ln in lines:
        print(ln)
    wer = any("WerSetFlags(NOHEAP) -> true" in ln for ln in lines)
    lock = any("VirtualLock(" in ln and "-> true" in ln for ln in lines)
    err = any("SetErrorMode applied" in ln for ln in lines)
    print(f"\nRAM: SetErrorMode={err} WerSetFlags={wer} VirtualLock={lock}")
    raise SystemExit(0 if (wer and lock and err) else 1)


if __name__ == "__main__":
    main()

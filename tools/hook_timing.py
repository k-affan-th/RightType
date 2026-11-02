#!/usr/bin/env python3
"""Summarise what a `hook-timing` build of RightType wrote (docs/HOOK_TIMING.md).

    python tools/hook_timing.py hook.txt [more.txt ...]

Each file holds one duration in nanoseconds per line: every call of the
keyboard hook, and (in `<file>.space`) the calls for a typed Space. A key
waits for the hook, so the tail (p99, max) matters more than the median.
"""
import sys
from pathlib import Path


def summary(path: Path) -> str:
    try:
        values = sorted(int(x) for x in path.read_text().split())
    except OSError:
        return f"{path}: not found"
    if not values:
        return f"{path}: empty"

    def q(p: float) -> float:
        return values[min(len(values) - 1, int(len(values) * p))] / 1000

    return (
        f"{path}: n={len(values)}  p50 {q(.5):.0f} us  p99 {q(.99):.0f} us  "
        f"p99.9 {q(.999):.0f} us  max {values[-1] / 1000:.0f} us"
    )


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    for name in sys.argv[1:]:
        base = Path(name)
        print(summary(base))
        print(summary(base.with_name(base.name + ".space")))


if __name__ == "__main__":
    main()

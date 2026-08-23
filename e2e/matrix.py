"""Persistent-session E2E matrix: one RightType launch per mode-group, many cases.

Sessions:
  AUTO    — boundary corrections both directions, fast typing, undo hotkey
  SUGGEST — hint stays non-destructive until Alt+CapsLock accepts
  GUARD   — password field + blacklisted terminal never correct (trace-asserted)

Usage: uv run matrix.py            (full report)
       uv run matrix.py --session auto
"""

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

from pywinauto import Desktop, mouse

import lib

sys.stdout.reconfigure(encoding="utf-8")
HERE = Path(__file__).resolve().parent
LOG = HERE / "rt_stderr.log"
APPDATA_CONFIG = Path(lib.os.environ["APPDATA"]) / "RightType" / "config.toml"


def set_config_mode(mode: str):
    text = APPDATA_CONFIG.read_text(encoding="utf-8") if APPDATA_CONFIG.exists() else ""
    import re

    if re.search(r'(?m)^mode\s*=', text):
        text = re.sub(r'(?m)^mode\s*=\s*"[a-z]+"', f'mode = "{mode}"', text)
    else:
        text = f'mode = "{mode}"\n' + text
    if "enabled" not in text:
        text = "enabled = true\n" + text
    APPDATA_CONFIG.write_text(text, encoding="utf-8")


def start_rt_traced():
    return lib.start_righttype(stderr_log=LOG)


def trace_lines() -> list[str]:
    if not LOG.exists():
        return []
    return LOG.read_text(encoding="utf-8", errors="replace").splitlines()


def click_element(win, control_type: str, pick: str):
    cands = []
    for d in win.descendants(control_type=control_type):
        try:
            r = d.rectangle()
            area = max(0, r.width()) * max(0, r.height())
        except Exception:
            continue
        if area < 5000:
            continue
        cands.append((area, r))
    if not cands:
        raise SystemExit(f"no {control_type} found to click")
    if pick == "largest":
        _, r = max(cands, key=lambda t: t[0])
    else:
        _, r = max(cands, key=lambda t: t[1].top)
    mouse.click(button="left", coords=((r.left + r.right) // 2, (r.top + r.bottom) // 2))


def type_token(app, token: str, lang: str, pause: float = 0.03):
    win = app.top_window()
    lib.set_layout(win.handle, lang)
    win.set_focus()
    time.sleep(0.3)
    win.type_keys("{F5}")
    time.sleep(1.5)
    click_element(win, "Edit", "largest")
    time.sleep(0.3)
    win.type_keys(token, with_spaces=True, pause=pause)
    time.sleep(0.15)
    win.type_keys("{SPACE}", pause=0.02)
    time.sleep(2.5)
    return app


def read_out(app) -> str:
    return lib.read_text_value(app, largest=True, edits_only=True)


def case_result(name, got, want, ok=None):
    return {"case": name, "actual": got, "expected": want, "pass": (got.strip() == want if ok is None else ok)}


def _thai_only(s: str) -> str:
    return "".join(c for c in s if "\u0e00" <= c <= "\u0e7f")


def type_live(app, us_token: str, thai_word: str, pause: float = 0.06):
    """D-006 human emulation: type US-layout chars; once RightType commits and
    switches the layout, continue with the remaining *Thai* glyphs natively."""
    win = app.top_window()
    lib.set_layout(win.handle, "en")
    win.set_focus()
    time.sleep(0.3)
    win.type_keys("{F5}")
    time.sleep(1.5)
    click_element(win, "Edit", "largest")
    time.sleep(0.3)
    for ch in us_token:
        win.type_keys(ch, pause=0)
        time.sleep(pause)
        thai_part = _thai_only(read_out(app))
        if thai_part:
            remainder = thai_word[len(thai_part):] if thai_word.startswith(thai_part) else ""
            if remainder:
                win.type_keys(remainder, pause=0)
                time.sleep(1.5)
            time.sleep(0.8)
            return read_out(app)
    time.sleep(2.0)
    return read_out(app)


def session_auto(app):
    results = []

    def out():
        return read_out(app)

    type_token(app, "แนพพำแะ", "th")
    results.append(case_result("then_basic_boundary", out(), "correct"))

    got = type_live(app, "l;ylfu", "สวัสดี")
    results.append(case_result("enth_live_full", got.strip(), "สวัสดี"))

    got = type_live(app, "dy[", "กับ")
    results.append(case_result("enth_live_bracket", got.strip(), "กับ"))

    got = type_live(app, "l;ylfu", "สวัสดี", pause=0.001)
    results.append(case_result("fast_typing_live", got.strip(), "สวัสดี"))

    # Undo: fresh live correction, then Ctrl+Shift+CapsLock restores the raw token.
    type_live(app, "dy[", "กับ")
    mid = out()
    app.top_window().set_focus()
    time.sleep(0.3)
    app.top_window().type_keys("^+{CAPSLOCK}")
    time.sleep(2.0)
    after = out()
    ok = mid.strip() == "กับ" and after.strip() == "dy["
    results.append(
        {
            "case": "undo_after_correction",
            "actual": f"mid={mid!r} after={after!r}",
            "expected": "mid='กับ' after='dy['",
            "pass": ok,
        }
    )
    return results


def session_suggest():
    set_config_mode("suggest")
    rt = start_rt_traced()
    time.sleep(2.0)
    app, httpd = lib.start_edge(HERE / "target.html")
    results = []
    try:
        # Suggest stays boundary-driven in v1: the hint forms at whitespace.
        type_token(app, "l;ylfu", "en")
        untouched = read_out(app)
        results.append(case_result("suggest_no_touch", untouched.strip(), "l;ylfu"))
        win = app.top_window()
        win.set_focus()
        time.sleep(0.3)
        win.type_keys("%{CAPSLOCK}")
        time.sleep(2.5)
        results.append(case_result("suggest_accept", read_out(app), "สวัสดี"))
    finally:
        lib.close_app(app)
        httpd.shutdown()
        rt.terminate()
        set_config_mode("auto")
    return results


def session_guard():
    set_config_mode("auto")
    rt = start_rt_traced()
    time.sleep(2.0)
    app, httpd = lib.start_edge(HERE / "target.html")
    ps = None
    results = []
    try:
        base_det = sum(1 for l in trace_lines() if "det=" in l)

        # Password field: focus it, type a wrong-layout token — must stay untouched.
        win = app.top_window()
        win.set_focus()
        time.sleep(0.4)
        click_element(win, "Edit", "bottom")
        time.sleep(0.4)
        win.type_keys("l;ylfu", with_spaces=True, pause=0.03)
        time.sleep(0.15)
        win.type_keys("{SPACE}", pause=0.02)
        time.sleep(2.5)
        new_det = sum(1 for l in trace_lines() if "det=" in l)
        results.append(
            {
                "case": "guard_password_field",
                "actual": f"detections={new_det - base_det}",
                "expected": "detections=0",
                "pass": new_det == base_det,
            }
        )

        # Blacklisted terminal: same assertion inside PowerShell window.
        ps = subprocess.Popen(
            ["powershell.exe", "-NoProfile", "-Command", "$host.UI.RawUI.WindowTitle='rt-e2e-ps'; Start-Sleep 120"],
            creationflags=subprocess.CREATE_NEW_CONSOLE,
        )
        time.sleep(2.5)
        base_det = sum(1 for l in trace_lines() if "det=" in l)
        ps_win = Desktop(backend="uia").window(title_re=".*rt-e2e-ps.*", top_level_only=True)
        ps_win.set_focus()
        time.sleep(0.6)
        ps_win.type_keys("l;ylfu", with_spaces=True, pause=0.03)
        time.sleep(0.15)
        ps_win.type_keys("{SPACE}", pause=0.02)
        time.sleep(2.5)
        new_det = sum(1 for l in trace_lines() if "det=" in l)
        results.append(
            {
                "case": "guard_blacklisted_terminal",
                "actual": f"detections={new_det - base_det}",
                "expected": "detections=0",
                "pass": new_det == base_det,
            }
        )
    finally:
        lib.close_app(app)
        httpd.shutdown()
        rt.terminate()
        if ps:
            subprocess.run(["taskkill", "/PID", str(ps.pid), "/F"], capture_output=True)
    return results


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--session", choices=["auto", "suggest", "guard"])
    args = ap.parse_args()

    all_results = []
    if args.session in (None, "auto"):
        set_config_mode("auto")
        rt = start_rt_traced()
        time.sleep(2.0)
        app, httpd = lib.start_edge(HERE / "target.html")
        try:
            all_results += session_auto(app)
        finally:
            lib.close_app(app)
            httpd.shutdown()
            rt.terminate()
    if args.session in (None, "suggest"):
        all_results += session_suggest()
    if args.session in (None, "guard"):
        all_results += session_guard()

    passed = sum(1 for r in all_results if r["pass"])
    for r in all_results:
        print(("PASS " if r["pass"] else "FAIL "), json.dumps(r, ensure_ascii=False))
    print(f"MATRIX: {passed}/{len(all_results)} PASS")
    raise SystemExit(0 if passed == len(all_results) else 1)


if __name__ == "__main__":
    main()

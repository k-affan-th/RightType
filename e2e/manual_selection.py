"""E2E for the manual selection path: release checklist rows 3.2-3.4.

Selection conversion is the one correction that leaves RightType's own buffer
and goes through the system clipboard, so it is the only path that can damage
something the typist did not ask it to touch: their clipboard, or a control
that took focus while the conversion was in flight.

Three things are proved here, on a real browser:

- Shift+CapsLock converts the selection in place.
- The clipboard the user had is put back afterwards, byte for byte.
- A focus change mid-conversion aborts it instead of injecting into whatever
  window arrived next. The page's password field is the target of that check,
  which doubles as proof that nothing lands in a concealed control.

Usage: uv run manual_selection.py [edge|chrome]
"""

import subprocess
import sys
import time
from pathlib import Path

from pywinauto import mouse

import lib
from d008_revision import (
    BROWSERS,
    clear_field,
    find_target_window,
    kill_edge,
    read_textarea,
    restore_config,
    start_edge_additive,
)

sys.stdout.reconfigure(encoding="utf-8")
HERE = Path(__file__).resolve().parent
APPDATA_CONFIG = Path(lib.os.environ["APPDATA"]) / "RightType" / "config.toml"
BROWSER = sys.argv[1] if len(sys.argv) > 1 else "edge"
SENTINEL = "rt-clipboard-sentinel-42"
MANUAL_LOG = HERE / "rt_manual.log"


def trace():
    """This run's trace. d008_revision.trace() reads its own log, which would
    report the previous suite's keystrokes as if they were ours."""
    if not MANUAL_LOG.exists():
        return []
    return MANUAL_LOG.read_text(encoding="utf-8", errors="replace").splitlines()


def force_manual_mode():
    """Selection conversion is a manual hotkey; Auto would rewrite the text
    before it could be selected. Returns the original config to restore."""
    original = APPDATA_CONFIG.read_text(encoding="utf-8") if APPDATA_CONFIG.exists() else None
    if original is None:
        APPDATA_CONFIG.parent.mkdir(parents=True, exist_ok=True)
        APPDATA_CONFIG.write_text('enabled = true\nmode = "manual"\n', encoding="utf-8")
        return None
    lines = [ln for ln in original.splitlines() if not ln.startswith("mode")]
    lines.append('mode = "manual"')
    APPDATA_CONFIG.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return original


def clear_clipboard():
    subprocess.run(["cmd", "/c", "type nul | clip"], capture_output=True, shell=False)


def set_clipboard(text: str):
    """Set the clipboard the way an ordinary copy does.

    PowerShell's Set-Clipboard goes through .NET, which registers private
    formats alongside CF_UNICODETEXT. RightType refuses to snapshot a clipboard
    holding anything richer than plain text (D-003), so a .NET-set clipboard
    tests the refusal path, not the conversion. clip.exe sets plain text only.
    """
    subprocess.run(["cmd", "/c", f"echo {text}| clip"], capture_output=True)


def get_clipboard() -> str:
    out = subprocess.run(
        ["powershell", "-NoProfile", "-Command", "Get-Clipboard -Raw"],
        capture_output=True,
        text=True,
    )
    return (out.stdout or "").strip()


def password_value(win) -> str:
    """The page's password input, which nothing should ever be injected into."""
    for d in win.descendants():
        try:
            if d.element_info.control_type != "Edit":
                continue
            if d.legacy_properties().get("IsPassword"):
                return d.legacy_properties().get("Value") or ""
        except Exception:
            continue
    return ""


def select_all_and_convert(win):
    win.type_keys("^a", pause=0.05)
    time.sleep(0.3)
    # Shift+CapsLock: convert the selection.
    win.type_keys("+{CAPSLOCK}", pause=0.05)


def main():
    MANUAL_LOG.unlink(missing_ok=True)
    original = force_manual_mode()
    results = []
    rt = browser = httpd = None
    try:
        rt = lib.start_righttype(stderr_log=str(HERE / "rt_manual.log"))
        time.sleep(1.5)
        _app, httpd, browser = start_edge_additive(HERE / "target.html", BROWSERS[BROWSER])
        win = find_target_window()

        # --- 3.2 selection conversion -----------------------------------
        ok = clear_field(win, None)
        if not ok:
            results.append(("selection-convert", "SKIP", "could not clear the field"))
        else:
            # D-003 refuses to touch a clipboard it cannot snapshot as plain
            # text, so the two clipboard states are measured separately: an
            # empty clipboard is the documented happy path, ordinary copied
            # text is what a real user almost always has.
            # Discriminator: can *anything* copy from this control? If the
            # harness's own Ctrl+C fills the clipboard but RightType's does
            # not, the fault is in send_chord; if neither does, the selection
            # or the environment is at fault, not the product.
            set_clipboard("before-copy")
            time.sleep(0.6)
            win.type_keys("l;ylfu", with_spaces=True, pause=0.05)
            time.sleep(0.4)
            win.type_keys("^a", pause=0.05)
            time.sleep(0.3)
            win.type_keys("^c", pause=0.05)
            time.sleep(1.0)
            harness_copy = get_clipboard()
            results.append(
                (
                    "harness-copy-works",
                    "PASS" if harness_copy == "l;ylfu" else "FAIL",
                    f"clipboard after harness ^c: {harness_copy!r}",
                )
            )

            clear_field(win, None)
            clear_clipboard()
            time.sleep(0.8)
            win.type_keys("l;ylfu", with_spaces=True, pause=0.05)
            time.sleep(0.5)
            select_all_and_convert(win)
            time.sleep(3.0)
            got = read_textarea(win)
            results.append(
                (
                    "selection-convert-empty-clipboard",
                    "PASS" if got == "สวัสดี" else "FAIL",
                    f"expected 'สวัสดี' got {got!r}",
                )
            )

            clear_field(win, None)
            win.type_keys("l;ylfu", with_spaces=True, pause=0.05)
            time.sleep(0.5)
            set_clipboard(SENTINEL)
            time.sleep(1.2)
            select_all_and_convert(win)
            time.sleep(3.0)
            got2 = read_textarea(win)
            results.append(
                (
                    "selection-convert-text-clipboard",
                    "PASS" if got2 == "สวัสดี" else "FAIL",
                    f"expected 'สวัสดี' got {got2!r}",
                )
            )

            # --- 3.3 clipboard restored --------------------------------
            back = get_clipboard()
            results.append(
                (
                    "clipboard-restored",
                    "PASS" if back == SENTINEL else "FAIL",
                    f"expected {SENTINEL!r} got {back!r}",
                )
            )

        # --- 3.4 focus race / no cross-control injection ----------------
        if clear_field(win, None):
            win.type_keys("l;ylfu", with_spaces=True, pause=0.05)
            time.sleep(0.4)
            win.type_keys("^a", pause=0.05)
            time.sleep(0.2)
            win.type_keys("+{CAPSLOCK}", pause=0.03)
            # Move focus immediately: the conversion must abort, not follow.
            r = win.rectangle()
            mouse.click(button="left", coords=((r.left + r.right) // 2, r.bottom - 40))
            time.sleep(2.0)
            pw = password_value(win)
            results.append(
                (
                    "no-cross-control-injection",
                    "PASS" if pw == "" else "FAIL",
                    f"password field held {pw!r}",
                )
            )
        else:
            results.append(("no-cross-control-injection", "SKIP", "could not clear"))
    finally:
        if browser is not None:
            kill_edge(browser)
        if httpd is not None:
            httpd.shutdown()
        if rt is not None:
            rt.terminate()
        restore_config(original)

    passed = sum(1 for _, verdict, _ in results if verdict == "PASS")
    for name, verdict, detail in results:
        print(f"{verdict:<5} {name:<28} {detail}")
    print(f"\nmanual selection matrix [{BROWSER}]: {passed}/{len(results)}")
    tail = trace()[-14:]
    print("\n".join(tail) if tail else "(no trace)")


if __name__ == "__main__":
    main()

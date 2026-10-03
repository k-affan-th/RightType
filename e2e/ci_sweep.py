"""Real-app E2E for CI (GitHub's Windows runners) — and for anyone's desktop.

    python ci_sweep.py            # all targets
    python ci_sweep.py page omnibox notepad

On anyone's PC without a checkout: CI uploads RightType-selftest-<sha>, a zip
with the diagnostic build, these scripts and test-on-my-pc.bat. Unzip it,
double-click the .bat; it writes RightType-test-report.txt and
RightType-test-trace.txt next to itself. Apps that are not installed are
skipped.

Physical virtual-key events (as a person types) go into real apps with the
debug build of RightType, and the text is read back through UI Automation:

  page     — a text box in a web page (Chrome).
  omnibox  — the browser's address bar, with a history entry that makes it
             complete what is typed inline (the selected completion after the
             caret used to swallow the first Backspace of a correction).
  notepad  — Windows Notepad.
  hang     — proof for the keyboard hook: a window that takes 3 s to answer
             accessibility requests gets focus and a few keys; then the page
             must still be corrected. RightType's focus check asks that window
             on the thread that also runs the keyboard hook, and Windows drops
             a hook that does not answer in time.

Needs Thai Kedmanee and US English installed (CI adds Thai first). Exits 1 if
any case fails, except those listed in KNOWN_FAILING, which are reported but
do not fail the run until they are fixed (and then say so, as XPASS).
"""

import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import threading
import sys
import time
from pathlib import Path

import full_sweep as fs
import lib

tap, type_keys, CTRL, BACK, SHIFT = fs.tap, fs.type_keys, fs.CTRL, fs.BACK, fs.SHIFT
HKL_EN, HKL_TH = fs.HKL_EN, fs.HKL_TH
user32 = ctypes.windll.user32
ENTER = 0x0D

# Cases known to fail, by "target: name". Reported, not fatal, until fixed.
# (Windows 11 Notepad drops Thai injected as Unicode, E-024, but CI's
# Notepad is the classic editor, so nothing is listed for it here.)
KNOWN_FAILING: set[str] = set()
EDGE_ROUNDS = 4
CURRENT = [None]  # the running RightType process
# The debug build writes its problem report here every 1.5 s (tray → "Save a
# problem report…" in a release build), so the sweep can prove it holds no
# typed text.
PROBLEM_REPORT = Path(os.environ.get("TEMP", ".")) / "rt-e2e-problem-report.txt"
os.environ["RIGHTTYPE_E2E_REPORT"] = str(PROBLEM_REPORT)

flip = lambda: tap(BACK, SHIFT)  # noqa: E731


def set_capslock(on):
    if bool(user32.GetKeyState(fs.CAPS) & 1) != on:
        tap(fs.CAPS)
        time.sleep(0.3)


def palette_by_keyboard(t):
    """The command palette without a mouse: select a word, open it, type to
    search, Enter."""
    def steps():
        fs.select_word()
        time.sleep(0.3)
        tap(fs.SPACE, fs.CTRL, fs.ALT)  # the palette's hotkey
        time.sleep(1.0)
        fs.type_keys("upper")
        time.sleep(0.4)
        tap(ENTER)
        time.sleep(1.2)
    fs.run(t, "palette by keyboard: search, Enter", "hello", "HELLO", then=[steps])


# Added to the sweep's config: a snippet and the misspelling fixes (2.1).
SNIPPET_CONFIG = '''[[snippets]]
trigger = ";sig"
text = "Best regards"
scope = "either"

[[snippets]]
trigger = ";today"
text = "{iso}"
scope = "either"

[[snippets]]
trigger = "teh"
text = "the"
scope = "typo"
'''


def write_sweep_config(tables="", keys=""):
    """The sweep's config. `keys` are more top-level keys; `tables` (TOML
    tables such as [app_modes]) go after every top-level key: a key written
    after a table header belongs to that table (and then the whole file is
    refused)."""
    fs.write_config(mode="auto", learn=False)
    path = fs.DATA / "config.toml"
    path.write_text(path.read_text(encoding="utf-8") + "fix_spelling = true\ncomplete_thai = true\n"
                    + keys + "\n"
                    + tables + SNIPPET_CONFIG, encoding="utf-8")


def snippets_and_spelling(t):
    """2.1: a snippet on either keyboard, taken back with Shift+Backspace;
    a common Thai misspelling put right, and put back by Backspace right
    after."""
    fs.run(t, "snippet expands", ";sig ", "Best regards")
    fs.run(t, "snippet expands on the Thai keyboard (same keys)", ";sig ", "Best regards",
           layout=HKL_TH)
    fs.run(t, "snippet taken back with Shift+Backspace", ";sig ", ";sig", then=[flip])
    import datetime
    fs.run(t, "snippet with today's date", ";today ", datetime.date.today().isoformat())
    # อนุญาติ (keys vo6Pk9b on the Thai keyboard) → อนุญาต.
    fs.run(t, "common misspelling put right", "vo6Pk9b ", "อนุญาต", layout=HKL_TH)
    fs.run(t, "Backspace right after puts the misspelling back", "vo6Pk9b ", "อนุญาติ",
           layout=HKL_TH, then=[lambda: tap(BACK)])


def palette_search(text, settle=1.2):
    """Open the palette, search for `text`, Enter."""
    def steps():
        tap(fs.SPACE, fs.CTRL, fs.ALT)  # the palette's hotkey
        time.sleep(1.0)
        fs.type_keys(text)
        time.sleep(0.4)
        tap(ENTER)
        time.sleep(settle)
    return steps


def select_line():
    fs.select_word()
    time.sleep(0.3)


def palette_text_tools(t):
    """2.1: the palette's text tools, in Manual mode so nothing is fixed as
    typed: amounts and years rewritten, a special character typed, only the
    wrong-keyboard words of a selection fixed, the copied text typed key by
    key, and "Fix this field" checked word by word before fixing."""
    fs.check(t.name, "mode set to manual", str(fs.set_mode("manual")), "True")
    try:
        fs.run(t, "palette: amount in words", "1250", "หนึ่งพันสองร้อยห้าสิบบาทถ้วน",
               then=[select_line, palette_search("amount")])
        fs.run(t, "palette: year BE to CE", "2569", "2026",
               then=[select_line, palette_search("year")])
        fs.run(t, "palette: special character by name", "25", "25°",
               then=[palette_search("degree")])
        fs.run(t, "palette: only the wrong-keyboard words of a selection",
               "hello l;ylfu", "hello สวัสดี",
               then=[select_line, palette_search("wrong-keyboard")])
        put_on_clipboard("hi สวัสดี")
        fs.run(t, "palette: copied text typed key by key", "", "hi สวัสดี",
               then=[palette_search("key by key", settle=2.0)])
        # Fix this field: the words are listed ticked; Enter fixes them, or
        # Space first leaves the selected one as typed.
        fs.run(t, "fix this field after checking", "l;ylfu 8iy[", "สวัสดี ครับ",
               then=[palette_search("fix this", settle=1.5), lambda: tap(ENTER),
                     lambda: time.sleep(1.5)])
        # Space only once the words are listed, as the typist would see them
        # first (reading the field can take seconds: CI once took 3.6 s).
        def listed_then(*steps):
            def go():
                start = fs.log_size()
                palette_search("fix this", settle=0.3)()
                deadline = time.time() + 8
                while time.time() < deadline and \
                        "words to fix listed" not in fs.log_since(start):
                    time.sleep(0.2)
                time.sleep(0.4)
                for step in steps:
                    step()
            return go
        fs.run(t, "fix this field with a word unticked", "l;ylfu 8iy[", "l;ylfu ครับ",
               then=[listed_then(lambda: tap(fs.SPACE), lambda: time.sleep(0.3),
                                 lambda: tap(ENTER), lambda: time.sleep(1.5))])
    finally:
        fs.set_mode("auto")


def hold_flip(seconds=0.8):
    """Shift+Backspace held: the key's down repeated, as a held key does."""
    user32.keybd_event(SHIFT, 0, 0, 0)
    time.sleep(0.05)
    user32.keybd_event(BACK, 0, 0, 0)
    for _ in range(int(seconds / 0.1)):
        time.sleep(0.1)
        user32.keybd_event(BACK, 0, 0, 0)
    user32.keybd_event(BACK, 0, 2, 0)
    user32.keybd_event(SHIFT, 0, 2, 0)


def traced(steps, what):
    """Run `steps`; did the trace say `what` meanwhile?"""
    start = fs.log_size()
    steps()
    time.sleep(0.6)
    return "yes" if what in fs.log_since(start) else "no"


def keyboard_states(t):
    """2.2: addresses and numbers typed with the Thai keyboard on come back;
    Shift+Backspace held flips the whole run; the keypad with NumLock off
    and Insert are pointed out."""
    fs.run(t, "email typed on the Thai keyboard", "name@gmail.com ", "name@gmail.com",
           layout=HKL_TH)
    fs.run(t, "web address typed on the Thai keyboard", "www.google.co.th ",
           "www.google.co.th", layout=HKL_TH)
    fs.run(t, "number typed on the Thai keyboard", "100 ", "100", layout=HKL_TH)
    fs.run(t, "time typed on the Thai keyboard", "10:30 ", "10:30", layout=HKL_TH)
    fs.check(t.name, "mode set to manual", str(fs.set_mode("manual")), "True")
    try:
        fs.run(t, "Shift+Backspace held flips the whole run", "l;ylfu 8iy[ l;ylfu ",
               "สวัสดี ครับ สวัสดี", then=[hold_flip], settle=1.0)
    finally:
        fs.set_mode("auto")
    t.clear()
    # Keypad 1 with NumLock off sends End without the extended flag (`tap`
    # sends the separate End key, extended).
    def keypad_1():
        fs.key(0x23)
        fs.key(0x23, True)
        time.sleep(0.07)
    # This process's view of NumLock (GetKeyState) can be stale, so: try,
    # and if nothing was said, switch NumLock over and try once more; put it
    # back afterwards.
    said = traced(keypad_1, "NumLock off: said so")
    toggled = said != "yes"
    if toggled:
        tap(0x90)
    try:
        if toggled:
            said = traced(keypad_1, "NumLock off: said so")
        fs.check(t.name, "keypad with NumLock off is pointed out", said, "yes")
    finally:
        if toggled:
            tap(0x90)
    # Insert from the editing keys is an extended key.
    def insert():
        user32.keybd_event(0x2D, 0, 1, 0)
        user32.keybd_event(0x2D, 0, 3, 0)
    fs.check(t.name, "Insert in a text field is pointed out",
             traced(insert, "Insert pressed in a text field"), "yes")
    insert()  # back to how it was


def password_tag(t):
    """2.2: the TH tag at a password field with the Thai keyboard on."""
    def steps():
        t.pw.set_focus()
        time.sleep(0.3)
        t.layout(HKL_TH)
        time.sleep(0.8)
    fs.check(t.name, "TH tag at a password field", traced(steps, "password tag: TH"), "yes")
    t.layout(HKL_EN)
    t.focus()


def thai_text_tools(t):
    """2.2: Thai spacing from the palette; "why?" for the last word; a
    misspelling of one's own put right; the rest of a long Thai word taken
    with Tab."""
    fs.run(t, "own misspelling put right", "teh ", "the")
    fs.run(t, "own misspelling put back by Backspace right after", "teh ", "teh",
           then=[lambda: tap(BACK)])
    fs.check(t.name, "why? says an English word was left as typed",
             traced(lambda: (fs.type_keys("hello "), time.sleep(0.5),
                             palette_search("why")()), "why: KeptEnglish"), "yes")
    fs.run(t, "rest of a long Thai word with Tab", "xit=kly,ry", "ประชาสัมพันธ์",
           layout=HKL_TH, then=[lambda: tap(0x09)])
    fs.check(t.name, "mode set to manual", str(fs.set_mode("manual")), "True")
    try:
        fs.run(t, "Thai spacing from the palette", "gfHdqg]jo", "เด็ก ๆ เล่น",
               layout=HKL_TH, then=[select_line, lambda: t.layout(HKL_EN),
                                    palette_search("spacing")])
    finally:
        fs.set_mode("auto")


def keyboard_map(t):
    """2.2: the keyboard map types the key clicked on it, where the typist
    is, without taking the focus."""
    from pywinauto import Desktop
    t.clear()
    t.focus()
    tap(ord("K"), CTRL, fs.ALT)
    time.sleep(1.2)
    typed = "no window"
    try:
        win = Desktop(backend="uia").window(title="Keyboard map")
        win.child_window(title="ส", control_type="Button").invoke()
        time.sleep(1.0)
        typed = t.read().strip()
    except Exception as e:
        typed = f"failed: {e}"
    finally:
        tap(ord("K"), CTRL, fs.ALT)  # closed again
        time.sleep(0.5)
    fs.check(t.name, "keyboard map types the key clicked", typed, "ส")


def keyboard_lock(t):
    """2.3: cleaning locks the keyboard (nothing reaches the app) until
    unlocked; the key tester takes keys and closes on Esc held."""
    from pywinauto import Desktop
    t.clear()
    t.focus()
    palette_search("clean the keyboard", settle=1.5)()
    typed = "no window"
    try:
        win = Desktop(backend="uia").window(title_re="Clean the keyboard|ทำความสะอาดคีย์บอร์ด")
        win.child_window(title_re="Lock and start|ล็อกและเริ่มเช็ด",
                         control_type="Button").invoke()
        time.sleep(3.8)  # the lead-in before the lock
        t.focus()
        time.sleep(0.4)
        type_keys("locked ")
        time.sleep(0.6)
        typed = t.read().strip()
        win.child_window(title_re="Unlock|ปลดล็อก", control_type="Button").invoke()
        time.sleep(0.6)
        # The window's own Close, not the title bar's.
        next(b for b in win.descendants(control_type="Button")
             if b.window_text() in ("Close", "ปิด")
             and b.parent().element_info.control_type != "TitleBar").invoke()
        time.sleep(0.6)
    except Exception as e:
        typed = f"failed: {e}"
    fs.check(t.name, "cleaning lock keeps keys from the app", typed, "")
    t.focus()
    time.sleep(0.3)
    type_keys("ok ")
    time.sleep(0.6)
    fs.check(t.name, "keys work again after cleaning", t.read().strip(), "ok")
    t.clear()
    palette_search("test the keyboard", settle=1.5)()
    def hold_esc():
        fs.key(0x1B)
        for _ in range(12):  # auto-repeat, as a held key sends
            time.sleep(0.2)
            fs.key(0x1B)
        fs.key(0x1B, True)
        time.sleep(0.8)
    type_keys("abc")
    fs.check(t.name, "key tester closes on Esc held",
             traced(hold_esc, "keyboard lock: off"), "yes")
    t.focus()
    time.sleep(0.3)
    type_keys("ok ")
    time.sleep(0.6)
    fs.check(t.name, "keys work again after the key tester", t.read().strip(), "ok")


def keyboard_health(t):
    """2.3: the Tools page's health check finds CapsLock on, and its fix
    turns it off."""
    from pywinauto import Desktop
    t.focus()
    result = "no window"
    try:
        set_capslock(True)
        palette_search("settings", settle=2.0)()
        win = Desktop(backend="uia").window(title_re="RightType — .*")
        win.child_window(title_re="Tools|เครื่องมือ", control_type="RadioButton").invoke()
        time.sleep(0.8)
        win.child_window(title_re="Check again|ตรวจอีกครั้ง", control_type="Button").invoke()
        time.sleep(0.8)
        def fix_caps():
            buttons = [b for b in win.descendants(control_type="Button")
                       if b.window_text() in ("Turn off", "ปิด") and b.is_visible()]
            # CapsLock is the last check on the page.
            buttons[-1].invoke()
        result = traced(fix_caps, "health: fix CapsOff")
        win.close()
        time.sleep(0.5)
    except Exception as e:
        result = f"failed: {e}"
    finally:
        set_capslock(False)
    fs.check(t.name, "health check turns CapsLock off", result, "yes")


def key_bounce(t):
    """2.3: a key that types twice by itself (pressed again a few ms after
    letting go): counted, and dropped for a key chosen to be filtered (h)."""
    write_sweep_config(keys="debounce_keys = [0x23]\n")  # h
    try:
        CURRENT[0] = fs.start_rt()
        t.clear()
        t.focus()
        time.sleep(0.4)
        def bounce(vk):
            fs.key(vk)
            time.sleep(0.06)
            fs.key(vk, True)
            time.sleep(0.005)  # a bounce: no finger is this quick
            fs.key(vk)
            time.sleep(0.02)
            fs.key(vk, True)
            time.sleep(0.3)
        fs.check(t.name, "a bounce of a filtered key is dropped",
                 traced(lambda: bounce(ord("H")), "key bounce dropped"), "yes")
        fs.check(t.name, "a bounce of another key is counted",
                 traced(lambda: bounce(ord("J")), "key bounce: scan 0x24"), "yes")
        time.sleep(0.4)
        fs.check(t.name, "the filtered key typed once", t.read().strip(), "hjj")
    finally:
        write_sweep_config()
        CURRENT[0] = fs.start_rt()


def by_app(t):
    """2.3 P4: a word fixed in Notepad counts for Notepad (and its
    read-back too, where it shares its text)."""
    t.clear()
    t.focus()
    t.layout(fs.HKL_EN)
    time.sleep(0.3)
    fs.check(t.name, "a fix counts for the app",
             traced(lambda: type_keys("l;ylfu8iy[ "), "app quality: notepad.exe Fixed"),
             "yes")


def caret_list(t):
    """2.4 A: Shift tapped twice opens the list at the cursor; what is
    searched never reaches the text; Enter types the character picked;
    capitals (Shift with a letter) never open it; Esc closes it."""
    def shift_twice():
        for _ in range(2):
            fs.key(SHIFT)
            time.sleep(0.05)
            fs.key(SHIFT, True)
            time.sleep(0.12)
    t.clear()
    t.focus()
    t.layout(fs.HKL_EN)
    time.sleep(0.3)
    fs.check(t.name, "Shift twice opens the list at the cursor",
             traced(shift_twice, "caret list: open"), "yes")
    time.sleep(0.4)
    type_keys("degree")
    time.sleep(0.5)
    tap(ENTER)
    time.sleep(1.2)
    fs.check(t.name, "the list types the character; the search stays out",
             t.read().strip(), "°")
    t.clear()
    t.focus()
    fs.check(t.name, "capitals do not open the list",
             traced(lambda: type_keys("Hello World "), "caret list: open"), "no")
    t.clear()
    t.focus()
    shift_twice()
    time.sleep(0.4)
    fs.check(t.name, "Esc closes the list",
             traced(lambda: (tap(0x1B), time.sleep(0.3)), "caret list: closed"), "yes")
    type_keys("ok")
    time.sleep(0.5)
    fs.check(t.name, "typing goes to the app after the list", t.read().strip(), "ok")


def ghosts(t):
    """2.4 C: `->` offers →, Tab takes it; without Tab the text stays as
    typed."""
    t.clear()
    t.focus()
    t.layout(fs.HKL_EN)
    time.sleep(0.3)
    fs.check(t.name, "a symbol is offered for ->",
             traced(lambda: (type_keys("a ->"), time.sleep(0.3)), "ghost offered: U+2192"), "yes")
    tap(0x09)
    time.sleep(0.5)
    type_keys(" b")
    time.sleep(0.8)
    fs.check(t.name, "Tab puts the symbol in", t.read().strip(), "a \u2192 b")
    t.clear()
    t.focus()
    type_keys("x != y")
    time.sleep(0.8)
    fs.check(t.name, "without Tab the text stays as typed", t.read().strip(), "x != y")


def keys_on_screen(t):
    """2.3 C8: shortcuts shown on screen when turned on; letters never."""
    write_sweep_config(keys="show_keys = true\n")
    try:
        CURRENT[0] = fs.start_rt()
        t.clear()
        t.focus()
        time.sleep(0.4)
        def ctrl_home():
            tap(0x24, CTRL)
        fs.check(t.name, "a shortcut is shown",
                 traced(ctrl_home, "keys on screen: Ctrl+Home"), "yes")
        fs.check(t.name, "letters are not shown",
                 traced(lambda: type_keys("ab"), "keys on screen: A"), "no")
    finally:
        write_sweep_config()
        CURRENT[0] = fs.start_rt()


def burst(keys):
    """Keys as fast as a machine sends them: no pause between them."""
    for vk in keys:
        fs.key(vk)
        fs.key(vk, True)


def devices(t):
    """2.3: a barcode scanner's burst with the Thai keyboard on comes back as
    digits; a device typing into the Run box faster than a hand is held
    back."""
    t.clear()
    t.focus()
    t.layout(HKL_TH)
    time.sleep(0.4)
    code = "8851234567890"
    burst([ord(c) for c in code] + [ENTER])
    time.sleep(1.0)
    fs.check(t.name, "scanner burst comes back as digits", t.read().strip(), code)
    t.clear()
    t.layout(HKL_EN)
    def run_box():
        tap(ord("R"), 0x5B)  # Win+R
        time.sleep(0.8)
        burst([ord(c) for c in "NOTEPAD"] + [0xBE, ord("E"), ord("X"), ord("E"), ENTER])
        time.sleep(1.5)
        tap(0x1B)  # close the Run box (the device has gone quiet)
        time.sleep(0.5)
    fs.check(t.name, "fake keyboard typing into Run is held back",
             traced(run_box, "fake keyboard: keys held back"), "yes")
    t.focus()


def hold_for_accents(t):
    """2.3: with the option on, holding e and pressing 1 types é in place of
    the e."""
    write_sweep_config(keys="hold_for_accents = true\n")
    try:
        CURRENT[0] = fs.start_rt()
        t.clear()
        t.focus()
        time.sleep(0.4)
        fs.key(ord("E"))
        for _ in range(8):  # held: Windows repeats the key-down
            time.sleep(0.05)
            fs.key(ord("E"))
        fs.key(ord("E"), True)
        time.sleep(0.4)
        tap(ord("1"))
        time.sleep(0.6)
        fs.check(t.name, "holding e then 1 types é", t.read().strip(), "é")
    finally:
        write_sweep_config()
        CURRENT[0] = fs.start_rt()


def shortcuts_in_english(t):
    """2.3: with the Thai keyboard on, a web page sees Ctrl+ร for Ctrl+I;
    with the option on, Ctrl switches to English while held, so it sees
    Ctrl+i, and Thai is back after."""
    from pywinauto import Desktop  # noqa: F401
    def seen():
        for d in t.win.descendants(control_type="Text"):
            name = d.element_info.name or ""
            if name.startswith("ctrl ") or name == "none":
                return name
        return "not found"
    def ctrl_i():
        t.focus()
        t.layout(HKL_TH)
        time.sleep(0.4)
        fs.key(CTRL)
        time.sleep(0.15)
        tap(ord("I"))
        fs.key(CTRL, True)
        time.sleep(0.6)
    ctrl_i()
    fs.check(t.name, "Thai keyboard: a page sees Ctrl+ร", seen(), "ctrl ร")
    write_sweep_config(keys="shortcuts_in_english = true\n")
    try:
        CURRENT[0] = fs.start_rt()
        ctrl_i()
        fs.check(t.name, "shortcuts in English: a page sees Ctrl+i", seen(), "ctrl i")
        fs.check(t.name, "Thai keyboard back after the shortcut",
                 f"{keyboard_of(t.hwnd):04X}", "041E")
    finally:
        write_sweep_config()
        CURRENT[0] = fs.start_rt()


def shortcut_list(t):
    """2.3: the app's shortcut list: searched, and Enter presses the
    shortcut in the app (Notepad's Replace, Ctrl+H)."""
    from pywinauto import Desktop
    t.clear()
    t.focus()
    result = "no list"
    try:
        palette_search("app's shortcuts", settle=1.5)()
        time.sleep(0.5)
        type_keys("replace")
        time.sleep(0.6)
        tap(ENTER)
        time.sleep(1.5)
        # Notepad's Replace box is owned by Notepad: UI Automation lists it
        # under Notepad's window, not the desktop.
        dialogs = Desktop(backend="uia").windows(title_re="Replace|แทนที่") or [
            d for d in t.win.descendants(control_type="Window")
            if d.window_text() in ("Replace", "แทนที่")]
        result = "opened" if dialogs else "no Replace dialog"
        for d in dialogs:
            d.close()
        if not dialogs:
            # Whatever did open must not keep the focus from the cases after.
            tap(0x1B)
            time.sleep(0.3)
    except Exception as e:
        result = f"failed: {e}"
    fs.check(t.name, "shortcut list presses the shortcut found", result, "opened")
    t.focus()


def typing_practice(t):
    """2.3: typing practice takes the keys while it is in front (nothing
    reaches the app behind), and the app gets them again once it closes."""
    from pywinauto import Desktop
    t.clear()
    t.focus()
    result = "no window"
    try:
        palette_search("typing practice", settle=2.0)()
        win = Desktop(backend="uia").window(title_re="Typing practice|ฝึกพิมพ์")
        win.set_focus()
        time.sleep(0.4)
        type_keys("asdf")
        time.sleep(0.5)
        # The keys went to the practice, not to Notepad behind it.
        result = "kept" if t.read().strip() == "" else f"leaked: {t.read().strip()!r}"
        # T5: the practised keyboard stays on screen while working.
        on_screen = "no map"
        try:
            win.child_window(title_re="Keep this keyboard on screen|เปิดแป้นนี้ค้างไว้บนจอ",
                             control_type="Button").invoke()
            time.sleep(1.0)
            if Desktop(backend="uia").window(title="Keyboard map").exists(timeout=2):
                on_screen = "shown"
        except Exception as e:
            on_screen = f"failed: {e}"
        win.close()
        time.sleep(0.6)
        if on_screen == "shown":
            tap(ord("K"), CTRL, fs.ALT)  # closed again
            time.sleep(0.5)
        fs.check(t.name, "practice keeps its keyboard on screen", on_screen, "shown")
    except Exception as e:
        result = f"failed: {e}"
    fs.check(t.name, "typing practice keeps its keys", result, "kept")
    t.focus()
    type_keys("ok ")
    time.sleep(0.6)
    fs.check(t.name, "keys reach the app after practice", t.read().strip(), "ok")


def thai_word_delete(t):
    """2.1: Ctrl+Backspace after Thai takes one Thai word, not the run."""
    fs.run(t, "Ctrl+Backspace deletes one Thai word", "l;ylfu8iy[", "สวัสดี",
           layout=HKL_TH, then=[lambda: tap(BACK, CTRL)])
    fs.run(t, "Ctrl+Backspace twice, Ctrl held", "l;ylfu8iy[l;ylfu", "สวัสดี",
           layout=HKL_TH, then=[lambda: hold_ctrl_tap_twice(BACK)])


def hold_ctrl_tap_twice(vk):
    user32.keybd_event(CTRL, 0, 0, 0)
    time.sleep(0.05)
    for _ in range(2):
        user32.keybd_event(vk, 0, 0, 0)
        user32.keybd_event(vk, 0, 2, 0)
        time.sleep(0.4)
    user32.keybd_event(CTRL, 0, 2, 0)


def enter_guard(t):
    """2.1: in a chat app (Notepad stands in for one here), Enter on a
    message typed on the wrong keyboard is held once."""
    write_sweep_config(keys='chat_apps = ["notepad.exe"]\n')
    try:
        CURRENT[0] = fs.start_rt()
        t.focus()
        fs.check(t.name, "mode set to manual", str(fs.set_mode("manual")), "True")

        def lines(keys, enters):
            t.clear()
            fs.type_keys(keys)
            for _ in range(enters):
                time.sleep(0.4)
                tap(ENTER)
            time.sleep(0.4)
            fs.type_keys("x")
            time.sleep(0.8)
            return t.read().replace("\r\n", "\n").replace("\r", "\n")
        fs.check(t.name, "Enter held on a wrong-keyboard message",
                 lines("l;ylfu 8iy[", 1), "l;ylfu 8iy[x")
        fs.check(t.name, "Enter again sends it anyway",
                 lines("l;ylfu 8iy[", 2), "l;ylfu 8iy[\nx")
        fs.check(t.name, "Enter not held on a right message",
                 lines("hello world", 1), "hello world\nx")
    finally:
        fs.set_mode("auto")
        write_sweep_config()
        CURRENT[0] = fs.start_rt()
        t.focus()
    # A fresh RightType: give the problem report (checked after the sweep)
    # a word to record again.
    fs.run(t, "Auto again after the chat check", "l;ylfu ", "สวัสดี")


def keyboard_of(hwnd):
    """The keyboard layout (low word) of the window's thread."""
    tid = user32.GetWindowThreadProcessId(hwnd, None)
    return user32.GetKeyboardLayout(tid) & 0xFFFF


def app_keyboards(t):
    """2.2: the keyboard an app starts with; English outside text; the grave
    key typing its character; a switch that came with a shortcut undone.
    Notepad stands in for the app."""
    write_sweep_config(keys='grave_types = true\nguard_switch = true\n',
                       tables='[app_keyboards]\n"notepad.exe" = "en"\n\n')
    try:
        CURRENT[0] = fs.start_rt()
        t.focus()
        t.layout(HKL_TH)
        # Away and back: Notepad comes to the front again.
        user32.SetForegroundWindow(user32.FindWindowW("Shell_TrayWnd", None))
        time.sleep(0.8)
        t.focus()
        time.sleep(1.0)
        fs.check(t.name, "app keyboard: English on coming to the front",
                 f"{keyboard_of(t.hwnd):04X}", "0409")
        fs.run(t, "grave key types ` on the English keyboard", "", "`",
               then=[lambda: tap(0xC0)])
        fs.run(t, "grave key types _ on the Thai keyboard", "", "_",
               layout=HKL_TH, then=[lambda: tap(0xC0)])
        # A switch right after Ctrl+Shift+a key (not ours) is put back once a
        # key shows RightType the new layout.
        t.clear()
        t.layout(HKL_EN)
        def shortcut_then_switch():
            tap(ord("X"), CTRL, SHIFT)
            user32.PostMessageW(t.hwnd, 0x0050, 0, HKL_TH)
            time.sleep(0.2)
            tap(fs.SPACE)
            time.sleep(0.6)
        fs.check(t.name, "language switch with a shortcut is undone",
                 traced(shortcut_then_switch, "language switch with a shortcut: undone"), "yes")
    finally:
        t.layout(HKL_EN)
        write_sweep_config()
        CURRENT[0] = fs.start_rt()
        t.focus()
    fs.run(t, "Auto again after the keyboard checks", "l;ylfu ", "สวัสดี")


def outside_text(t):
    """2.2: in an app set to "English outside text", the button (no caret)
    gets English, and the text box the Thai keyboard back."""
    if not getattr(t, "tool", None):
        return
    write_sweep_config(tables='[app_keyboards]\n"chrome.exe" = "en-outside-text"\n\n')
    try:
        CURRENT[0] = fs.start_rt()
        t.focus()
        t.layout(HKL_TH)
        t.tool.set_focus()
        time.sleep(1.0)
        fs.check(t.name, "English outside text: a button gets English",
                 f"{keyboard_of(t.hwnd):04X}", "0409")
        t.box.set_focus()
        time.sleep(1.0)
        fs.check(t.name, "English outside text: the text box gets Thai back",
                 f"{keyboard_of(t.hwnd):04X}", "041E")
    finally:
        t.layout(HKL_EN)
        write_sweep_config()
        CURRENT[0] = fs.start_rt()
        t.focus()
    fs.run(t, "Auto again after English outside text", "l;ylfu ", "สวัสดี")


def code_mode(t):
    """2.1 Code mode, with the browser set to it: names stay, Thai keys
    typed for code come back as the English typed, Thai only in comments."""
    write_sweep_config('[app_modes]\n"chrome.exe" = "code"\n"msedge.exe" = "code"\n\n')
    try:
        CURRENT[0] = fs.start_rt()
        t.focus()
        fs.run(t, "code mode: a name stays", "getUserName ", "getUserName")
        fs.run(t, "code mode: Thai keys typed for code", "asdf ", "asdf", layout=HKL_TH)
        fs.run(t, "code mode: Thai-looking keys in code stay", "l;ylfu ", "l;ylfu")
        fs.run(t, "code mode: Thai in a comment", "// l;ylfu ", "// สวัสดี")
    finally:
        write_sweep_config()
        CURRENT[0] = fs.start_rt()
        t.focus()
    # A fresh RightType: give the problem report (checked after the sweep)
    # a word to record again.
    fs.run(t, "Auto again after Code mode", "l;ylfu ", "สวัสดี")


def one_instance():
    """2.1: a second launch of the same program leaves (the first keeps
    running); another copy of the program takes over."""
    first = fs.start_rt()
    env = {**os.environ, "RIGHTTYPE_E2E_ACCEPT_INJECTED": "1",
           "RIGHTTYPE_E2E_DATA_DIR": str(fs.DATA)}
    second = subprocess.Popen([str(fs.EXE)], env=env)
    try:
        second.wait(timeout=8)
        left = "left"
    except subprocess.TimeoutExpired:
        left = "still running"
    fs.check("instance", "second launch of the same program leaves", left, "left")
    fs.check("instance", "the first keeps running",
             "running" if first.poll() is None else "ended", "running")
    copy_dir = Path(os.environ.get("TEMP", ".")) / "rt-e2e-copy"
    copy_dir.mkdir(exist_ok=True)
    copy = copy_dir / "righttype.exe"
    import shutil
    shutil.copy2(fs.EXE, copy)
    other = subprocess.Popen([str(copy)], env=env)
    try:
        first.wait(timeout=10)
        old = "closed"
    except subprocess.TimeoutExpired:
        old = "still running"
    time.sleep(1.0)
    fs.check("instance", "another copy takes over: the old one closes", old, "closed")
    fs.check("instance", "another copy takes over: the new one runs",
             "running" if other.poll() is None else "ended", "running")
    other.kill()
    subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)


def full_screen_browser_still_works(t):
    """Full screen (F11) with a text cursor is not a game: still corrected."""
    tap(0x7A)  # F11
    time.sleep(1.5)
    try:
        fs.run(t, "full-screen browser still corrected", "l;ylfu ", "สวัสดี")
    finally:
        tap(0x7A)
        time.sleep(1.5)


def capslock_left_on(t):
    """CapsLock left on: the English layout shows L;YLFU for the keys of
    สวัสดี. RightType keeps the keys as if it were off."""
    for name, keys, expect, layout in [
        ("Thai typed with CapsLock on", "l;ylfu ", "สวัสดี", HKL_EN),
        ("English typed with CapsLock on stays", "hello world ", "HELLO WORLD", HKL_EN),
        # Left on by accident: Shift on the first letter shows hELLO.
        ("CapsLock on by accident, English", "Hello ", "Hello", HKL_EN),
        # Thai layout: CapsLock shifts every key (l;ylfu gives ศซํศโ๊).
        ("CapsLock on by accident, Thai layout", "l;ylfu ", "สวัสดี", HKL_TH),
        # Capitals were meant after all: one Shift+Backspace puts them back
        # (and CapsLock on again).
        ("CapsLock fix taken back with Shift+Backspace", "Hello |flip", "hELLO", HKL_EN),
    ]:
        t.clear()
        t.layout(layout)
        set_capslock(True)
        try:
            typed, _, then = keys.partition("|")
            fs.type_keys(typed)
            if then == "flip":
                time.sleep(0.5)
                flip()
            time.sleep(0.8)
            fs.check(t.name, name, t.read(), expect)
        finally:
            set_capslock(False)


def thai_capslock_probe():
    """What the Thai Kedmanee layout types with CapsLock on (printed, not
    judged): whether CapsLock changes Thai letters decides what a word typed
    on the Thai layout with CapsLock on looks like."""
    state = (ctypes.c_ubyte * 256)()
    out = ctypes.create_unicode_buffer(8)
    rows = []
    for caps in (0, 1):
        state[fs.CAPS] = caps
        chars = ""
        for key in "l;ylfu":
            vk = ord(key.upper()) if key.isalpha() else fs.PUNCT[key]
            n = user32.ToUnicodeEx(vk, 0, state, out, 8, 0, ctypes.c_void_p(HKL_TH))
            chars += out.value[:n] if n > 0 else "?"
        rows.append(f"caps={caps}: {chars}")
    print("Thai layout, keys l;ylfu: " + " | ".join(rows), flush=True)


def installed_layouts():
    n = user32.GetKeyboardLayoutList(0, None)
    buf = (ctypes.c_void_p * n)()
    user32.GetKeyboardLayoutList(n, buf)
    return [(b or 0) & 0xFFFFFFFF for b in buf]


def rt_health(proc):
    """Is RightType still running, and does its UI thread (which also runs
    the keyboard hook) still answer? If it hangs, print every thread's stack
    with the Windows debugger when the runner has one."""
    code = proc.poll()
    if code is not None:
        return f"exited with code {code & 0xFFFFFFFF:#010x}"
    found = []
    proto = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)

    def each(hwnd, _):
        pid = wt.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value == proc.pid:
            found.append(hwnd)
        return True

    user32.EnumWindows(proto(each), 0)
    if not found:
        return "running, no window found"
    result = ctypes.c_size_t()
    ok = user32.SendMessageTimeoutW(found[0], 0, 0, 0, 0x0002, 3000, ctypes.byref(result))
    if ok:
        return "running and responsive"
    cdb = next((p for p in [
        r"C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe",
        r"C:\Program Files\Windows Kits\10\Debuggers\x64\cdb.exe",
    ] if Path(p).exists()), None)
    if cdb is None:
        return "running but NOT responding (no cdb on this machine for stacks)"
    out = subprocess.run(
        [cdb, "-pv", "-p", str(proc.pid), "-y", str(Path(str(fs.EXE)).parent),
         "-c", "~*kn 60; qd"],
        capture_output=True, text=True, errors="replace", timeout=180)
    return "running but NOT responding; thread stacks:\n" + out.stdout[-30000:]


# --------------------------------------------------------------------------- targets


class Page(fs.Chrome):
    name = "page"

    def __init__(self):
        if not lib.CHROME_EXE.exists():
            raise SkipTarget("Google Chrome is not installed here")
        super().__init__()


class HangPage(Page):
    name = "hang"


class Omnibox(fs.Target):
    """The address bar, after the test URL was typed there once, so typing
    its first letters again makes the browser complete the rest inline."""

    name = "omnibox"

    def __init__(self):
        if not lib.CHROME_EXE.exists():
            raise SkipTarget("Google Chrome is not installed here")
        self.app, self.httpd = lib.start_edge(lib.HERE / "target.html", lib.CHROME_EXE)
        self.win = self.app.top_window()
        self.hwnd = self.win.handle
        port = self.httpd.server_address[1]
        # Typed (not just visited) URLs are the ones completed inline.
        self.win.set_focus()
        self.layout(HKL_EN)
        tap(ord("L"), CTRL)
        type_keys(f"giupo-rt.localhost:{port}/target.html")
        tap(ENTER)
        time.sleep(3)
        self.box = next(
            d for d in self.win.descendants(control_type="Edit")
            if "address" in (d.element_info.name or "").lower()
        )

    def focus(self):
        self.win.set_focus()
        tap(ord("L"), CTRL)
        time.sleep(0.3)

    def clear(self):
        self.focus()
        tap(ord("A"), CTRL)
        tap(fs.DELETE)
        time.sleep(0.3)

    def read(self):
        return self.box.iface_value.CurrentValue or ""

    def close(self):
        tap(0x1B)  # Esc: leave the address bar as it was
        lib.close_app(self.app)
        self.httpd.shutdown()


class EdgeOmnibox(Omnibox):
    """Edge's address bar as it comes: no history, but Bing's search
    suggestions drop down (and arrive late) while you type."""

    name = "edge"

    def __init__(self):
        if not lib.EDGE_EXE.exists():
            raise SkipTarget("Microsoft Edge is not installed here")
        self.app, self.httpd = lib.start_edge(lib.HERE / "target.html", lib.EDGE_EXE)
        self.win = self.app.top_window()
        self.hwnd = self.win.handle
        self.box = next(
            d for d in self.win.descendants(control_type="Edit")
            if "address" in (d.element_info.name or "").lower()
        )


def fast_keys(s, pause=0.03):
    """A quick typist: the keys land while the app is still busy."""
    for ch in s:
        if ch == " ":
            vk = fs.SPACE
        elif ch.isalpha() or ch.isdigit():
            vk = ord(ch.upper())
        elif ch in fs.SHIFTED:
            base = fs.SHIFTED[ch]
            fs.tap(fs.PUNCT.get(base, ord(base)), fs.SHIFT, pause=pause)
            continue
        else:
            vk = fs.PUNCT[ch]
        if ch.isupper():
            fs.tap(vk, fs.SHIFT, pause=pause)
        else:
            fs.tap(vk, pause=pause)


def watch(t, name, keys, expect, typer=type_keys):
    """Type, then read the field every 0.25 s for 2 s, to see what changes
    after the keys (suggestions arriving late, say)."""
    t.clear()
    t.layout(HKL_EN)
    typer(keys)
    seen = []
    for _ in range(8):
        time.sleep(0.25)
        v = t.read()
        if not seen or seen[-1] != v:
            seen.append(v)
    print(f"  {t.name} {name}: {seen}", flush=True)
    return fs.check(t.name, name, seen[-1], expect)


class Notepad(fs.Target):
    name = "notepad"

    def __init__(self):
        self.app = lib.start_notepad()
        self.win = self.app.top_window()
        self.hwnd = self.win.handle
        kinds = sorted({d.element_info.class_name for d in self.win.descendants()})
        print(f"notepad window classes: {kinds}", flush=True)

    def focus(self):
        self.win.set_focus()
        time.sleep(0.2)

    def read(self):
        for d in self.win.descendants():
            if d.element_info.control_type in ("Document", "Edit"):
                try:
                    return d.iface_value.CurrentValue or ""
                except Exception:
                    try:
                        return d.iface_text.DocumentRange.GetText(-1) or ""
                    except Exception:
                        continue
        return ""

    def close(self):
        self.clear()
        try:
            self.app.kill()
        except Exception:
            pass


class SkipTarget(Exception):
    """The app this target needs is not on this machine."""


class Word(fs.Target):
    """Microsoft Word, where it is installed (not on CI): it rewrites words
    as they are typed (a capital at a sentence's start, AutoCorrect), which
    RightType's check-after-write must not take for garbling."""

    name = "word"

    def __init__(self):
        from pywinauto import Desktop
        import shutil
        exe = shutil.which("winword.exe")
        if not exe:
            for root in (os.environ.get("ProgramFiles", ""), os.environ.get("ProgramFiles(x86)", "")):
                for sub in ("Microsoft Office\\root\\Office16", "Microsoft Office\\Office16"):
                    candidate = Path(root) / sub / "WINWORD.EXE"
                    if candidate.exists():
                        exe = str(candidate)
        if not exe:
            raise SkipTarget("Microsoft Word is not installed here")
        self.proc = subprocess.Popen([exe, "/q", "/n"])
        self.win = None
        for _ in range(60):
            time.sleep(0.5)
            for w in Desktop(backend="uia").windows(top_level_only=True):
                if w.element_info.class_name == "OpusApp":
                    self.win = w
                    break
            if self.win:
                break
        if not self.win:
            raise SystemExit("Word did not open a window")
        self.hwnd = self.win.handle
        time.sleep(2)

    def focus(self):
        self.win.set_focus()
        time.sleep(0.3)

    def read(self):
        for d in self.win.descendants(control_type="Document"):
            try:
                return d.iface_text.DocumentRange.GetText(-1) or ""
            except Exception:
                continue
        return ""

    def close(self):
        try:
            self.proc.kill()
        except Exception:
            pass
        subprocess.run(["taskkill", "/IM", "winword.exe", "/F"], capture_output=True)


def word_sweep(t):
    """2.2: RightType next to Word's own rewriting."""
    t.focus()
    fs.run(t, "EN->TH word in Word", "l;ylfu ", "สวัสดี")
    fs.run(t, "email on the Thai keyboard in Word", "name@gmail.com ", "name@gmail.com",
           layout=HKL_TH)
    # Word capitalises the first word of a sentence after RightType wrote it.
    t.clear()
    t.layout(HKL_TH)
    fs.type_keys("correct ")
    time.sleep(1.2)
    fs.check(t.name, "TH->EN word, Word may capitalise it", t.read().strip().lower(), "correct")
    t.layout(HKL_EN)


class Notepad11(Notepad):
    """Windows 11's Notepad (the Store app: a RichEdit box in a WinUI
    window), not the classic one CI has in System32. Thai typed there by
    RightType arrived with characters missing on a real PC."""

    name = "notepad11"
    APP = r"shell:AppsFolder\Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"

    def __init__(self):
        from pywinauto import Application, Desktop
        subprocess.run(["taskkill", "/IM", "notepad.exe", "/F"], capture_output=True)
        time.sleep(1)

        def windows():
            out = {}
            for w in Desktop(backend="uia").windows(top_level_only=True):
                try:
                    if w.class_name() == "Notepad":
                        out[w.handle] = w
                except Exception:
                    pass
            return out

        before = set(windows())
        subprocess.Popen(["explorer.exe", self.APP])
        deadline = time.time() + 30
        while time.time() < deadline:
            fresh = [h for h in windows() if h not in before]
            if fresh:
                break
            time.sleep(0.5)
        else:
            raise SkipTarget("Windows 11 Notepad is not installed here")
        time.sleep(2.0)
        self.app = Application(backend="uia").connect(handle=fresh[0], timeout=10)
        self.win = self.app.top_window()
        self.hwnd = self.win.handle
        kinds = sorted({d.element_info.class_name for d in self.win.descendants()})
        print(f"notepad11 window classes: {kinds}", flush=True)
        self.focus()

        class GUITHREADINFO(ctypes.Structure):
            _fields_ = [("cbSize", wt.DWORD), ("flags", wt.DWORD), ("hwndActive", wt.HWND),
                        ("hwndFocus", wt.HWND), ("hwndCapture", wt.HWND),
                        ("hwndMenuOwner", wt.HWND), ("hwndMoveSize", wt.HWND),
                        ("hwndCaret", wt.HWND), ("rcCaret", wt.RECT)]

        gui = GUITHREADINFO(cbSize=ctypes.sizeof(GUITHREADINFO))
        tid = user32.GetWindowThreadProcessId(self.hwnd, None)
        user32.GetGUIThreadInfo(tid, ctypes.byref(gui))
        cls = ctypes.create_unicode_buffer(64)
        user32.GetClassNameW(gui.hwndFocus, cls, 64)
        print(f"notepad11 keyboard focus window class: {cls.value!r}", flush=True)
        if "RichEditD2DPT" not in kinds:
            self.close()
            raise SkipTarget("the Notepad that opened is not the Windows 11 one")

    def focus(self):
        self.win.set_focus()
        for d in self.win.descendants():
            if d.element_info.class_name == "RichEditD2DPT":
                d.set_focus()
                break
        time.sleep(0.2)


def packets(text, pause=0.0):
    """Send `text` as KEYEVENTF_UNICODE key events (what RightType's
    injector sends): all at once when `pause` is 0, else one by one."""
    events = []
    for ch in text:
        for up in (False, True):
            i = fs.INPUT(type=1)
            i.u.ki = fs.KEYBDINPUT(0, ord(ch), 0x4 | (0x2 if up else 0), 0, 0)
            events.append(i)
    if pause:
        for i in range(0, len(events), 2):
            arr = (fs.INPUT * 2)(*events[i:i + 2])
            user32.SendInput(2, arr, ctypes.sizeof(fs.INPUT))
            time.sleep(pause)
    else:
        arr = (fs.INPUT * len(events))(*events)
        user32.SendInput(len(events), arr, ctypes.sizeof(fs.INPUT))
    time.sleep(0.8)


def unicode_events(text):
    out = []
    for ch in text:
        for up in (False, True):
            i = fs.INPUT(type=1)
            i.u.ki = fs.KEYBDINPUT(0, ord(ch), 0x4 | (0x2 if up else 0), 0, 0)
            out.append(i)
    return out


def backspace_events(n):
    out = []
    for _ in range(n):
        for up in (False, True):
            i = fs.INPUT(type=1)
            i.u.ki = fs.KEYBDINPUT(0x08, 0, 0x2 if up else 0, 0, 0)
            out.append(i)
    return out


def send(events):
    if events:
        arr = (fs.INPUT * len(events))(*events)
        user32.SendInput(len(events), arr, ctypes.sizeof(fs.INPUT))


def replace_typed(keys, thai, how):
    """Type `keys` for real, then replace them with `thai` the way `how`
    says: RightType's single batch, or a variant of it."""
    type_keys(keys)
    time.sleep(0.3)
    n = len(keys)
    if how == "one batch (RightType)":
        send(backspace_events(n) + unicode_events(thai))
    elif how == "two calls":
        send(backspace_events(n))
        send(unicode_events(thai))
    elif how == "two calls, 30 ms apart":
        send(backspace_events(n))
        time.sleep(0.03)
        send(unicode_events(thai))
    elif how == "one call per character":
        send(backspace_events(n))
        for ch in thai:
            send(unicode_events(ch))
    time.sleep(0.8)


def notepad11_probe(t):
    """Evidence, with RightType not running: what does Windows 11 Notepad
    keep of Thai sent the way RightType sends it? Printed, not judged."""
    word = "วันนี้"
    variants = [
        ("ASCII burst, English layout", HKL_EN, lambda: packets("hello"), "hello"),
        ("Thai burst, English layout", HKL_EN, lambda: packets(word), word),
        ("Thai one by one, English layout", HKL_EN, lambda: packets(word, 0.15), word),
        ("Thai burst, Thai layout", HKL_TH, lambda: packets(word), word),
        ("Thai one by one, Thai layout", HKL_TH, lambda: packets(word, 0.15), word),
        # What RightType does at the anchor: the last mark right as it asks
        # the window to switch to the Thai layout.
        ("last mark sent with a layout switch", HKL_EN,
         lambda: (packets(word[:-1]), user32.PostMessageW(t.hwnd, 0x0050, 0, HKL_TH),
                  packets(word[-1])), word),
        ("Thai typed on the Thai keyboard", HKL_TH, lambda: type_keys(";yoouh"), word),
    ]
    for how in ["one batch (RightType)", "two calls", "two calls, 30 ms apart",
                "one call per character"]:
        variants.append((f"typed keys replaced: {how}", HKL_EN,
                         lambda how=how: replace_typed("l;ylf", "สวัสดี", how), "สวัสดี"))
    for how in ["one batch (RightType)", "two calls, 30 ms apart"]:
        variants.append((f"typed keys replaced by English: {how}", HKL_EN,
                         lambda how=how: replace_typed("l;ylf", "hello", how), "hello"))
    for name, hkl, act, expect in variants:
        got = []
        for _ in range(3):
            t.clear()
            t.layout(hkl)
            act()
            got.append(t.read().strip())
        kept = sum(g == expect for g in got)
        print(f"  probe notepad11 {name}: {kept}/3 intact {got}", flush=True)


# --------------------------------------------------------------------------- cases


class HangWindow:
    """A top-level window whose thread takes `hang` seconds to answer
    WM_GETOBJECT (an accessibility request), like an app busy with its own
    work. Anyone asking it who has focus waits that long."""

    def __init__(self, hang=3.0):
        self.hang = hang
        self.hwnd = None
        ready = threading.Event()
        threading.Thread(target=self._run, args=(ready,), daemon=True).start()
        if not ready.wait(10):
            raise SystemExit("hang window did not start")

    def _run(self, ready):
        LRESULT = ctypes.c_ssize_t
        WNDPROC = ctypes.WINFUNCTYPE(LRESULT, wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM)
        user32.DefWindowProcW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
        user32.DefWindowProcW.restype = LRESULT

        def proc(h, m, w, l):
            if m == 0x003D:  # WM_GETOBJECT
                time.sleep(self.hang)
            return user32.DefWindowProcW(h, m, w, l)

        self._proc = WNDPROC(proc)

        class WNDCLASSW(ctypes.Structure):
            _fields_ = [("style", wt.UINT), ("lpfnWndProc", WNDPROC), ("cbClsExtra", ctypes.c_int),
                        ("cbWndExtra", ctypes.c_int), ("hInstance", wt.HINSTANCE), ("hIcon", wt.HICON),
                        ("hCursor", wt.HANDLE), ("hbrBackground", wt.HBRUSH),
                        ("lpszMenuName", wt.LPCWSTR), ("lpszClassName", wt.LPCWSTR)]

        hinst = ctypes.windll.kernel32.GetModuleHandleW(None)
        wc = WNDCLASSW(lpfnWndProc=self._proc, hInstance=hinst, lpszClassName="RtHangWindow")
        user32.RegisterClassW(ctypes.byref(wc))
        user32.CreateWindowExW.restype = wt.HWND
        self.hwnd = user32.CreateWindowExW(0, "RtHangWindow", "RightType hang test",
                                           0x10CF0000, 100, 100, 400, 200, None, None, hinst, None)
        ready.set()
        msg = wt.MSG()
        while user32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
            user32.TranslateMessage(ctypes.byref(msg))
            user32.DispatchMessageW(ctypes.byref(msg))

    def focus(self):
        tap(fs.ALT)  # an Alt press lets this process take the foreground
        user32.SetForegroundWindow(self.hwnd)

    def close(self):
        user32.PostMessageW(self.hwnd, 0x0010, 0, 0)  # WM_CLOSE


def hang_sweep(t):
    """Keys while a focused app is slow to answer must not cost the hook."""
    fs.run(t, "before the slow window", "l;ylfu ", "สวัสดี")
    w = HangWindow(hang=3.0)
    w.focus()
    time.sleep(0.3)
    for _ in range(6):  # typed while RightType's focus check waits on it
        tap(ord("X"), pause=0.25)
    time.sleep(3.5)
    # Edge's crash, forced: a word RightType converts mid-word is finished by
    # a space while RightType is still waiting inside an accessibility call
    # (its focus check on the slow window), so the space is handled inside
    # that call. In Edge this happened only now and then, and then RightType
    # died (0xC000041D). Here every word lands inside the wait.
    for _ in range(3):
        user32.SetForegroundWindow(t.hwnd)
        time.sleep(0.5)
        w.focus()
        time.sleep(0.3)
        type_keys("l;ylfu ")
        time.sleep(3.5)
    fs.check(t.name, "a converted word finished inside a slow accessibility call",
             rt_health(CURRENT[0]), "running and responsive")
    w.close()
    fs.run(t, "after the slow window", "l;ylfu ", "สวัสดี")


def clipboard_state():
    """The clipboard's sequence number and its text, to see whether
    anything wrote to it."""
    seq = user32.GetClipboardSequenceNumber()
    text = None
    k32 = ctypes.windll.kernel32
    k32.GlobalLock.restype = ctypes.c_void_p
    k32.GlobalLock.argtypes = [ctypes.c_void_p]
    user32.GetClipboardData.restype = ctypes.c_void_p
    for _ in range(20):
        if user32.OpenClipboard(None):
            h = user32.GetClipboardData(13)  # CF_UNICODETEXT
            if h:
                p = k32.GlobalLock(h)
                text = ctypes.wstring_at(p) if p else None
                k32.GlobalUnlock(ctypes.c_void_p(h))
            user32.CloseClipboard()
            break
        time.sleep(0.05)
    return seq, text


def put_on_clipboard(text):
    k32 = ctypes.windll.kernel32
    k32.GlobalAlloc.restype = ctypes.c_void_p
    k32.GlobalLock.restype = ctypes.c_void_p
    k32.GlobalLock.argtypes = [ctypes.c_void_p]
    data = (text + "\0").encode("utf-16-le")
    h = k32.GlobalAlloc(0x0002, len(data))
    ctypes.memmove(k32.GlobalLock(h), data, len(data))
    k32.GlobalUnlock(ctypes.c_void_p(h))
    for _ in range(20):
        if user32.OpenClipboard(None):
            user32.EmptyClipboard()
            user32.SetClipboardData.argtypes = [wt.UINT, ctypes.c_void_p]
            user32.SetClipboardData(13, h)
            user32.CloseClipboard()
            return
        time.sleep(0.05)
    raise SystemExit("could not open the clipboard")


def selection_leaves_clipboard_alone(t):
    """Converting a selection must not pass it through the clipboard:
    Windows keeps clipboard history, can sync it to other devices (an
    Android phone), and any program may watch it."""
    put_on_clipboard("rt-e2e clipboard sentinel")
    before = clipboard_state()
    fs.run(t, "convert selection", "ok", "นา",
           then=[fs.select_word, fs.convert_sel], settle=1.5)
    after = clipboard_state()
    fs.check(t.name, "convert selection leaves the clipboard alone",
             f"seq+{after[0] - before[0]} {after[1]!r}", f"seq+0 {before[1]!r}")


HUMAN = __import__("random").Random(20260930)  # same "typist" every run


def human_keys(s):
    """Keys at a person's uneven pace: mostly 60-160 ms apart, sometimes a
    quick burst (25 ms), sometimes a pause (300 ms)."""
    for ch in s:
        roll = HUMAN.random()
        pause = 0.025 if roll < 0.2 else 0.3 if roll > 0.95 else HUMAN.uniform(0.06, 0.16)
        fast_keys(ch, pause=pause)


# Sentences as people type them, some from real bug reports.
REAL_SENTENCES = [
    # The first word alone, to tell which half of the next one goes wrong.
    ("Thai word alone", ";yoouh ", "วันนี้"),
    ("Thai then English, one line", ";yoouh there is ", "วันนี้ there is"),
    ("Thai sentence", "lj'wa]N,k.shsojvp ", "ส่งไฟล์มาให้หน่อย"),
    ("English then Thai", "hello l;ylfu8iy[ ", "hello สวัสดีครับ"),
    ("Thai then an English tech word", "l;ylfu8iy[ middleware ", "สวัสดีครับ middleware"),
    # In prose the prefix takes its hyphen (2.1); not in an address bar.
    ("English computer words", "relogin logout ", "re-login logout"),
]

# Targets that are address bars: nothing there is rewritten as prose.
ADDRESS_BARS = ("omnibox", "edge")


def flip_back_several(t):
    """Shift+Backspace pressed again reaches one word further back (up to
    eight). In Manual mode, so the words stay as typed until flipped."""
    fs.check(t.name, "mode set to manual", str(fs.set_mode("manual")), "True")
    try:
        fs.run(t, "Shift+Backspace once flips the last word", "l;ylfu 8iy[ ",
               "l;ylfu ครับ", then=[flip], settle=1.0)
        fs.run(t, "Shift+Backspace twice flips two words", "l;ylfu 8iy[ ",
               "สวัสดี ครับ", then=[flip, flip], settle=1.0)
        fs.run(t, "Shift+Backspace three times flips three words",
               "l;ylfu 8iy[ l;ylfu ", "สวัสดี ครับ สวัสดี",
               then=[flip, flip, flip], settle=1.0)
        fs.run(t, "a caret move forgets the recent words", "l;ylfu ", "l;ylfu",
               then=[lambda: tap(fs.END), flip], settle=1.0)
    finally:
        fs.set_mode("auto")


def prose(t, expect):
    """`expect` as it ends up in `t`: an address bar keeps `relogin`."""
    return expect.replace("re-login", "relogin") if t.name in ADDRESS_BARS else expect


def realistic(t):
    for name, keys, expect in REAL_SENTENCES:
        fs.run(t, f"typed like a person: {name}", "", prose(t, expect),
               then=[lambda keys=keys: human_keys(keys)], settle=1.2)


def sweep(t):
    run = fs.run
    run(t, "EN->TH word", "l;ylfu ", "สวัสดี")
    run(t, "letters-only Thai word", "giupo ", "เรียน")
    run(t, "EN->TH then Thai typed natively", "l;ylfu giupo ", "สวัสดี เรียน")
    run(t, "TH->EN word", "correct ", "correct", layout=HKL_TH)
    # Not Thai: it only starts like three short Thai words (พำ สน เร).
    run(t, "English computer word stays English (hyphen in prose)", "relogin ",
        prose(t, "re-login"))
    run(t, "English word with its own spelling stays", "reinstall ", "reinstall")
    # A wrong correction is undone with one Shift+Backspace, wherever it
    # happened: in the middle of a word, after a long word was handed to the
    # Thai layout, or at the space.
    run(t, "Shift+Backspace takes back a mid-word conversion", "l;ylfu", "l;ylfu", then=[flip])
    run(t, "Shift+Backspace takes back a long word", "l;ylfu8iy[", "l;ylfu8iy[", then=[flip])
    # A word Auto keeps as English (it spells Thai นา too); flipped by hand.
    run(t, "Shift+Backspace flips EN to TH", "ok", "นา", then=[flip])
    run(t, "Shift+Backspace undoes an automatic fix", "correct ", "แนพพำแะ",
        layout=HKL_TH, then=[flip])
    # Pressed once too often: with no older word to reach, the next press
    # puts the word back (it used to say "nothing to flip").
    run(t, "Shift+Backspace twice puts the word back", "reload ", "reload", then=[flip, flip])
    flip_back_several(t)
    # Found on a real PC: keys typed quickly while a word was being rewritten
    # were lost. A person typing ~30 ms per key, word ended by a space.
    fs.run(t, "fast typing through a correction", "", "สวัสดีครับ",
           then=[lambda: fast_keys("l;ylfu8iy["), lambda: tap(fs.SPACE)])
    realistic(t)
    capslock_left_on(t)
    # Thai typed in a wrong order that looks right: two เ for แ (keys g g).
    fs.run(t, "two เ typed for แ is put right", "gg,; ", "แมว", layout=HKL_TH)
    snippets_and_spelling(t)
    if t.name in ("page", "notepad"):
        keyboard_states(t)
        thai_text_tools(t)
        thai_word_delete(t)
        palette_text_tools(t)
    if t.name == "notepad":
        keyboard_map(t)
        keyboard_lock(t)
        keyboard_health(t)
        key_bounce(t)
        keys_on_screen(t)
        caret_list(t)
        ghosts(t)
        by_app(t)
        devices(t)
        hold_for_accents(t)
        shortcut_list(t)
        typing_practice(t)
        enter_guard(t)
        app_keyboards(t)
    if t.name == "page":
        shortcuts_in_english(t)
        password_tag(t)
        outside_text(t)
        full_screen_browser_still_works(t)
        palette_by_keyboard(t)
        code_mode(t)
    selection_leaves_clipboard_alone(t)


def notepad11_sweep(t):
    sweep(t)
    verify_catches_garbling(t)


def verify_catches_garbling(t):
    """Windows 11 Notepad garbles Thai typed as keys right after Backspaces.
    With the text-box path and the pause turned off (debug switches), the
    check-after-write must say what really happened, and once it has seen a
    garbled word the next one must arrive intact (it waits longer there)."""
    proc = fs.start_rt({"RIGHTTYPE_E2E_NO_TEXTBOX": "1", "RIGHTTYPE_E2E_NO_GAP": "1"})
    CURRENT[0] = proc
    t.focus()
    # Notepad garbles most of the time, not every time: type until the check
    # has caught one, then the next must arrive intact.
    caught = False
    for attempt in range(1, 6):
        start = fs.LOG.stat().st_size
        t.clear()
        t.layout(HKL_EN)
        # No space after it: the check waits for a pause in typing.
        fs.type_keys("l;ylfu")
        time.sleep(1.2)
        got = t.read()
        trace = fs.log_since(start)
        verdict = ("differs" if "verify: app shows something else" in trace
                   else "same" if "verify: correction shown as sent" in trace else "none")
        truth = "same" if got.strip() == "สวัสดี" else "differs"
        print(f"  garbling attempt {attempt}: got {got!r}, check said {verdict}", flush=True)
        fs.check(t.name, f"check-after-write tells the truth (attempt {attempt})", verdict, truth)
        if caught:
            fs.check(t.name, "after a garbled word the next arrives intact", got.strip(), "สวัสดี")
            break
        caught = verdict == "differs"
    else:
        print("  Notepad never garbled in 5 attempts", flush=True)
    # End the word, so this RightType's problem report has a word end too.
    tap(fs.SPACE)
    time.sleep(0.5)


def edge_sweep(t):
    watch(t, "EN->TH word, suggestions open", "l;ylfu", "สวัสดี")
    watch(t, "EN->TH word, typed fast", "l;ylfu", "สวัสดี", typer=fast_keys)
    watch(t, "letters-only Thai word", "giupo", "เรียน")
    sweep(t)


def ui_timing():
    """How long each RightType window takes to build and to appear painted
    (the debug build traces it). Printed, not judged."""
    import re
    for what in ["settings", "stats", "fixer", "welcome"]:
        start = fs.LOG.stat().st_size if fs.LOG.exists() else 0
        proc = fs.start_rt({"RIGHTTYPE_SHOW": what})
        time.sleep(4)
        with open(fs.LOG, encoding="utf-8", errors="replace") as f:
            f.seek(start)
            lines = [m.group(0) for m in re.finditer(r"window [^:]+: built in \d+ ms, shown painted at \d+ ms", f.read())]
        print(f"  ui {what}: {lines or 'no timing line'}", flush=True)
        proc.kill()
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
        time.sleep(0.5)


def memory_use():
    """2.3 P3: RightType's memory (working set and private bytes) when idle
    after starting, and once a word has been fixed (the dictionaries are in
    use then). Printed, not judged: the build here is a debug build."""
    class Counters(ctypes.Structure):
        _fields_ = [("cb", wt.DWORD), ("PageFaultCount", wt.DWORD),
                    ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
                    ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                    ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t),
                    ("PrivateUsage", ctypes.c_size_t)]

    def measure(pid):
        h = ctypes.windll.kernel32.OpenProcess(0x1000, False, pid)  # QUERY_LIMITED
        c = Counters()
        c.cb = ctypes.sizeof(c)
        ok = ctypes.windll.psapi.GetProcessMemoryInfo(h, ctypes.byref(c), c.cb)
        ctypes.windll.kernel32.CloseHandle(h)
        mb = lambda n: f"{n / 1048576:.1f} MB"  # noqa: E731
        return f"working set {mb(c.WorkingSetSize)}, private {mb(c.PrivateUsage)}" if ok else "unknown"

    started = time.perf_counter()
    proc = fs.start_rt()
    print(f"  memory idle after start: {measure(proc.pid)}", flush=True)
    try:
        t = Notepad()
        t.focus()
        type_keys("l;ylfu8iy[ hello ")
        time.sleep(1.5)
        print(f"  memory after a fix: {measure(proc.pid)}", flush=True)
        t.close()
    except Exception as e:
        print(f"  memory after a fix: not measured ({e})", flush=True)
    finally:
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
    print(f"  (measured in {time.perf_counter() - started:.1f} s)", flush=True)


def report_has_no_typed_text(target, results):
    """The problem report RightType kept while this target was typed in
    names what it did, never the words: none of the text the checks saw may
    be in it."""
    time.sleep(2)  # one more report tick
    try:
        report = PROBLEM_REPORT.read_text(encoding="utf-8")
    except OSError:
        fs.check(target, "problem report was written", "", "a report")
        return
    # Text the checks typed or read (not the clipboard checks' own sentinel,
    # whose words are not typed, and "clipboard" is in RightType's messages).
    # Nor the answers of checks that typed nothing ("yes", "True"): those
    # are the sweep's words, and the report says "yes" of its own.
    # (and the verdicts of checks that typed nothing: "kept", "shown",
    # "opened" — the report says "shown as sent" of its own).
    answers = {"yes", "no", "True", "False", "kept", "shown", "opened"}
    # Words of RightType's own report messages ("put back to the keys"):
    # the typo check types `teh` for "the".
    own = {"the"}
    words = {w for _, name, _, got, expect in results if "clipboard" not in name
             and got not in answers and expect not in answers
             for w in (got + " " + expect).split() if len(w) >= 3 and w not in own}
    # Whole words: "correct" is part of the report's own word "correction".
    import re
    leaked = sorted(w for w in words
                    if re.search(rf"(?<![\w\u0E00-\u0E7F]){re.escape(w)}(?![\w\u0E00-\u0E7F])", report))
    fs.check(target, "problem report has no typed text", " ".join(leaked), "")
    # How long the hook took per key in this target (2.3 P2: numbers only).
    # Every wait in the hook draws on a 200 ms budget per key: 250 ms is the
    # line no key may cross (Windows passes keys on by itself after 300).
    import re
    slowest = None
    for line in report.splitlines():
        if line.startswith(("keyboard hook time", "of it waiting")):
            print(f"[{target}] {line}", flush=True)
        m = re.match(r"keyboard hook time: .*slowest ([0-9.]+) ms", line)
        if m:
            slowest = float(m.group(1))
    if slowest is not None:
        fs.check(target, "keyboard hook stays under 250 ms per key",
                 "yes" if slowest < 250 else f"slowest {slowest} ms", "yes")
    fs.check(target, "problem report records word ends",
             "yes" if "word end" in report else "no", "yes")


def main():
    want = set(sys.argv[1:]) or {"page", "omnibox", "edge", "notepad", "notepad11", "word",
                                 "hang"}
    # Thai must be loaded for this session (CI installs it just before).
    user32.LoadKeyboardLayoutW("0000041E", 0)
    user32.LoadKeyboardLayoutW("00000409", 0)
    layouts = [f"{h:08X}" for h in installed_layouts()]
    print(f"keyboard layouts: {layouts}", flush=True)
    if not {f"{HKL_EN:08X}", f"{HKL_TH:08X}"} <= set(layouts):
        raise SystemExit("US English and Thai Kedmanee must both be installed")

    thai_capslock_probe()
    write_sweep_config()
    # Edge runs several rounds: RightType froze there in some runs and not
    # others (after a word boundary handled inside a focus callback), and one
    # clean round proves nothing.
    targets = [("page", Page), ("omnibox", Omnibox)] + [("edge", EdgeOmnibox)] * EDGE_ROUNDS + [
        ("notepad", Notepad), ("notepad11", Notepad11), ("word", Word), ("hang", HangPage)]
    sections = {}
    try:
        for key, make in targets:
            if key not in want:
                continue
            start = fs.LOG.stat().st_size if fs.LOG.exists() else 0
            # The address bar is seeded before RightType runs, so the URL is
            # typed exactly as written.
            try:
                if key in ("omnibox", "notepad11"):
                    subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)
                    t = make()
                    if key == "notepad11":
                        notepad11_probe(t)
                    proc = fs.start_rt()
                    t.focus()
                else:
                    proc = fs.start_rt()
                    t = make()
            except SkipTarget as why:
                print(f"{key} skipped: {why}", flush=True)
                continue
            except SystemExit as why:
                # The app would not start: a failure of this target, not of
                # the whole sweep.
                print(f"{key} could not start: {why}", flush=True)
                fs.check(key, "target started", str(why), "started")
                continue
            CURRENT[0] = proc
            # A target where RightType died leaves CapsLock on (its
            # Shift+CapsLock then reached Windows); do not let that fail
            # every target after it.
            if user32.GetKeyState(fs.CAPS) & 1:
                print("CapsLock was left on; turning it off", flush=True)
                tap(fs.CAPS)
            results_before = len(fs.RESULTS)
            try:
                {"edge": edge_sweep, "hang": hang_sweep, "notepad11": notepad11_sweep,
                 "word": word_sweep}.get(key, sweep)(t)
            finally:
                # A target may restart RightType (CURRENT holds the one running).
                print(f"RightType after {key}: {rt_health(CURRENT[0])}", flush=True)
                report_has_no_typed_text(key, fs.RESULTS[results_before:])
                if key != "notepad11":  # garbled on purpose there
                    alarms = fs.log_since(start).count("verify: app shows something else")
                    fs.check(key, "check-after-write raises no false alarm", str(alarms), "0")
                t.close()
                sections.setdefault(key, []).append((start, fs.LOG.stat().st_size))
    finally:
        subprocess.run(["taskkill", "/IM", "righttype.exe", "/F"], capture_output=True)

    if "instance" in want or not sys.argv[1:]:
        one_instance()
    ui_timing()
    memory_use()

    failed, known, fixed = [], [], []
    for target, name, ok, got, expect in fs.RESULTS:
        label = f"{target}: {name}"
        if label in KNOWN_FAILING:
            (fixed if ok else known).append(label)
        elif not ok:
            failed.append(f"{label} — got {got.strip()!r}, expected {expect.strip()!r}")
    passed = sum(r[2] for r in fs.RESULTS)
    # Each failing target's own part of the trace (the log is shared, so its
    # end belongs to whichever target ran last). Printed before the summary:
    # log viewers and the API return only the end of a long log, and the
    # summary is what has to be in it.
    data = fs.LOG.read_bytes() if failed else b""
    for key, spans in sections.items():
        if any(line.startswith(f"{key}:") for line in failed):
            for n, (start, end) in enumerate(spans, 1):
                part = data[start:end].decode("utf-8", errors="replace")
                print(f"\n--- RightType trace: {key} (round {n}) ---")
                print(part[-60000:])
    print(f"\nSUMMARY {passed}/{len(fs.RESULTS)} passed; trace: {fs.LOG}")
    for label in known:
        print(f"  KNOWN FAILING {label}")
    for label in fixed:
        print(f"  XPASS {label} — remove it from KNOWN_FAILING")
    for line in failed:
        print(f"  FAIL {line}")
    # Each failing case's trace once more, last: a log cut to its end (as
    # the CI tools show it) keeps these.
    for label, part in fs.FAILED_TRACES:
        print(f"\n--- failing case: {label} (its trace, last 6000 chars) ---\n{part}", flush=True)
    report = lib.HERE / "RightType-test-report.txt"
    lines = [f"SUMMARY {passed}/{len(fs.RESULTS)} passed"]
    lines += [f"FAIL {line}" for line in failed] + [f"KNOWN FAILING {x}" for x in known]
    lines += [f"{'PASS' if ok else 'FAIL'} {target}: {name}: got {got.strip()!r}"
              for target, name, ok, got, _ in fs.RESULTS]
    report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    try:
        (lib.HERE / "RightType-test-trace.txt").write_bytes(fs.LOG.read_bytes())
    except OSError:
        pass
    print(f"report: {report}", flush=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()

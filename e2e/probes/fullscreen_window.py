"""A plain full-screen window for capslock_switch.py: borderless, topmost,
covering the whole primary monitor, no text field. Prints its top-level HWND
on the first line, then runs until its stdin closes or Escape is pressed."""
import ctypes
import sys
import tkinter as tk

root = tk.Tk()
root.title("RightType e2e full-screen test window")
w, h = root.winfo_screenwidth(), root.winfo_screenheight()
root.overrideredirect(True)
root.geometry(f"{w}x{h}+0+0")
root.configure(bg="black")
root.attributes("-topmost", True)
root.update()
root.focus_force()
top = ctypes.windll.user32.GetAncestor(root.winfo_id(), 2) or root.winfo_id()
print(top, flush=True)
root.bind("<Escape>", lambda e: root.destroy())


def poll():
    # The probe closes this window by killing the process; the timeout is a
    # backstop so a crashed probe cannot leave a full-screen window behind.
    if poll.n > 600:
        root.destroy()
        return
    poll.n += 1
    root.after(500, poll)


poll.n = 0
root.after(500, poll)
root.mainloop()

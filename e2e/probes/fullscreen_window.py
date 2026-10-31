"""A plain full-screen window for capslock_switch.py: a borderless, topmost
Win32 popup covering the whole primary monitor, with no text field and no
caret (Tk makes a caret for its focus window, which RightType reads as typing).
Prints its HWND on the first line and runs until killed; a timer closes it
after 5 minutes so a crashed probe cannot leave a full-screen window behind."""
import ctypes
from ctypes import wintypes

u = ctypes.WinDLL("user32", use_last_error=True)
k = ctypes.WinDLL("kernel32", use_last_error=True)
LRESULT = ctypes.c_ssize_t
WNDPROC = ctypes.WINFUNCTYPE(LRESULT, wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM)
u.DefWindowProcW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
u.DefWindowProcW.restype = LRESULT
u.CreateWindowExW.restype = wintypes.HWND
u.CreateWindowExW.argtypes = [wintypes.DWORD, wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD,
                              ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                              wintypes.HWND, wintypes.HMENU, wintypes.HINSTANCE, wintypes.LPVOID]
u.SetTimer.argtypes = [wintypes.HWND, ctypes.c_size_t, wintypes.UINT, ctypes.c_void_p]

WM_DESTROY, WM_TIMER, WM_ERASEBKGND = 0x0002, 0x0113, 0x0014


class WNDCLASS(ctypes.Structure):
    _fields_ = [("style", wintypes.UINT), ("lpfnWndProc", WNDPROC), ("cbClsExtra", ctypes.c_int),
                ("cbWndExtra", ctypes.c_int), ("hInstance", wintypes.HINSTANCE),
                ("hIcon", wintypes.HANDLE), ("hCursor", wintypes.HANDLE),
                ("hbrBackground", wintypes.HBRUSH), ("lpszMenuName", wintypes.LPCWSTR),
                ("lpszClassName", wintypes.LPCWSTR)]


def proc(hwnd, msg, wp, lp):
    if msg == WM_TIMER or msg == WM_DESTROY:
        u.PostQuitMessage(0)
        return 0
    return u.DefWindowProcW(hwnd, msg, wp, lp)


cb = WNDPROC(proc)
wc = WNDCLASS()
wc.lpfnWndProc = cb
wc.hInstance = k.GetModuleHandleW(None)
wc.lpszClassName = "RtE2eFullScreen"
wc.hbrBackground = ctypes.windll.gdi32.GetStockObject(4)  # BLACK_BRUSH
u.RegisterClassW(ctypes.byref(wc))
w, h = u.GetSystemMetrics(0), u.GetSystemMetrics(1)
WS_POPUP, WS_VISIBLE = 0x80000000, 0x10000000
WS_EX_TOPMOST = 0x8
hwnd = u.CreateWindowExW(WS_EX_TOPMOST, wc.lpszClassName, "RightType e2e full-screen test window",
                         WS_POPUP | WS_VISIBLE, 0, 0, w, h, None, None, wc.hInstance, None)
u.ShowWindow(hwnd, 5)
u.SetForegroundWindow(hwnd)
u.SetFocus(hwnd)
u.SetTimer(hwnd, 1, 300000, None)
print(hwnd, flush=True)
msg = wintypes.MSG()
while u.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
    u.TranslateMessage(ctypes.byref(msg))
    u.DispatchMessageW(ctypes.byref(msg))

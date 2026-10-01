//! One RightType at a time, settled without a dialog — Windows only.
//!
//! Two RightTypes would both correct every word (`สวัสดี` typed once, fixed
//! twice). RightLang answered a second launch with a message box that had to
//! be clicked away. Here nothing needs clicking:
//!
//! - **The same program launched again** (same path, same version): the one
//!   already running shows "RightType is already running" for a moment and
//!   the new one leaves quietly. Opening it again usually means "where is it?".
//! - **Another program file or version** (a new download, a portable copy):
//!   launching it means "use this one", so the running one is asked to quit,
//!   the new one takes over and says which version it replaced.
//!
//! The running instance holds the mutex [`RUNNING`] and publishes its process
//! id, version and path in a small shared-memory block. Versions before 2.1
//! publish nothing; they are found by program name and closed the same way.
//!
//! **Restart after a crash** (Settings, on by default): a small watchdog copy
//! of RightType (`--watchdog`) waits for the main one to end. It starts it
//! again only when it crashed — not when it was quit, closed by Task Manager
//! or replaced — at most [`MAX_RESTARTS`] times in a row, and never while
//! Windows is shutting down. The watchdog has no hook, no window and reads
//! nothing.

use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, Thread32First, Thread32Next,
    PROCESSENTRY32W, TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
    FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, GetCurrentProcessId, GetExitCodeProcess, OpenEventW, OpenProcess,
    SetEvent, TerminateProcess, WaitForMultipleObjects, WaitForSingleObject, INFINITE,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    SYNCHRONIZATION_SYNCHRONIZE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, PostMessageW, PostThreadMessageW, SM_SHUTTINGDOWN, WM_QUIT,
};

/// Held by the running RightType for as long as it runs.
const RUNNING: PCWSTR = w!("Local\\RightType.Running");
/// Who holds [`RUNNING`]: an [`Owner`].
const OWNER: PCWSTR = w!("Local\\RightType.Owner");
/// Set by a second launch of the same program: "show that you are running".
const HELLO: PCWSTR = w!("Local\\RightType.Hello");
/// Set by another version or copy: "quit, I am taking over".
const QUIT: PCWSTR = w!("Local\\RightType.Quit");

/// Posted to the tray window; `wparam` is [`HELLO_MSG`] or [`QUIT_MSG`].
pub const WM_INSTANCE: u32 = 0x8000 + 0x570;
pub const HELLO_MSG: usize = 1;
pub const QUIT_MSG: usize = 2;

/// How long a RightType asked to quit gets before it is ended.
const QUIT_WAIT: Duration = Duration::from_secs(3);
/// Crash restarts in a row before the watchdog gives up.
pub const MAX_RESTARTS: u32 = 3;
/// A run this long resets the restart count: the crash was not a loop.
const STEADY_RUN: Duration = Duration::from_secs(10 * 60);

/// What the running instance publishes about itself.
#[repr(C)]
#[derive(Clone, Copy)]
struct Owner {
    pid: u32,
    version: [u16; 32],
    path: [u16; 520],
}

/// How this launch should go on.
pub enum Start {
    /// Run. `replaced` names the version (or "" when unknown) closed to make
    /// way for this one.
    Run { replaced: Option<String> },
    /// Another RightType is running and was told to show itself; leave.
    Leave,
}

/// Handles kept open for the life of the process.
static MUTEX: AtomicIsize = AtomicIsize::new(0);
static TRAY: AtomicIsize = AtomicIsize::new(0);
static LISTENING: AtomicBool = AtomicBool::new(false);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(s: &[u16]) -> String {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..end])
}

fn copy_into(dst: &mut [u16], s: &str) {
    let room = dst.len() - 1;
    for (d, c) in dst
        .iter_mut()
        .zip(s.encode_utf16().take(room).chain(std::iter::repeat(0)))
    {
        *d = c;
    }
}

fn own_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Decide whether this launch runs, settling any RightType already running.
pub fn claim() -> Start {
    unsafe {
        let Ok(mutex) = CreateMutexW(None, false, RUNNING) else {
            // Cannot tell: run, as before 2.1.
            return Start::Run { replaced: None };
        };
        let existed = GetLastError() == ERROR_ALREADY_EXISTS;
        let mut replaced = None;
        if existed {
            // The holder may still be starting and not have published itself.
            let mut owner = read_owner();
            for _ in 0..8 {
                if owner.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
                owner = read_owner();
            }
            match owner {
                Some(owner) if same_program(&owner) => {
                    signal(HELLO);
                    let _ = CloseHandle(mutex);
                    return Start::Leave;
                }
                Some(owner) => {
                    signal(QUIT);
                    end_process(owner.pid, false);
                    replaced = Some(from_wide(&owner.version));
                }
                // Holder gone between the two calls: the mutex is free below.
                None => {}
            }
            // The old one releases the mutex as it exits (or abandons it if
            // it was ended); either way it is ours after this wait.
            let _ = WaitForSingleObject(mutex, QUIT_WAIT.as_millis() as u32 + 2000);
        } else {
            let _ = WaitForSingleObject(mutex, 0);
            if close_legacy() {
                replaced = Some(String::new());
            }
        }
        MUTEX.store(mutex.0 as isize, Ordering::Relaxed);
        publish_owner();
        Start::Run { replaced }
    }
}

fn same_program(owner: &Owner) -> bool {
    from_wide(&owner.version) == env!("CARGO_PKG_VERSION")
        && from_wide(&owner.path).eq_ignore_ascii_case(&own_path())
}

unsafe fn read_owner() -> Option<Owner> {
    let map = OpenFileMappingW(FILE_MAP_READ.0, false, OWNER).ok()?;
    let view = MapViewOfFile(map, FILE_MAP_READ, 0, 0, std::mem::size_of::<Owner>());
    let owner = (!view.Value.is_null()).then(|| *(view.Value as *const Owner));
    if !view.Value.is_null() {
        let _ = UnmapViewOfFile(view);
    }
    let _ = CloseHandle(map);
    owner.filter(|o| o.pid != 0)
}

/// Publish who we are. The mapping stays open (and so alive) until exit.
unsafe fn publish_owner() {
    let Ok(map) = CreateFileMappingW(
        HANDLE(-1isize as *mut _),
        None,
        PAGE_READWRITE,
        0,
        std::mem::size_of::<Owner>() as u32,
        OWNER,
    ) else {
        return;
    };
    let view: MEMORY_MAPPED_VIEW_ADDRESS =
        MapViewOfFile(map, FILE_MAP_WRITE, 0, 0, std::mem::size_of::<Owner>());
    if view.Value.is_null() {
        return;
    }
    let mut owner = Owner {
        pid: GetCurrentProcessId(),
        version: [0; 32],
        path: [0; 520],
    };
    copy_into(&mut owner.version, env!("CARGO_PKG_VERSION"));
    copy_into(&mut owner.path, &own_path());
    *(view.Value as *mut Owner) = owner;
    // Keep `map` and `view` for the life of the process.
}

unsafe fn signal(name: PCWSTR) {
    if let Ok(ev) = OpenEventW(
        windows::Win32::System::Threading::EVENT_MODIFY_STATE,
        false,
        name,
    ) {
        let _ = SetEvent(ev);
        let _ = CloseHandle(ev);
    }
}

/// Wait for `pid` to exit after it was asked to; end it if it does not.
/// `post_quit` also posts WM_QUIT to its threads (for versions before 2.1,
/// which do not listen for [`QUIT`]).
unsafe fn end_process(pid: u32, post_quit: bool) {
    let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, false, pid) else {
        return;
    };
    if post_quit {
        for thread in threads_of(pid) {
            let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    if WaitForSingleObject(process, QUIT_WAIT.as_millis() as u32) != WAIT_OBJECT_0 {
        // Exit code 0: its watchdog must not restart it.
        let _ = TerminateProcess(process, 0);
        let _ = WaitForSingleObject(process, 2000);
    }
    let _ = CloseHandle(process);
}

unsafe fn threads_of(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else {
        return out;
    };
    let mut t = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    if Thread32First(snap, &mut t).is_ok() {
        loop {
            if t.th32OwnerProcessID == pid {
                out.push(t.th32ThreadID);
            }
            if Thread32Next(snap, &mut t).is_err() {
                break;
            }
        }
    }
    let _ = CloseHandle(snap);
    out
}

/// Close a RightType from before 2.1 (no mutex) running under our program
/// name. Watchdogs are left alone. Returns whether one was closed.
unsafe fn close_legacy() -> bool {
    let me = GetCurrentProcessId();
    let name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "righttype.exe".into());
    let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
        return false;
    };
    let mut found = Vec::new();
    let mut p = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if Process32FirstW(snap, &mut p).is_ok() {
        loop {
            let exe = from_wide(&p.szExeFile);
            if p.th32ProcessID != me && exe.eq_ignore_ascii_case(&name) {
                found.push(p.th32ProcessID);
            }
            if Process32NextW(snap, &mut p).is_err() {
                break;
            }
        }
    }
    let _ = CloseHandle(snap);
    let mut closed = false;
    for pid in found {
        if is_watchdog(pid) {
            continue;
        }
        end_process(pid, true);
        closed = true;
    }
    closed
}

fn watchdog_event_name(pid: u32) -> Vec<u16> {
    wide(&format!("Local\\RightType.Watchdog.{pid}"))
}

unsafe fn is_watchdog(pid: u32) -> bool {
    let name = watchdog_event_name(pid);
    match OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(name.as_ptr())) {
        Ok(h) => {
            let _ = CloseHandle(h);
            true
        }
        Err(_) => false,
    }
}

/// Listen for [`HELLO`] and [`QUIT`] from later launches, posting
/// [`WM_INSTANCE`] to the tray window `hwnd`.
pub fn listen(hwnd: isize) {
    TRAY.store(hwnd, Ordering::Relaxed);
    if LISTENING.swap(true, Ordering::Relaxed) {
        return;
    }
    unsafe {
        let (Ok(hello), Ok(quit)) = (
            CreateEventW(None, false, false, HELLO),
            CreateEventW(None, false, false, QUIT),
        ) else {
            return;
        };
        let (hello, quit) = (hello.0 as isize, quit.0 as isize);
        let _ = std::thread::Builder::new()
            .name("instance".into())
            .spawn(move || loop {
                let handles = [HANDLE(hello as *mut _), HANDLE(quit as *mut _)];
                let r = WaitForMultipleObjects(&handles, false, INFINITE);
                let what = match r.0.wrapping_sub(WAIT_OBJECT_0.0) {
                    0 => HELLO_MSG,
                    1 => QUIT_MSG,
                    _ => return,
                };
                let tray = HWND(TRAY.load(Ordering::Relaxed) as *mut _);
                let _ = PostMessageW(tray, WM_INSTANCE, WPARAM(what), LPARAM(0));
            });
    }
}

/// What to tell the typist once the tray is up.
static NOTICE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Remember what this launch did: replaced another version, or came back
/// after a crash.
pub fn set_notice(replaced: Option<String>, restarted: Option<u32>) {
    use righttype::i18n::{tr, trf, T};
    let v = env!("CARGO_PKG_VERSION");
    let text = match (replaced, restarted) {
        (_, Some(_)) => Some(tr(T::ToastRestarted).to_string()),
        (Some(old), None) if old.is_empty() || old == v => {
            Some(trf(T::ToastReplacedUnknown, &[("v", v)]))
        }
        (Some(old), None) => Some(trf(T::ToastReplaced, &[("v", v), ("old", &old)])),
        (None, None) => None,
    };
    *NOTICE.lock().unwrap() = text;
}

/// The notice from [`set_notice`], once.
pub fn take_notice() -> Option<String> {
    NOTICE.lock().unwrap().take()
}

// ------------------------------------------------------------------ watchdog

/// The watchdog runs in release builds; a debug build (the e2e tests, which
/// end RightType themselves) only with `RIGHTTYPE_E2E_WATCHDOG`.
pub fn watchdog_allowed() -> bool {
    !cfg!(debug_assertions) || std::env::var_os("RIGHTTYPE_E2E_WATCHDOG").is_some()
}

/// Should a RightType that ended with `code` be started again?
///
/// Only a crash: an NT status of severity "error" (access violation, stack
/// overflow, fail-fast…) or Rust's exit code for a panic. Quitting returns 0;
/// Task Manager and `taskkill /F` return 1.
pub fn is_crash(code: u32) -> bool {
    code >= 0xC000_0000 || code == 101
}

/// Start the watchdog for this process (Settings → restart after a crash).
/// `restarts` is how many crash restarts led to this run.
pub fn start_watchdog(restarts: u32) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let pid = unsafe { GetCurrentProcessId() };
    let _ = std::process::Command::new(exe)
        .arg("--watchdog")
        .arg(pid.to_string())
        .arg(restarts.to_string())
        .creation_flags(DETACHED_PROCESS)
        .spawn();
}

/// `--watchdog <pid> <restarts>`: wait for `pid` to end and restart it if it
/// crashed. Runs instead of the app; never returns to it.
pub fn run_watchdog(args: &[String]) -> ! {
    let pid: u32 = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
    let restarts: u32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(0);
    unsafe {
        let me = watchdog_event_name(GetCurrentProcessId());
        let _marker = CreateEventW(None, true, false, PCWSTR(me.as_ptr()));
        let Ok(process) = OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        ) else {
            std::process::exit(0);
        };
        let started = Instant::now();
        let _ = WaitForSingleObject(process, INFINITE);
        let mut code = 0u32;
        let _ = GetExitCodeProcess(process, &mut code);
        let _ = CloseHandle(process);
        let shutting_down = GetSystemMetrics(SM_SHUTTINGDOWN) != 0;
        let restarts = if started.elapsed() >= STEADY_RUN {
            0
        } else {
            restarts
        };
        if is_crash(code) && !shutting_down && restarts < MAX_RESTARTS {
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe)
                    .arg("--restarted")
                    .arg((restarts + 1).to_string())
                    .spawn();
            }
        }
    }
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_crash_is_restarted() {
        assert!(is_crash(0xC000_0005)); // access violation
        assert!(is_crash(0xC000_0409)); // fail-fast (Rust abort)
        assert!(is_crash(101)); // Rust panic
        assert!(!is_crash(0)); // quit, or replaced by another version
        assert!(!is_crash(1)); // Task Manager / taskkill /F
    }

    #[test]
    fn names_are_cut_to_fit() {
        let mut buf = [0u16; 4];
        copy_into(&mut buf, "2.10.0");
        assert_eq!(from_wide(&buf), "2.1");
        copy_into(&mut buf, "a");
        assert_eq!(from_wide(&buf), "a");
    }
}

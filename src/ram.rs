//! RAM hardening — Windows only. Reduces how a secret in memory could ever
//! reach disk or a crash artifact, beyond what `zeroize` alone covers.
//!
//! - [`harden_process`] — excludes heap memory from any Windows Error Reporting
//!   crash dump (`WerSetFlags(WER_FAULT_REPORTING_FLAG_NOHEAP)`) and suppresses
//!   the fault dialog, so a crash can't hand a debugger/report our secret buffer.
//! - [`lock_region`] — pins a buffer's pages in physical RAM (`VirtualLock`) so
//!   the OS can never write it to the pagefile or a hibernation file. Intended
//!   for [`crate::buffer::WordBuffer::stable_region`], which is guaranteed not
//!   to reallocate — locking a region that later moves would silently stop
//!   protecting the *new* allocation.
//!
//! Every call here is best-effort: on failure we simply don't get the extra
//! protection (RightType still runs, and `zeroize`/the secret-shaped bail-out
//! remain the primary defenses) — never a reason to crash or degrade the app.

use windows::Win32::System::Diagnostics::Debug::{
    SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX,
};
use windows::Win32::System::ErrorReporting::{WerSetFlags, WER_FAULT_REPORTING_FLAG_NOHEAP};
use windows::Win32::System::Memory::VirtualLock;

/// Apply process-wide hardening. Call once, early at startup.
///
/// # Safety
/// Trivially safe to call (no pointers involved); marked `unsafe` only because
/// the underlying Win32 calls are.
pub unsafe fn harden_process() {
    // Don't show the "Windows has stopped working" dialog for this process —
    // one less path that keeps the process alive (and its memory intact) after
    // a fault, waiting on user interaction or a debugger to attach.
    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    // If Windows Error Reporting does generate a report anyway, exclude heap
    // memory from it — that's exactly where our word buffer lives.
    let _ = WerSetFlags(WER_FAULT_REPORTING_FLAG_NOHEAP);
}

/// Lock `len` bytes at `ptr` into physical RAM so they can never be paged to
/// disk (swap file / hibernation file). Best-effort: does nothing if `len` is
/// zero, and silently no-ops on failure (e.g. the per-process working-set-lock
/// quota is exhausted — the buffer this guards is only tens of bytes, so that
/// should never happen in practice).
///
/// # Safety
/// `ptr` must be valid for `len` bytes for as long as the lock should hold, and
/// the caller must not let the underlying allocation move or be freed while
/// still locked.
pub unsafe fn lock_region(ptr: *const u8, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    let locked = VirtualLock(ptr as *const _, len).is_ok();
    // No matching unlock: this buffer lives for the whole process, and Windows
    // automatically unlocks (and reclaims) all VirtualLock'd pages when the
    // process exits — an explicit VirtualUnlock has nothing left to protect.
    locked
}

#[cfg(test)]
mod tests {
    use super::{harden_process, lock_region};
    use windows::Win32::System::ErrorReporting::{WerGetFlags, WER_FAULT_REPORTING_FLAG_NOHEAP};
    use windows::Win32::System::Memory::VirtualUnlock;
    use windows::Win32::System::Threading::GetCurrentProcess;

    #[test]
    fn virtual_lock_accepts_a_small_stable_allocation() {
        let allocation = Box::new([0u8; 64]);
        let ptr = allocation.as_ptr();
        let len = allocation.len();
        assert!(unsafe { lock_region(ptr, len) });
        assert!(unsafe { VirtualUnlock(ptr as *const _, len) }.is_ok());
    }

    #[test]
    fn process_hardening_sets_wer_no_heap_flag() {
        unsafe { harden_process() };
        let flags = unsafe { WerGetFlags(GetCurrentProcess()) }.unwrap();
        assert_ne!(flags.0 & WER_FAULT_REPORTING_FLAG_NOHEAP.0, 0);
    }
}

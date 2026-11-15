//! The lock: a named mutex shared with `launcher.ps1`, so that WSL calls (through the launcher) and Windows calls
//! (this exe) run one at a time and each window matches one 1Password prompt.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use windows::core::HSTRING;

/// A held lock, released on drop.
pub struct Held(HANDLE);

impl Drop for Held {
    fn drop(&mut self) {
        // SAFETY: we own the handle and hold the mutex.
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

pub enum Acquired {
    Held(Held),
    TimedOut,
    Interrupted,
}

/// Takes the mutex, waiting up to `wait` in 250 ms slices. `on_wait` runs once, `notice_after` after the first try
/// failed (zero: at once; the runner waits a second, as the launcher does); `interrupted` is polled between slices.
pub fn acquire(
    name: &str,
    wait: Duration,
    notice_after: Duration,
    on_wait: impl FnOnce(),
    interrupted: impl Fn() -> bool,
) -> windows::core::Result<Acquired> {
    // SAFETY: CreateMutexW with a valid name; the handle is closed by `Held` or below.
    let h = unsafe { CreateMutexW(None, false, &HSTRING::from(name))? };
    let got = |r| r == WAIT_OBJECT_0 || r == WAIT_ABANDONED;
    // SAFETY: valid handle.
    if got(unsafe { WaitForSingleObject(h, 0) }) {
        return Ok(Acquired::Held(Held(h)));
    }
    let started = Instant::now();
    let deadline = started + wait;
    let mut on_wait = Some(on_wait);
    loop {
        if started.elapsed() >= notice_after
            && let Some(f) = on_wait.take()
        {
            f();
        }
        if interrupted() {
            // SAFETY: we own the handle and do not hold the mutex.
            unsafe {
                let _ = CloseHandle(h);
            }
            return Ok(Acquired::Interrupted);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            // SAFETY: as above.
            unsafe {
                let _ = CloseHandle(h);
            }
            return Ok(Acquired::TimedOut);
        }
        let slice = left.min(Duration::from_millis(250)).as_millis() as u32;
        // SAFETY: valid handle.
        let r = unsafe { WaitForSingleObject(h, slice) };
        if got(r) {
            return Ok(Acquired::Held(Held(h)));
        }
        if r != WAIT_TIMEOUT {
            // WAIT_FAILED: give up waiting rather than spin.
            // SAFETY: as above.
            unsafe {
                let _ = CloseHandle(h);
            }
            return Err(windows::core::Error::from_thread());
        }
    }
}

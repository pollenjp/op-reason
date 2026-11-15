//! The Job Object that `op.exe` runs in (design `2026-11-15-wsl-through-exe-design.md` §6). It ends its processes when
//! its last handle closes, and only this exe holds that handle: when the exe dies (killed, or from WSL however
//! Windows ends it), the system closes the handle and ends `op.exe`, what it started, and so its 1Password prompt.
//!
//! `op.exe` is created suspended and resumed only once it is in the job, so nothing it starts can escape the job.

use std::os::windows::io::AsRawHandle;
use std::process::Child;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};
use windows::core::PCWSTR;

/// A job that ends its processes when this exe lets go of it.
pub struct Job(HANDLE);

// SAFETY: a job handle may be used from any thread.
unsafe impl Send for Job {}

impl Job {
    /// A new, unnamed job with kill-on-close.
    pub fn new() -> windows::core::Result<Job> {
        // SAFETY: no security attributes and no name; the handle is owned by the returned value.
        let job = Job(unsafe { CreateJobObjectW(None, PCWSTR::null())? });
        job.set_limits(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)?;
        Ok(job)
    }

    fn set_limits(&self, flags: JOB_OBJECT_LIMIT) -> windows::core::Result<()> {
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = flags;
        // SAFETY: `info` is a valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION of the size passed, alive for the call.
        unsafe {
            SetInformationJobObject(
                self.0,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
    }

    /// Puts a process into the job.
    pub fn assign(&self, child: &Child) -> windows::core::Result<()> {
        // SAFETY: both handles are valid while `self` and `child` live.
        unsafe { AssignProcessToJobObject(self.0, HANDLE(child.as_raw_handle())) }
    }

    /// Clears kill-on-close, once `op.exe` has ended by itself: what `op run` left running keeps running, as it does
    /// under the launcher.
    pub fn release(&self) {
        let _ = self.set_limits(JOB_OBJECT_LIMIT(0));
    }

    /// Ends every process of the job (`op.exe` and what it started).
    pub fn terminate(&self) {
        // SAFETY: valid handle.
        unsafe {
            let _ = TerminateJobObject(self.0, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: we own the handle. With kill-on-close still set, this ends the job's processes.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Resumes the thread of a process that was created suspended. Returns whether a thread was resumed.
pub fn resume(pid: u32) -> bool {
    let mut resumed = false;
    // SAFETY: Toolhelp calls with a properly sized entry; every handle opened here is closed here.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else {
            return false;
        };
        let mut e = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut ok = Thread32First(snap, &mut e).is_ok();
        while ok {
            if e.th32OwnerProcessID == pid
                && let Ok(t) = OpenThread(THREAD_SUSPEND_RESUME, false, e.th32ThreadID)
            {
                if ResumeThread(t) != u32::MAX {
                    resumed = true;
                }
                let _ = CloseHandle(t);
            }
            ok = Thread32Next(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    resumed
}

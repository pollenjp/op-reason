//! What the relay reads of the process that connected and of its ancestors (design §5.2, §7): the chain from one
//! Toolhelp snapshot with start times and command lines, and the client's environment and current directory.
//!
//! A process of the same user may be read with `PROCESS_QUERY_LIMITED_INFORMATION` (start time, image, command line
//! through `ProcessCommandLineInformation`) and `PROCESS_VM_READ` (the environment and the directory, from its PEB). What
//! cannot be read stays empty: the request then counts as a call of its own, never as an allowed one.

use std::collections::HashMap;
use std::ffi::c_void;

use windows::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessCommandLineInformation,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE, STATUS_INFO_LENGTH_MISMATCH, WAIT_OBJECT_0};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_VM_READ,
    WaitForSingleObject,
};

use crate::win::procs;

/// One process of a chain.
#[derive(Debug, Clone)]
pub struct Link {
    pub pid: u32,
    pub name: String,
    /// The creation time (100 ns units since 1601), 0 when it cannot be read.
    pub start: u64,
    /// The command line, when it can be read.
    pub cmd: Option<String>,
}

/// The process `pid` and its ancestors, nearest first, from one snapshot (at most 64). A parent created after its
/// child ends the chain: that PID was reused.
pub fn chain(pid: u32) -> Vec<Link> {
    let snap = procs::snapshot();
    let by_pid: HashMap<u32, &procs::Proc> = snap.iter().map(|p| (p.pid, p)).collect();
    let mut out: Vec<Link> = Vec::new();
    let mut cur = pid;
    for _ in 0..64 {
        let Some(p) = by_pid.get(&cur) else {
            break;
        };
        let start = procs::created(cur).unwrap_or(0);
        if let Some(child) = out.last()
            && start != 0
            && child.start != 0
            && start > child.start
        {
            break;
        }
        out.push(Link {
            pid: cur,
            name: p.exe.clone(),
            start,
            cmd: command_line(cur),
        });
        if p.ppid == 0 || p.ppid == cur {
            break;
        }
        cur = p.ppid;
    }
    out
}

/// Whether the process `pid` that started at `start` still runs (a reused PID does not count).
pub fn alive(pid: u32, start: u64) -> bool {
    // SAFETY: the handle is closed before returning.
    unsafe {
        let Ok(h) = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        ) else {
            return false;
        };
        let running = WaitForSingleObject(h, 0) != WAIT_OBJECT_0;
        let _ = CloseHandle(h);
        running && (start == 0 || procs::created(pid) == Some(start))
    }
}

/// `PROCESS_BASIC_INFORMATION` (x64 layout).
#[repr(C)]
#[derive(Default)]
struct BasicInfo {
    exit_status: i32,
    peb: usize,
    affinity: usize,
    base_priority: i32,
    pid: usize,
    parent: usize,
}

/// The command line of a process, through `ProcessCommandLineInformation` (Windows 8.1 and later).
pub fn command_line(pid: u32) -> Option<String> {
    // SAFETY: the handle is closed before returning; the buffer (8-byte aligned) outlives the calls, and the string is
    // read only inside it.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u64; 512];
        let mut status;
        loop {
            let mut ret = 0u32;
            status = NtQueryInformationProcess(
                h,
                ProcessCommandLineInformation,
                buf.as_mut_ptr().cast::<c_void>(),
                (buf.len() * 8) as u32,
                &mut ret,
            );
            let want = (ret as usize).div_ceil(8);
            if status == STATUS_INFO_LENGTH_MISMATCH && want > buf.len() && want < 1 << 17 {
                buf.resize(want, 0);
                continue;
            }
            break;
        }
        let _ = CloseHandle(h);
        if status.is_err() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len() * 8);
        // a UNICODE_STRING (length in bytes, then a pointer into the same buffer)
        let len = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        let ptr = usize::from_le_bytes(bytes[8..16].try_into().ok()?);
        let off = ptr.checked_sub(bytes.as_ptr() as usize)?;
        let text = bytes.get(off..off.checked_add(len)?)?;
        Some(utf16(text))
    }
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
}

/// What the relay reads inside the client: its environment and its current directory.
#[derive(Debug, Clone, Default)]
pub struct Inside {
    pub env: Vec<(String, String)>,
    pub cwd: Option<String>,
}

impl Inside {
    /// A variable (names without regard to case, as Windows has them).
    pub fn var(&self, name: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// The environment and the directory of a process, from its PEB (`RTL_USER_PROCESS_PARAMETERS` at 0x20 of the PEB; in
/// it the current directory at 0x38, the environment at 0x80 and its size at 0x3F0, the x64 layout since Windows Vista).
pub fn inside(pid: u32) -> Option<Inside> {
    // SAFETY: the handle is closed before returning.
    unsafe {
        let h = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            false,
            pid,
        )
        .ok()?;
        let r = read_inside(h);
        let _ = CloseHandle(h);
        r
    }
}

/// Reads `buf.len()` bytes at `at` in the process.
///
/// # Safety
///
/// `h` is a process handle with `PROCESS_VM_READ`.
unsafe fn read(h: HANDLE, at: usize, buf: &mut [u8]) -> Option<()> {
    if at == 0 {
        return None;
    }
    let mut got = 0usize;
    // SAFETY: (caller) a valid handle; the buffer is ours and as long as asked for.
    unsafe {
        ReadProcessMemory(
            h,
            at as *const c_void,
            buf.as_mut_ptr().cast::<c_void>(),
            buf.len(),
            Some(&mut got),
        )
        .ok()?;
    }
    (got == buf.len()).then_some(())
}

fn word(bytes: &[u8], at: usize) -> Option<usize> {
    Some(usize::from_le_bytes(
        bytes.get(at..at + 8)?.try_into().ok()?,
    ))
}

/// # Safety
///
/// `h` is a process handle with `PROCESS_QUERY_LIMITED_INFORMATION` and `PROCESS_VM_READ`.
unsafe fn read_inside(h: HANDLE) -> Option<Inside> {
    let mut info = BasicInfo::default();
    let mut ret = 0u32;
    // SAFETY: (caller) a valid handle; the structure is ours and as large as asked.
    let status = unsafe {
        NtQueryInformationProcess(
            h,
            ProcessBasicInformation,
            (&mut info as *mut BasicInfo).cast::<c_void>(),
            size_of::<BasicInfo>() as u32,
            &mut ret,
        )
    };
    if status.is_err() || info.peb == 0 {
        return None;
    }
    let mut peb = [0u8; 0x28];
    // SAFETY: (caller) as above.
    unsafe { read(h, info.peb, &mut peb)? };
    let params_at = word(&peb, 0x20)?;
    let mut params = [0u8; 0x400];
    // SAFETY: (caller) as above.
    unsafe { read(h, params_at, &mut params)? };
    let cwd_len = u16::from_le_bytes([params[0x38], params[0x39]]) as usize;
    let cwd_at = word(&params, 0x40)?;
    let env_at = word(&params, 0x80)?;
    let env_size = word(&params, 0x3F0)?;
    let cwd = (cwd_len > 0 && cwd_len <= 0x10000)
        .then(|| {
            let mut b = vec![0u8; cwd_len];
            // SAFETY: (caller) as above.
            unsafe { read(h, cwd_at, &mut b) }.map(|_| utf16(&b))
        })
        .flatten()
        .map(|d| d.trim_end_matches('\\').to_owned());
    // The environment's size is known since Windows Vista; without it, a first guess and a smaller one.
    let mut env = Vec::new();
    let sizes: &[usize] = if env_size > 0 && env_size <= 1 << 20 {
        &[env_size]
    } else {
        &[32 * 1024, 4 * 1024]
    };
    for &size in sizes {
        let mut b = vec![0u8; size & !1];
        // SAFETY: (caller) as above.
        if unsafe { read(h, env_at, &mut b) }.is_some() {
            env = parse_env(&b);
            break;
        }
    }
    Some(Inside { env, cwd })
}

/// `NAME=VALUE` strings separated by NUL and ended by an empty one; the hidden `=C:` entries are left out.
fn parse_env(bytes: &[u8]) -> Vec<(String, String)> {
    utf16(bytes)
        .split('\0')
        .take_while(|e| !e.is_empty())
        .filter(|e| !e.starts_with('='))
        .filter_map(|e| e.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

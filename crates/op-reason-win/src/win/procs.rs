//! The process list: who our ancestors are (to find Claude Code) and how many other op.exe run (a prompt without
//! our window may belong to one of them).

use std::collections::HashMap;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, FILETIME};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::core::PWSTR;

/// One process of the snapshot.
#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub exe: String,
}

/// All processes right now (empty when the snapshot fails).
pub fn snapshot() -> Vec<Proc> {
    let mut out = Vec::new();
    // SAFETY: Toolhelp calls with a properly sized entry; the snapshot handle is closed at the end.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut e = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let len = e
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(e.szExeFile.len());
            out.push(Proc {
                pid: e.th32ProcessID,
                ppid: e.th32ParentProcessID,
                exe: String::from_utf16_lossy(&e.szExeFile[..len]),
            });
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// The creation time of a process (100-ns units), when we may open it.
pub(crate) fn created(pid: u32) -> Option<u64> {
    // SAFETY: the handle is closed before returning; out parameters are on the stack.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let (mut c, mut x, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let r = GetProcessTimes(h, &mut c, &mut x, &mut k, &mut u);
        let _ = CloseHandle(h);
        r.ok()?;
        Some((u64::from(c.dwHighDateTime) << 32) | u64::from(c.dwLowDateTime))
    }
}

/// This process and its ancestors, nearest first: `(pid, image name)`, at most 64. Stops at a parent created after its
/// child: that PID was reused by an unrelated process.
pub fn lineage(procs: &[Proc]) -> Vec<(u32, String)> {
    let by_pid: HashMap<u32, &Proc> = procs.iter().map(|p| (p.pid, p)).collect();
    let mut cur = std::process::id();
    let mut out = Vec::new();
    let Some(me) = by_pid.get(&cur) else {
        return out;
    };
    out.push((cur, me.exe.clone()));
    let mut cur_created = created(cur);
    for _ in 0..64 {
        let Some(ppid) = by_pid.get(&cur).map(|p| p.ppid) else {
            break;
        };
        if ppid == 0 || ppid == cur {
            break;
        }
        let Some(parent) = by_pid.get(&ppid) else {
            break;
        };
        let parent_created = created(ppid);
        if let (Some(p), Some(c)) = (parent_created, cur_created)
            && p > c
        {
            break;
        }
        out.push((ppid, parent.exe.clone()));
        cur = ppid;
        cur_created = parent_created;
    }
    out
}

/// The full path of a process's image, when we may open it.
pub(crate) fn image_path(pid: u32) -> Option<String> {
    let mut buf = [0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY: the buffer and its length live across the call; the handle is closed before returning.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let r =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// How many processes have the image name `name` (other op.exe: a prompt without our window may be theirs), not
/// counting the PIDs in `except` (this process, our own op.exe) nor processes whose image is `not_image` (other
/// op-reason calls, when this exe was installed under the name `op.exe`).
pub fn count_named(name: &str, except: &[u32], not_image: Option<&Path>) -> usize {
    let not_image = not_image.map(|p| p.to_string_lossy().to_lowercase());
    snapshot()
        .iter()
        .filter(|p| p.exe.eq_ignore_ascii_case(name) && !except.contains(&p.pid))
        .filter(|p| match &not_image {
            Some(me) => image_path(p.pid).is_none_or(|path| path.to_lowercase() != *me),
            None => true,
        })
        .count()
}

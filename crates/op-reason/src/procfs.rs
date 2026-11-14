//! What `/proc` tells: the Claude Code ancestor, and other op-reason processes connected through pipes.

use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;

/// The nearest ancestor (up to 64 levels) whose `comm` is `claude`, walking `<root>/<pid>/stat` from `start`.
pub fn find_claude(root: &Path, start: u32) -> Option<u32> {
    let mut pid = start;
    for _ in 0..64 {
        let stat = fs::read_to_string(root.join(pid.to_string()).join("stat")).ok()?;
        // comm is in parentheses and may contain spaces and parentheses: read after the last ") "
        let after = &stat[stat.rfind(") ")? + 2..];
        let ppid: u32 = after.split_whitespace().nth(1)?.parse().ok()?;
        if ppid <= 1 {
            return None;
        }
        let comm = fs::read_to_string(root.join(ppid.to_string()).join("comm")).ok()?;
        if comm.trim_end_matches('\n') == "claude" {
            return Some(ppid);
        }
        pid = ppid;
    }
    None
}

/// How far up a pipe chain to look.
const PIPE_DEPTH: usize = 4;

/// The pipe behind `/proc/<pid>/fd/<fd>`, as (device, inode), when that descriptor is a pipe.
fn pipe_of(pid: &str, fd: u32) -> Option<(u64, u64)> {
    let meta = fs::metadata(format!("/proc/{pid}/fd/{fd}")).ok()?;
    meta.file_type().is_fifo().then(|| (meta.dev(), meta.ino()))
}

/// Whether our own descriptor is a pipe.
pub fn own_fd_is_pipe(fd: u32) -> bool {
    pipe_of("self", fd).is_some()
}

/// Processes whose stdout (fd 1) is one of these pipes.
fn writers_of(pipes: &[(u64, u64)]) -> Vec<u32> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut writers = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        if let Some(p) = pipe_of(name, 1)
            && pipes.contains(&p)
        {
            writers.push(pid);
        }
    }
    writers
}

/// The writers of `pid`'s stdin pipe, the writers of their stdin pipes, and so on, up to [`PIPE_DEPTH`] levels.
fn upstream_of(pid: u32) -> Vec<u32> {
    let mut upstream = Vec::new();
    let mut frontier = vec![pid];
    let mut seen = std::collections::HashSet::new();
    for _ in 0..PIPE_DEPTH {
        let pipes: Vec<(u64, u64)> = frontier
            .iter()
            .filter_map(|p| pipe_of(&p.to_string(), 0))
            .collect();
        if pipes.is_empty() {
            break;
        }
        frontier = writers_of(&pipes)
            .into_iter()
            .filter(|w| seen.insert(*w))
            .collect();
        if frontier.is_empty() {
            break;
        }
        upstream.extend(&frontier);
    }
    upstream
}

/// Whether `pid` runs the same executable as this process (an op-reason, called by any name).
pub fn is_op_reason(pid: u32) -> bool {
    let (Ok(theirs), Ok(ours)) = (
        fs::metadata(format!("/proc/{pid}/exe")),
        fs::metadata("/proc/self/exe"),
    ) else {
        return false;
    };
    theirs.dev() == ours.dev() && theirs.ino() == ours.ino()
}

/// Whether another op-reason feeds our stdin somewhere up the pipe chain.
pub fn op_upstream() -> bool {
    if !own_fd_is_pipe(0) {
        return false;
    }
    let me = std::process::id();
    upstream_of(me)
        .into_iter()
        .any(|p| p != me && is_op_reason(p))
}

/// Whether `holder` (an op-reason that holds the lock) reads our output somewhere up its pipe chain.
pub fn feeds(holder: u32) -> bool {
    let me = std::process::id();
    if holder == me || !own_fd_is_pipe(1) || !is_op_reason(holder) {
        return false;
    }
    upstream_of(holder).contains(&me)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_proc(entries: &[(u32, &str, u32)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "op-reason-proc-{}",
            op_reason_core::log::random_u64()
        ));
        for (pid, comm, ppid) in entries {
            let d = root.join(pid.to_string());
            fs::create_dir_all(&d).unwrap();
            fs::write(
                d.join("stat"),
                format!("{pid} ({comm}) S {ppid} {pid} {pid}\n"),
            )
            .unwrap();
            fs::write(d.join("comm"), format!("{comm}\n")).unwrap();
        }
        root
    }

    #[test]
    fn finds_the_claude_ancestor() {
        let root = temp_proc(&[(1000, "bash", 999), (999, "claude", 500), (500, "fish", 1)]);
        assert_eq!(find_claude(&root, 1000), Some(999));
        fs::remove_dir_all(&root).unwrap();
        let root = temp_proc(&[(1000, "bash", 999), (999, "a) b", 500), (500, "fish", 1)]);
        assert_eq!(find_claude(&root, 1000), None);
        fs::remove_dir_all(&root).unwrap();
    }
}

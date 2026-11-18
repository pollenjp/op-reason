//! What `/proc` tells: the Claude Code ancestor, other op-reason processes connected through pipes, and the chain of
//! ancestors that `op-reason-ssh` finds the root of its call in.

use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;

use op_reason_core::ssh::roots::Node;

/// One process of a chain: its PID, its start time (clock ticks after boot) and its name and arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub pid: u32,
    pub start: u64,
    pub node: Node,
}

/// `start` and its ancestors, nearest first, read from `<root>/<pid>/stat` and `cmdline` (at most 64). A parent that
/// started after its child ends the chain: that PID was reused. Empty when `start` cannot be read.
pub fn chain(root: &Path, start: u32) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    let mut pid = start;
    for _ in 0..64 {
        let Some((comm, ppid, started)) = stat(root, pid) else {
            break;
        };
        if out.last().is_some_and(|child| started > child.start) {
            break;
        }
        out.push(Link {
            pid,
            start: started,
            node: Node {
                name: comm,
                args: cmdline(root, pid),
            },
        });
        if ppid == 0 || ppid == pid {
            break;
        }
        pid = ppid;
    }
    out
}

/// The `comm`, the parent and the start time of `<root>/<pid>/stat` (fields 2, 4 and 22).
fn stat(root: &Path, pid: u32) -> Option<(String, u32, u64)> {
    let text = fs::read_to_string(root.join(pid.to_string()).join("stat")).ok()?;
    // comm is in parentheses and may contain spaces and parentheses: it ends at the last ") "
    let open = text.find('(')?;
    let close = text.rfind(") ")?;
    let comm = text.get(open + 1..close)?.to_owned();
    let mut fields = text.get(close + 2..)?.split_whitespace();
    let ppid = fields.nth(1)?.parse().ok()?;
    let started = fields.nth(17)?.parse().ok()?;
    Some((comm, ppid, started))
}

/// The arguments of `<root>/<pid>/cmdline` (empty when it cannot be read).
fn cmdline(root: &Path, pid: u32) -> Vec<String> {
    let Ok(bytes) = fs::read(root.join(pid.to_string()).join("cmdline")) else {
        return Vec::new();
    };
    let mut args: Vec<String> = bytes
        .split(|b| *b == 0)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    if args.last().is_some_and(String::is_empty) {
        args.pop();
    }
    args
}

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

    /// A fake `/proc` with start times and command lines: `(pid, comm, ppid, start, argv)`.
    fn temp_proc_full(entries: &[(u32, &str, u32, u64, &[&str])]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "op-reason-proc-chain-{}",
            op_reason_core::log::random_u64()
        ));
        for (pid, comm, ppid, start, argv) in entries {
            let d = root.join(pid.to_string());
            fs::create_dir_all(&d).unwrap();
            fs::write(
                d.join("stat"),
                format!("{pid} ({comm}) S {ppid} 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0 {start} 0 0\n"),
            )
            .unwrap();
            let mut cmdline = argv.join("\0");
            cmdline.push('\0');
            fs::write(d.join("cmdline"), cmdline).unwrap();
        }
        root
    }

    #[test]
    fn the_chain_with_start_times_and_arguments() {
        let root = temp_proc_full(&[
            (30, "op-reason-ssh", 20, 300, &["op-reason-ssh", "host"]),
            (20, "a) b", 10, 200, &["weird", "x y"]),
            (10, "fish", 1, 100, &["-fish"]),
            (1, "init", 0, 1, &["/init"]),
        ]);
        let links = chain(&root, 30);
        let got: Vec<(u32, u64, &str)> = links
            .iter()
            .map(|l| (l.pid, l.start, l.node.name.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (30, 300, "op-reason-ssh"),
                (20, 200, "a) b"),
                (10, 100, "fish"),
                (1, 1, "init")
            ]
        );
        assert_eq!(
            links[1].node.args,
            vec!["weird".to_owned(), "x y".to_owned()]
        );
        assert_eq!(links[2].node.args, vec!["-fish".to_owned()]);
        assert!(chain(&root, 99).is_empty(), "no such process");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_parent_that_started_after_its_child_ends_the_chain() {
        let root = temp_proc_full(&[
            (30, "op-reason-ssh", 20, 300, &["op-reason-ssh"]),
            (20, "git", 10, 200, &["git", "push"]),
            (10, "bash", 1, 250, &["bash"]),
        ]);
        let pids: Vec<u32> = chain(&root, 30).iter().map(|l| l.pid).collect();
        assert_eq!(pids, vec![30, 20], "10 is a reused PID");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unreadable_arguments_leave_them_empty() {
        let root = temp_proc_full(&[(30, "x", 1, 3, &["x"])]);
        fs::remove_file(root.join("30").join("cmdline")).unwrap();
        let links = chain(&root, 30);
        assert_eq!(links.len(), 1);
        assert!(links[0].node.args.is_empty());
        fs::remove_dir_all(&root).unwrap();
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

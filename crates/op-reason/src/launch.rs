//! Starting other programs: lookups in `PATH`, timeouts, `wslpath`, `WSLENV`.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Finds a command: a name with `/` must be an executable file; other names are looked up in `PATH`.
pub fn which(cmd: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let is_exec = |p: &Path| {
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if cmd.contains('/') {
        let p = PathBuf::from(cmd);
        return is_exec(&p).then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(cmd))
        .find(|p| is_exec(p))
}

/// Waits for a child up to `limit`; kills it when the time runs out.
pub fn wait_timeout(child: &mut Child, limit: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {}
            Err(_) => return None,
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Runs a command with stdin, stdout and stderr discarded, up to `limit`. `None` when it could not start or timed out.
pub fn status_timeout(cmd: &mut Command, limit: Duration) -> Option<ExitStatus> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    wait_timeout(&mut child, limit)
}

/// Runs a command and captures stdout (stdin and stderr discarded), up to `limit`.
pub fn output_timeout(cmd: &mut Command, limit: Duration) -> Option<(ExitStatus, Vec<u8>)> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let status = wait_timeout(&mut child, limit);
    let buf = reader.join().unwrap_or_default();
    status.map(|s| (s, buf))
}

/// `wslpath -w <path>`: the Windows form of a Linux path (empty when wslpath fails).
pub fn wslpath_w(path: &Path) -> String {
    output_timeout(
        Command::new("wslpath").arg("-w").arg(path),
        Duration::from_secs(10),
    )
    .filter(|(s, _)| s.success())
    .map(|(_, out)| {
        String::from_utf8_lossy(&out)
            .trim_end_matches(['\n', '\r'])
            .to_string()
    })
    .unwrap_or_default()
}

/// The caller's `WSLENV` with items added (a trailing `:` of the caller's value is dropped).
pub fn wslenv_plus(items: &[&str]) -> String {
    let current = std::env::var("WSLENV").unwrap_or_default();
    let mut value = current.strip_suffix(':').unwrap_or(&current).to_string();
    for item in items {
        if !value.is_empty() {
            value.push(':');
        }
        value.push_str(item);
    }
    value
}

/// The caller's `WSLENV` without empty items and without items that start with `OP_` (1Password settings), for the
/// watchdog's direct `op.exe` calls.
pub fn wslenv_without_op() -> String {
    std::env::var("WSLENV")
        .unwrap_or_default()
        .split(':')
        .filter(|i| !i.is_empty() && !i.starts_with("OP_"))
        .collect::<Vec<_>>()
        .join(":")
}

/// The exit code of a finished child as a shell reports it (128 + signal when killed by a signal).
pub fn exit_code(status: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// Writes text to stdout, ignoring a closed pipe (`op-reason --skill | head -1`).
pub fn print_stdout(text: impl AsRef<OsStr>) {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_ref().as_bytes());
    let _ = out.flush();
}

/// Writes text to stderr as it is.
pub fn print_stderr(text: &str) {
    use std::io::Write;
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(text.as_bytes());
    let _ = err.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_out() {
        let started = Instant::now();
        assert!(
            status_timeout(Command::new("sleep").arg("5"), Duration::from_millis(200)).is_none()
        );
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(
            status_timeout(&mut Command::new("true"), Duration::from_secs(5))
                .is_some_and(|s| s.success())
        );
    }
}

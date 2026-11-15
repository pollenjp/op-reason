//! Finding and starting op.exe. stdin and stdout are the caller's handles, passed down untouched; stderr goes
//! through a pipe that a thread copies to our stderr while keeping a bounded copy (to tell `denied` and `timeout`).

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use op_reason_core::args::Kind;

/// Overrides the op.exe path (tests, unusual installs).
pub const OP_EXE_VAR: &str = "OPREASON_OP_EXE";

/// Where op.exe is, or what was tried.
pub enum Found {
    At(PathBuf),
    Missing(Option<String>),
}

/// `OPREASON_OP_EXE`, else the first `op.exe` in an absolute PATH entry. Relative entries are skipped (Windows'
/// own search would run an op.exe planted in the current directory), and so is this exe if it was copied as
/// `op.exe`.
pub fn find() -> Found {
    if let Some(p) = std::env::var_os(OP_EXE_VAR).filter(|v| !v.is_empty()) {
        let p = PathBuf::from(p);
        return if p.is_file() {
            Found::At(p)
        } else {
            Found::Missing(Some(p.display().to_string()))
        };
    }
    let me = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        if !dir.is_absolute() {
            continue;
        }
        let cand = dir.join("op.exe");
        if !cand.is_file() {
            continue;
        }
        if me.is_some() && cand.canonicalize().ok() == me {
            continue;
        }
        return Found::At(cand);
    }
    Found::Missing(None)
}

/// The image name used to count other op.exe processes.
pub fn image_name(op: &Path) -> String {
    op.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "op.exe".to_owned())
}

/// Sets the environment per kind, as the WSL binary does.
fn apply_env(cmd: &mut Command, kind: Kind) {
    match kind {
        Kind::Desktop | Kind::Help => {
            cmd.env_remove("OP_SERVICE_ACCOUNT_TOKEN")
                .env_remove("OP_CONNECT_HOST")
                .env_remove("OP_CONNECT_TOKEN");
        }
        Kind::ServiceAccount => {
            cmd.env("OP_BIOMETRIC_UNLOCK_ENABLED", "false");
        }
        Kind::Connect => {
            cmd.env_remove("OP_SERVICE_ACCOUNT_TOKEN")
                .env("OP_BIOMETRIC_UNLOCK_ENABLED", "false");
        }
    }
    cmd.env_remove(OP_EXE_VAR);
}

/// A running op.exe and the thread that copies its stderr.
pub struct Running {
    pub child: Child,
    tee: mpsc::Receiver<Vec<u8>>,
}

/// The longest stderr copy kept for classifying the result (the end of the stream).
const KEEP: usize = 64 * 1024;

/// Starts op.exe with the caller's stdin and stdout.
pub fn start(op: &Path, args: &[OsString], kind: Kind) -> std::io::Result<Running> {
    let mut cmd = Command::new(op);
    cmd.args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped());
    apply_env(&mut cmd, kind);
    let mut child = cmd.spawn()?;
    let mut err = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("no stderr pipe"))?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut copy: Vec<u8> = Vec::new();
        let mut buf = [0u8; 4096];
        let stderr = std::io::stderr();
        loop {
            match err.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    relay(&mut stderr.lock(), chunk);
                    copy.extend_from_slice(chunk);
                    if copy.len() > KEEP {
                        let cut = copy.len() - KEEP;
                        copy.drain(..cut);
                    }
                }
            }
        }
        let _ = tx.send(copy);
    });
    Ok(Running { child, tee: rx })
}

impl Running {
    /// The stderr copy, once op.exe has exited. A grandchild that still holds the pipe (an `op run` command left
    /// in the background) must not hang the call, so this waits at most two seconds.
    pub fn stderr_copy(&self) -> Vec<u8> {
        self.tee
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_default()
    }
}

/// Writes `chunk` to our stderr. A console takes only valid UTF-8: its `write` writes the valid head and refuses
/// the rest on the next call, so only the part not yet written is passed on lossily (`write_all` and a lossy retry
/// of the whole chunk would print the head twice).
fn relay(out: &mut impl Write, chunk: &[u8]) {
    let mut rest = chunk;
    while !rest.is_empty() {
        match out.write(rest) {
            Ok(0) => break,
            Ok(n) => rest = &rest[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => {
                let _ = out.write_all(String::from_utf8_lossy(rest).as_bytes());
                break;
            }
        }
    }
    let _ = out.flush();
}

/// `whoami` and `signout` get this long; a stuck one is killed (the mutex must not stay held forever).
const QUIET_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs op.exe with no window and all streams redirected (`whoami`, `signout`): nothing reaches the caller.
fn quiet(op: &Path, args: &[&str]) -> std::io::Result<(i32, String)> {
    let mut cmd = Command::new(op);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW.0);
    apply_env(&mut cmd, Kind::Desktop);
    let mut child = cmd.spawn()?;
    let err = child.stderr.take();
    let reader = thread::spawn(move || {
        let mut text = Vec::new();
        if let Some(mut e) = err {
            let _ = e.read_to_end(&mut text);
        }
        text
    });
    let deadline = Instant::now() + QUIET_TIMEOUT;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "op.exe {} did not finish within {} seconds",
                    args.join(" "),
                    QUIET_TIMEOUT.as_secs()
                ),
            ));
        }
        thread::sleep(Duration::from_millis(20));
    };
    let text = reader.join().unwrap_or_default();
    Ok((
        status.code().unwrap_or(1),
        String::from_utf8_lossy(&text).into_owned(),
    ))
}

/// Squeezes whitespace and cuts to `n` characters with `...`, like the launcher's `Get-Short`.
pub fn short(s: &str, n: usize) -> String {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > n {
        format!("{}...", t.chars().take(n).collect::<String>())
    } else {
        t
    }
}

/// Closes this process's CLI sessions: `whoami` first (it does not prompt without a session), then
/// `signout --all` only when signed in. Returns the `signout` field of the record.
pub fn close_session(op: &Path) -> String {
    match quiet(op, &["whoami"]) {
        Err(e) => format!("failed: {}", short(&e.to_string(), 120)),
        Ok((code, _)) if code != 0 => "not-signed-in".to_owned(),
        Ok(_) => match quiet(op, &["signout", "--all"]) {
            Ok((0, _)) => "ok".to_owned(),
            Ok((code, err)) => format!("failed: exit {code}: {}", short(&err, 120)),
            Err(e) => format!("failed: {}", short(&e.to_string(), 120)),
        },
    }
}

/// The length check needs the arguments as `&OsStr`.
pub fn as_os_strs(args: &[OsString]) -> Vec<&OsStr> {
    args.iter().map(OsString::as_os_str).collect()
}

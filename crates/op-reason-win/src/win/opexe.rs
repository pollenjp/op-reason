//! Finding and starting op.exe. stdin and stdout are the caller's handles, passed down untouched (stdin is fed from
//! bytes instead when the entry has read an `op inject` template from it); stderr goes
//! through a pipe that a thread copies to our stderr while keeping a bounded copy (to tell `denied` and `timeout`), or,
//! for a WSL call whose wrapper keeps that copy itself, is passed down too. op.exe runs in a kill-on-close Job Object
//! ([`super::job`]).

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};

use op_reason_core::args::Kind;
use op_reason_core::contract::ENV_PAYLOAD;

use super::job::{self, Job};
use crate::account::{self, Account};

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

/// op.exe's environment.
#[derive(Debug, Clone, Copy)]
pub enum Env {
    /// Per kind, as the WSL binary sets it (the entry face).
    Kind(Kind),
    /// As it came (the runner face: the WSL binary has already set it).
    AsIs,
}

/// op.exe's stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stderr {
    /// Through a pipe, relayed to ours, with a copy kept (the entry face, and the payload's `stderr: "scan"`).
    Tee,
    /// Ours, passed down (the payload's `stderr: "inherit"`: the WSL binary relays and copies it).
    Inherit,
}

/// Sets the environment. The payload never reaches op.exe.
fn apply_env(cmd: &mut Command, env: Env) {
    if let Env::Kind(kind) = env {
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
    }
    cmd.env_remove(OP_EXE_VAR).env_remove(ENV_PAYLOAD);
}

/// A running op.exe, the thread that copies its stderr (with [`Stderr::Tee`]) and its Job Object.
pub struct Running {
    pub child: Child,
    tee: Option<mpsc::Receiver<Vec<u8>>>,
    job: Option<Job>,
}

/// The longest stderr copy kept for classifying the result (the end of the stream).
const KEEP: usize = 64 * 1024;

/// Starts op.exe with the caller's stdin (or `input`, written to it from a thread: the template the entry read from
/// stdin, design `2026-11-17-refs-in-window-design.md` §2) and stdout, in a kill-on-close Job Object: created
/// suspended, put into the job, then resumed (without the job when one cannot be made or joined).
pub fn start(
    op: &Path,
    args: &[OsString],
    env: Env,
    stderr: Stderr,
    input: Option<Vec<u8>>,
) -> std::io::Result<Running> {
    let mut cmd = Command::new(op);
    cmd.args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::inherit()
        })
        .stdout(Stdio::inherit())
        .stderr(match stderr {
            Stderr::Tee => Stdio::piped(),
            Stderr::Inherit => Stdio::inherit(),
        })
        .creation_flags(CREATE_SUSPENDED.0);
    apply_env(&mut cmd, env);
    let mut child = cmd.spawn()?;
    let job = Job::new().ok().filter(|j| j.assign(&child).is_ok());
    if !job::resume(child.id()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(std::io::Error::other(
            "op.exe was created but could not be resumed",
        ));
    }
    if let (Some(bytes), Some(mut pipe)) = (input, child.stdin.take()) {
        // Dropping the pipe at the end closes op.exe's stdin, so that it sees the end of the template.
        thread::spawn(move || {
            let _ = pipe.write_all(&bytes);
        });
    }
    let Some(mut err) = child.stderr.take() else {
        return Ok(Running {
            child,
            tee: None,
            job,
        });
    };
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
    Ok(Running {
        child,
        tee: Some(rx),
        job,
    })
}

impl Running {
    /// The stderr copy, once op.exe has exited (empty with [`Stderr::Inherit`]). A grandchild that still holds the
    /// pipe (an `op run` command left in the background) must not hang the call, so this waits at most two seconds.
    pub fn stderr_copy(&self) -> Vec<u8> {
        self.tee
            .as_ref()
            .and_then(|rx| rx.recv_timeout(Duration::from_secs(2)).ok())
            .unwrap_or_default()
    }

    /// Stops op.exe and what it started (the whole job when there is one).
    pub fn kill(&mut self) {
        match &self.job {
            Some(job) => job.terminate(),
            None => {
                let _ = self.child.kill();
            }
        }
    }

    /// op.exe ended by itself: what it left running is no longer ended with this exe.
    pub fn release(&self) {
        if let Some(job) = &self.job {
            job.release();
        }
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

/// The most of `whoami`'s stdout that is kept (its JSON is a few hundred bytes).
const QUIET_OUT_MAX: u64 = 64 * 1024;

/// Runs op.exe with no window and all streams redirected (`whoami`, `signout`): nothing reaches the caller. Returns
/// the exit code, stdout (kept only with `keep_out`, at most [`QUIET_OUT_MAX`] bytes) and stderr.
fn quiet(op: &Path, args: &[&str], keep_out: bool) -> std::io::Result<(i32, String, String)> {
    let mut cmd = Command::new(op);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(if keep_out {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW.0);
    apply_env(&mut cmd, Env::Kind(Kind::Desktop));
    let mut child = cmd.spawn()?;
    let err = child.stderr.take();
    let reader = thread::spawn(move || {
        let mut text = Vec::new();
        if let Some(mut e) = err {
            let _ = e.read_to_end(&mut text);
        }
        text
    });
    let out = child.stdout.take();
    let out_reader = thread::spawn(move || {
        let mut text = Vec::new();
        if let Some(o) = out {
            let mut o = o.take(QUIET_OUT_MAX);
            let _ = o.read_to_end(&mut text);
            // Read the rest away, so that a long output cannot block op.exe.
            let _ = std::io::copy(o.get_mut(), &mut std::io::sink());
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
    let out = out_reader.join().unwrap_or_default();
    Ok((
        status.code().unwrap_or(1),
        String::from_utf8_lossy(&out).into_owned(),
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
/// `signout --all` only when signed in. Returns the `signout` field of the record, and the account whoami reported
/// (its JSON output: only the account ID and the sign-in host are read, design `2026-11-17-refs-in-window-design.md`
/// §5.2).
pub fn close_session(op: &Path) -> (String, Option<Account>) {
    match quiet(op, &["whoami", "--format", "json"], true) {
        Err(e) => (format!("failed: {}", short(&e.to_string(), 120)), None),
        Ok((code, _, _)) if code != 0 => ("not-signed-in".to_owned(), None),
        Ok((_, out, _)) => {
            let seen = account::from_whoami(&out);
            let signout = match quiet(op, &["signout", "--all"], false) {
                Ok((0, _, _)) => "ok".to_owned(),
                Ok((code, _, err)) => format!("failed: exit {code}: {}", short(&err, 120)),
                Err(e) => format!("failed: {}", short(&e.to_string(), 120)),
            };
            (signout, seen)
        }
    }
}

/// The length check needs the arguments as `&OsStr`.
pub fn as_os_strs(args: &[OsString]) -> Vec<&OsStr> {
    args.iter().map(OsString::as_os_str).collect()
}

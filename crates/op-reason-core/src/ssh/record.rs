//! The SSH relay's records (design §9): `%LOCALAPPDATA%\op-reason\ssh-log.jsonl`, one JSON object per line, next to
//! the log of `op` calls but rotated on its own (a playbook run makes hundreds of records), and never the data to sign
//! or a signature.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::log::{self, Caller};

/// The file.
pub const LOG_FILE: &str = "ssh-log.jsonl";
/// The file is rotated when it reaches this size.
pub const LOG_MAX: u64 = 4 * 1024 * 1024;
/// How many rotated files are kept (`ssh-log.1.jsonl` is the newest).
pub const LOG_KEEP: usize = 2;
/// Appends and rotations take this lock.
pub const LOCK_FILE: &str = "ssh-log.lock";

/// `ssh-log.<n>.jsonl`
fn rotated(state_dir: &Path, n: usize) -> PathBuf {
    state_dir.join(format!("ssh-log.{n}.jsonl"))
}

/// Appends one line, rotating first when the file has reached [`LOG_MAX`] (the oldest of [`LOG_KEEP`] goes). Takes
/// [`LOCK_FILE`] (waiting up to [`log::LOG_LOCK_WAIT`], then writing anyway). Errors are ignored: a record must never
/// stop a request.
pub fn append(state_dir: &Path, line: &str) {
    if log::ensure_dir(state_dir).is_err() {
        return;
    }
    let lock = log::open_lock_file(&state_dir.join(LOCK_FILE)).ok();
    let locked = lock
        .as_ref()
        .is_some_and(|f| log::lock_with_timeout(f, log::LOG_LOCK_WAIT));
    let file = state_dir.join(LOG_FILE);
    if fs::metadata(&file).is_ok_and(|m| m.len() >= LOG_MAX) {
        let _ = fs::remove_file(rotated(state_dir, LOG_KEEP));
        for n in (1..LOG_KEEP).rev() {
            let _ = fs::rename(rotated(state_dir, n), rotated(state_dir, n + 1));
        }
        let _ = fs::rename(&file, rotated(state_dir, 1));
    }
    if let Ok(mut f) = log::open_append(&file) {
        let mut data = line.as_bytes().to_vec();
        data.push(b'\n');
        let _ = f.write_all(&data);
    }
    if let (true, Some(f)) = (locked, lock) {
        let _ = f.unlock();
    }
}

/// The files that exist, oldest first (the rotated ones, then [`LOG_FILE`]).
pub fn files(state_dir: &Path) -> Vec<PathBuf> {
    (1..=LOG_KEEP)
        .rev()
        .map(|n| rotated(state_dir, n))
        .chain([state_dir.join(LOG_FILE)])
        .filter(|p| p.is_file())
        .collect()
}

/// One signature: an `ssh-sign` record.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Sign {
    pub v: u32,
    pub id: String,
    pub event: &'static str,
    pub time: String,
    pub epoch: i64,
    /// `per-call` or `show-only`.
    pub mode: String,
    /// per-call: `allowed` (a click), `passed` (an earlier answer), `denied` (a click or an earlier answer), `timeout`,
    /// `dismissed` (the window closed without an answer), `gone` (the client left first); show-only: `signed`; both:
    /// `failed` (1Password refused), `no-answer` (1Password closed without an answer), `no-upstream`, `lock-timeout`,
    /// `malformed`.
    pub result: String,
    /// From the request to its answer, to a tenth of a second.
    pub seconds: f64,
    /// The click: `allow`, `allow-terminal`, `deny`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// The record whose answer covered this request (its facts are there).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<CallRec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The reason, at most 500 characters (`""`: none given).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub client: ClientRec,
    pub key: KeyRec,
    pub purpose: PurposeRec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<HostRec>,
    pub forwarded: bool,
}

/// The call a request belongs to.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CallRec {
    /// `claude`, `shell`, `parent`, or `alone` (forwarded, or a WSL `ssh.exe` without `op-reason-ssh`).
    pub rule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<ProcRec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<ProcRec>,
    /// WSL calls: the distribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distro: Option<String>,
}

/// A process: PID, start time, name and masked command line.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ProcRec {
    pub pid: u32,
    pub start: u64,
    pub name: String,
    pub cmd: String,
}

/// The process that connected: PID, executable and masked command line (at most 1000 characters).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ClientRec {
    pub pid: u32,
    pub exe: String,
    pub cmd: String,
}

/// The key: fingerprint, comment (its name in 1Password) and type.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct KeyRec {
    pub fp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// What the signature is for: `userauth` (with `user`), `sshsig` (with `namespace`) or `other`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PurposeRec {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// The host: its name from the command line, its host key's fingerprint, and for a forwarded connection the host key
/// of the remote client's destination (`onward`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct HostRec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onward: Option<String>,
}

/// The relay itself: a `ssh-relay` record. `what`: `start`, `exit`, `no-prompt` (show-only: a signature passed too
/// fast for a prompt), `no-parent`, `parent-ended`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Relay {
    pub v: u32,
    pub id: String,
    pub event: &'static str,
    pub time: String,
    pub epoch: i64,
    pub what: String,
    pub mode: String,
    pub pipe: String,
    pub upstream: String,
    /// The time limit of an answer, in seconds.
    pub limit: u64,
    pub pid: u32,
    /// The relay's parent process, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ProcRec>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("op-reason-ssh-record-{name}-{}", log::random_u64()))
    }

    #[test]
    fn a_sign_record_keeps_its_shape() {
        let r = Sign {
            v: 1,
            id: "s1".into(),
            event: "ssh-sign",
            time: "t".into(),
            epoch: 5,
            mode: "per-call".into(),
            result: "allowed".into(),
            seconds: 2.5,
            answer: Some("allow".into()),
            call: Some(CallRec {
                rule: "claude".into(),
                root: Some(ProcRec {
                    pid: 7,
                    start: 9,
                    name: "cmd.exe".into(),
                    cmd: "cmd.exe /d /s /c x".into(),
                }),
                ..CallRec::default()
            }),
            caller: Some(Caller {
                kind: "claude".into(),
                claude_pid: Some(3),
                cwd: "~".into(),
                ..Caller::default()
            }),
            reason: Some("push".into()),
            client: ClientRec {
                pid: 11,
                exe: r"C:\ssh.exe".into(),
                cmd: "ssh.exe git@github.com".into(),
            },
            key: KeyRec {
                fp: "SHA256:x".into(),
                comment: Some("GitHub".into()),
                kind: Some("ssh-ed25519".into()),
            },
            purpose: PurposeRec {
                kind: "userauth".into(),
                user: Some("git".into()),
                namespace: None,
            },
            host: Some(HostRec {
                name: Some("git@github.com".into()),
                fp: Some("SHA256:h".into()),
                onward: None,
            }),
            ..Sign::default()
        };
        assert_eq!(
            log::line(&r),
            r#"{"v":1,"id":"s1","event":"ssh-sign","time":"t","epoch":5,"mode":"per-call","result":"allowed","seconds":2.5,"answer":"allow","call":{"rule":"claude","root":{"pid":7,"start":9,"name":"cmd.exe","cmd":"cmd.exe /d /s /c x"}},"caller":{"type":"claude","claude_pid":3,"herdr":null,"cwd":"~","repo":null,"branch":null},"reason":"push","client":{"pid":11,"exe":"C:\\ssh.exe","cmd":"ssh.exe git@github.com"},"key":{"fp":"SHA256:x","comment":"GitHub","type":"ssh-ed25519"},"purpose":{"kind":"userauth","user":"git"},"host":{"name":"git@github.com","fp":"SHA256:h"},"forwarded":false}"#
        );
        let relay = Relay {
            v: 1,
            id: "r1".into(),
            event: "ssh-relay",
            time: "t".into(),
            epoch: 6,
            what: "start".into(),
            mode: "show-only".into(),
            pipe: r"\\.\pipe\p".into(),
            upstream: r"\\.\pipe\u".into(),
            limit: 3600,
            pid: 42,
            parent: Some(ProcRec {
                pid: 41,
                start: 1,
                name: "svchost.exe".into(),
                cmd: String::new(),
            }),
        };
        assert_eq!(
            log::line(&relay),
            r#"{"v":1,"id":"r1","event":"ssh-relay","time":"t","epoch":6,"what":"start","mode":"show-only","pipe":"\\\\.\\pipe\\p","upstream":"\\\\.\\pipe\\u","limit":3600,"pid":42,"parent":{"pid":41,"start":1,"name":"svchost.exe","cmd":""}}"#
        );
    }

    #[test]
    fn rotates_and_keeps_two_old_files() {
        let dir = temp("rotate");
        assert!(files(&dir).is_empty());
        append(&dir, r#"{"a":1}"#);
        assert_eq!(
            fs::read_to_string(dir.join(LOG_FILE)).unwrap(),
            "{\"a\":1}\n"
        );
        for (n, text) in ["b", "c", "d"].iter().enumerate() {
            fs::write(dir.join(LOG_FILE), "x".repeat(LOG_MAX as usize)).unwrap();
            append(&dir, &format!(r#"{{"{text}":{n}}}"#));
        }
        assert_eq!(
            fs::read_to_string(dir.join(LOG_FILE)).unwrap(),
            "{\"d\":2}\n"
        );
        assert_eq!(
            files(&dir),
            vec![rotated(&dir, 2), rotated(&dir, 1), dir.join(LOG_FILE)]
        );
        assert!(!rotated(&dir, 3).exists(), "only two old files");
        assert!(dir.join(LOCK_FILE).exists());
        assert!(
            !dir.join("log.jsonl").exists(),
            "the log of op calls is left alone"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}

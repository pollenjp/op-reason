//! Log records (`v: 1`) and appending them with rotation. The format is the one the bash version wrote, field for
//! field: tools (such as a Claude Code hook) read it to find denials and timeouts.

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::contract::Herdr;
use crate::time::Now;

/// The log is renamed to `log.1.jsonl` when it reaches this size.
pub const LOG_MAX: u64 = 1_048_576;
/// Seconds to wait for `log.lock` before writing anyway.
pub const LOG_LOCK_WAIT: Duration = Duration::from_secs(5);

/// `caller` in `start` and `refused`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Caller {
    /// `claude` or `human`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The Claude Code process, when the caller is Claude Code.
    pub claude_pid: Option<u32>,
    /// herdr workspace and pane, when inside herdr.
    pub herdr: Option<Herdr>,
    /// The current directory.
    pub cwd: String,
    /// `owner/name`.
    pub repo: Option<String>,
    /// The branch.
    pub branch: Option<String>,
}

/// A `start` record.
#[derive(Debug, Clone, Serialize)]
pub struct Start<'a> {
    pub v: u32,
    pub id: &'a str,
    pub event: &'static str,
    pub time: &'a str,
    pub epoch: i64,
    pub session: Option<&'a str>,
    pub mode: &'a str,
    pub kind: &'a str,
    pub lock: &'a str,
    pub caller: &'a Caller,
    pub reason: &'a str,
    pub args: &'a [String],
}

/// The watchdog results in an `end` record.
#[derive(Debug, Clone, Serialize)]
pub struct Watch<'a> {
    pub start: Option<&'a str>,
    pub end: Option<&'a str>,
}

/// Why a WSL call in `auto` mode moved from `op-reason.exe` to the launcher (`end.fallback`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fallback {
    /// `not-found` (no `op-reason.exe`), `not-started` (it could not be started) or `no-status` (it exited without
    /// writing the status file).
    pub why: &'static str,
    /// The exit code of `op-reason.exe` for `no-status`, else null.
    pub exit: Option<i32>,
}

/// An `end` record.
#[derive(Debug, Clone, Serialize)]
pub struct End<'a> {
    pub v: u32,
    pub id: &'a str,
    pub event: &'static str,
    pub time: &'a str,
    pub epoch: i64,
    pub session: Option<&'a str>,
    pub result: &'a str,
    pub exit: i32,
    pub seconds: i64,
    pub window: Option<Value>,
    pub toast: Option<Value>,
    pub others: Option<Value>,
    pub signout: Option<Value>,
    pub watch: Watch<'a>,
    /// WSL calls: `exe` or `launcher`, whichever ran the call (left out when none ran, and on Windows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<&'a str>,
    /// WSL calls: only when `auto` moved from `op-reason.exe` to the launcher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<&'a Fallback>,
}

/// A `refused` record.
#[derive(Debug, Clone, Serialize)]
pub struct Refused<'a> {
    pub v: u32,
    pub id: &'a str,
    pub event: &'static str,
    pub time: &'a str,
    pub epoch: i64,
    pub session: Option<&'a str>,
    pub why: &'a str,
    pub caller: &'a Caller,
    pub reason: &'a str,
    pub args: &'a [String],
}

/// A `shared-approval` record.
#[derive(Debug, Clone, Serialize)]
pub struct Shared<'a> {
    pub v: u32,
    pub id: &'a str,
    pub event: &'static str,
    pub time: &'a str,
    pub epoch: i64,
    pub when: &'a str,
    pub cleared: bool,
}

/// Serializes a record as one line.
pub fn line<T: Serialize>(record: &T) -> String {
    serde_json::to_string(record).expect("records serialize")
}

/// A call id: `YYYYmmddTHHMMSS-<pid>-<random>`.
pub fn new_id(now: &Now) -> String {
    format!(
        "{}-{}-{}",
        now.id_stamp,
        std::process::id(),
        random_u64() % 32768
    )
}

/// A random number from the standard library's per-process random hash keys (no dependency needed).
pub fn random_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    h.finish()
}

/// Creates the state directory (mode 0700 on Unix).
pub fn ensure_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)
    }
}

/// Opens a file for appending, creating it with mode 0600 on Unix.
pub fn open_append(path: &Path) -> std::io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

/// Opens (creating, never truncating) a file used only for locking, with read and write access (mode 0600 on Unix).
/// Windows refuses `LockFileEx` on a handle opened for appending only, so a lock file must not be opened with
/// [`open_append`].
pub fn open_lock_file(path: &Path) -> std::io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

/// Takes an exclusive lock on `file`, waiting up to `wait`. Returns whether the lock was taken (false also when the
/// system refuses the lock).
pub fn lock_with_timeout(file: &File, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return true,
            Err(fs::TryLockError::WouldBlock) => {}
            Err(fs::TryLockError::Error(_)) => return false,
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Appends one line to `<state>/log.jsonl`, rotating to `log.1.jsonl` at [`LOG_MAX`]. Appending and rotating take
/// `<state>/log.lock` (waiting up to [`LOG_LOCK_WAIT`], then writing anyway). Errors are ignored: the log must never
/// stop a call.
pub fn append(state_dir: &Path, record_line: &str) {
    if ensure_dir(state_dir).is_err() {
        return;
    }
    let lock = open_lock_file(&state_dir.join("log.lock")).ok();
    let locked = lock
        .as_ref()
        .is_some_and(|f| lock_with_timeout(f, LOG_LOCK_WAIT));
    let log = state_dir.join("log.jsonl");
    if fs::metadata(&log).is_ok_and(|m| m.len() >= LOG_MAX) {
        let _ = fs::rename(&log, state_dir.join("log.1.jsonl"));
    }
    if let Ok(mut f) = open_append(&log) {
        let mut data = record_line.as_bytes().to_vec();
        data.push(b'\n');
        let _ = f.write_all(&data);
    }
    if let (true, Some(f)) = (locked, lock) {
        let _ = f.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caller() -> Caller {
        Caller {
            kind: "claude".into(),
            claude_pid: Some(999),
            herdr: Some(Herdr {
                workspace: "wX".into(),
                label: "作業 1 テスト".into(),
                pane: "wX:p1".into(),
            }),
            cwd: "/w".into(),
            repo: Some("o/r".into()),
            branch: Some("feat/x".into()),
        }
    }

    #[test]
    fn records_keep_the_bash_shape() {
        let c = caller();
        let args = vec!["item".to_string(), "get".to_string()];
        let s = line(&Start {
            v: 1,
            id: "i",
            event: "start",
            time: "t",
            epoch: 5,
            session: None,
            mode: "desktop",
            kind: "desktop",
            lock: "held",
            caller: &c,
            reason: "r",
            args: &args,
        });
        assert_eq!(
            s,
            r#"{"v":1,"id":"i","event":"start","time":"t","epoch":5,"session":null,"mode":"desktop","kind":"desktop","lock":"held","caller":{"type":"claude","claude_pid":999,"herdr":{"workspace":"wX","label":"作業 1 テスト","pane":"wX:p1"},"cwd":"/w","repo":"o/r","branch":"feat/x"},"reason":"r","args":["item","get"]}"#
        );
        let e = line(&End {
            v: 1,
            id: "i",
            event: "end",
            time: "t",
            epoch: 6,
            session: Some("s1"),
            result: "approved",
            exit: 0,
            seconds: 1,
            window: Some(Value::from("shown")),
            toast: None,
            others: Some(Value::from(0)),
            signout: Some(Value::from("ok")),
            watch: Watch {
                start: Some("none"),
                end: None,
            },
            runner: None,
            fallback: None,
        });
        assert_eq!(
            e,
            r#"{"v":1,"id":"i","event":"end","time":"t","epoch":6,"session":"s1","result":"approved","exit":0,"seconds":1,"window":"shown","toast":null,"others":0,"signout":"ok","watch":{"start":"none","end":null}}"#
        );
        let fell = Fallback {
            why: "no-status",
            exit: Some(1),
        };
        let e = line(&End {
            v: 1,
            id: "i",
            event: "end",
            time: "t",
            epoch: 6,
            session: None,
            result: "approved",
            exit: 0,
            seconds: 1,
            window: None,
            toast: None,
            others: None,
            signout: None,
            watch: Watch {
                start: None,
                end: None,
            },
            runner: Some("launcher"),
            fallback: Some(&fell),
        });
        assert!(
            e.ends_with(r#""watch":{"start":null,"end":null},"runner":"launcher","fallback":{"why":"no-status","exit":1}}"#),
            "{e}"
        );
        let r = line(&Refused {
            v: 1,
            id: "i",
            event: "refused",
            time: "t",
            epoch: 7,
            session: None,
            why: "no-reason",
            caller: &Caller {
                kind: "human".into(),
                cwd: "/w".into(),
                ..Default::default()
            },
            reason: "",
            args: &args,
        });
        assert_eq!(
            r,
            r#"{"v":1,"id":"i","event":"refused","time":"t","epoch":7,"session":null,"why":"no-reason","caller":{"type":"human","claude_pid":null,"herdr":null,"cwd":"/w","repo":null,"branch":null},"reason":"","args":["item","get"]}"#
        );
        let sh = line(&Shared {
            v: 1,
            id: "i",
            event: "shared-approval",
            time: "t",
            epoch: 8,
            when: "start",
            cleared: true,
        });
        assert_eq!(
            sh,
            r#"{"v":1,"id":"i","event":"shared-approval","time":"t","epoch":8,"when":"start","cleared":true}"#
        );
    }

    #[test]
    fn appends_and_rotates() {
        let dir = std::env::temp_dir().join(format!("op-reason-log-test-{}", random_u64()));
        append(&dir, r#"{"a":1}"#);
        assert_eq!(
            fs::read_to_string(dir.join("log.jsonl")).unwrap(),
            "{\"a\":1}\n"
        );
        let big = "x".repeat(LOG_MAX as usize);
        fs::write(dir.join("log.jsonl"), big).unwrap();
        append(&dir, r#"{"b":2}"#);
        assert_eq!(
            fs::read_to_string(dir.join("log.jsonl")).unwrap(),
            "{\"b\":2}\n"
        );
        assert_eq!(
            fs::metadata(dir.join("log.1.jsonl")).unwrap().len(),
            LOG_MAX
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn append_waits_for_the_log_lock() {
        let dir = std::env::temp_dir().join(format!("op-reason-log-lock-test-{}", random_u64()));
        ensure_dir(&dir).unwrap();
        let holder = open_lock_file(&dir.join("log.lock")).unwrap();
        assert!(
            lock_with_timeout(&holder, Duration::from_secs(1)),
            "the lock file can be locked"
        );
        let writer = {
            let dir = dir.clone();
            thread::spawn(move || append(&dir, r#"{"late":1}"#))
        };
        thread::sleep(Duration::from_millis(400));
        assert!(
            !dir.join("log.jsonl").exists(),
            "append must wait while another handle holds log.lock"
        );
        holder.unlock().unwrap();
        writer.join().unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("log.jsonl")).unwrap(),
            "{\"late\":1}\n"
        );
        drop(holder);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ids_have_three_parts() {
        let id = new_id(&crate::time::at(0));
        assert_eq!(id.split('-').count(), 3);
    }
}

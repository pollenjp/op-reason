//! The contract between an entry (the WSL binary, the PowerShell entry, or an executable that replaces it) and the
//! launcher: the payload passed in `OPREASON_PAYLOAD` and the status the launcher writes back. Section 5 of the design
//! document is the reference; keep the two in step.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Environment variable that carries the payload JSON (ASCII only).
pub const ENV_PAYLOAD: &str = "OPREASON_PAYLOAD";
/// Environment variable that carries the launcher source (base64 of UTF-8), read by the bootstrap.
pub const ENV_SCRIPT: &str = "OPREASON_SCRIPT";
/// Windows named mutex that makes calls from WSL and Windows run one at a time (taken by the launcher).
pub const MUTEX_NAME: &str = r"Local\op-reason";
/// Version of this contract (`v` in the payload).
pub const CONTRACT_VERSION: u32 = 1;
/// The payload must stay below this many characters (a Windows environment variable holds 32767 UTF-16 units).
pub const PAYLOAD_MAX: usize = 32_000;
/// `OPREASON_SCRIPT` must stay below this many characters.
pub const SCRIPT_B64_MAX: usize = 30_000;
/// Default seconds to wait for the global lock.
pub const LOCK_WAIT_DEFAULT: u64 = 90;

/// Exit code: usage error, no reason, arguments too long.
pub const EXIT_USAGE: i32 = 2;
/// Exit code: the lock wait timed out.
pub const EXIT_LOCK_TIMEOUT: i32 = 75;
/// Exit code: `op.exe`, `powershell.exe` or `wslpath` not found.
pub const EXIT_MISSING: i32 = 127;

/// What the entry passes to the launcher.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Payload {
    /// Contract version, [`CONTRACT_VERSION`].
    pub v: u32,
    /// `desktop`, `help`, `service` or `notify`.
    pub mode: String,
    /// Windows path of `op.exe` (not used by `notify`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    /// Arguments for `op.exe`, each UTF-8 then base64.
    #[serde(default)]
    pub args64: Vec<String>,
    /// Where the launcher writes the status JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// `en` or `ja`.
    pub lang: String,
    /// `held` (take the global lock) or `skipped` (do not).
    pub lock: String,
    /// Seconds to wait for the global lock.
    #[serde(rename = "lockWait")]
    pub lock_wait: u64,
    /// `inherit` or `scan`.
    pub stderr: String,
    /// What the window shows (desktop mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facts: Option<Facts>,
    /// What the toast tells (notify mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify: Option<Notify>,
}

/// Facts the launcher builds the window and toast texts from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Facts {
    /// Local start time, `YYYY-MM-DD HH:MM:SS`.
    pub time: String,
    /// Who calls.
    pub caller: FactsCaller,
    /// herdr workspace and pane, when inside herdr.
    pub herdr: Option<Herdr>,
    /// Where the call runs.
    pub place: Place,
    /// The normalized reason, up to 200 characters.
    pub reason: String,
    /// The masked command, up to 300 characters.
    pub command: String,
    /// Extra lines for the window.
    #[serde(default)]
    pub notes: Vec<Note>,
}

/// The caller as the window shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FactsCaller {
    /// `claude` or `human`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Process id of Claude Code, when known.
    pub pid: Option<u32>,
    /// `CLAUDE_CODE_SESSION_ID`, when set.
    pub session: Option<String>,
    /// Free text (a process chain, a terminal), when the entry has one.
    pub detail: Option<String>,
}

/// herdr workspace and pane (also the shape of `caller.herdr` in the log).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Herdr {
    /// Workspace id.
    pub workspace: String,
    /// Workspace label (may be empty).
    pub label: String,
    /// Pane id.
    pub pane: String,
}

/// Where a call runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// `owner/name`, or the directory name without an origin.
    pub repo: Option<String>,
    /// The branch, or `(detached <hash>)`.
    pub branch: Option<String>,
    /// The current directory, `~` for the home directory.
    pub cwd: String,
}

/// An extra line in the window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// `shared-cleared` or `lock-skipped`.
    pub kind: String,
    /// `lock-skipped`: the reason of the call that holds the lock (up to 60 characters).
    #[serde(
        rename = "holderReason",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub holder_reason: Option<String>,
}

/// What the notify toast tells.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Notify {
    /// `shared-approval`.
    pub kind: String,
    /// `start` or `end`.
    pub when: String,
    /// Whether the shared approval was cleared.
    pub cleared: bool,
    /// `HH:MM:SS`.
    pub time: String,
}

/// What the launcher writes back. Unknown fields are ignored; missing fields are `None`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Status {
    /// Whether `op.exe` was started.
    #[serde(default)]
    pub started: bool,
    /// `op.exe`'s exit code.
    #[serde(default)]
    pub exit: Option<i64>,
    /// Empty, a short message, or `lock-timeout`.
    #[serde(default)]
    pub error: Option<String>,
    /// `shown`, `skipped`, `failed: …` (copied to the log as is).
    #[serde(default)]
    pub window: Option<Value>,
    /// `shown`, `skipped`, `failed: …` (copied to the log as is).
    #[serde(default)]
    pub toast: Option<Value>,
    /// Other `op.exe` processes seen (copied to the log as is).
    #[serde(default)]
    pub others: Option<Value>,
    /// `ok`, `not-signed-in`, `skipped`, `failed: …` (copied to the log as is).
    #[serde(default)]
    pub signout: Option<Value>,
    /// `held`, `skipped`, `timeout`.
    #[serde(default)]
    pub lock: Option<String>,
    /// Who holds the mutex, on `lock-timeout`.
    #[serde(default)]
    pub holder: Option<String>,
    /// `stderr: "scan"` only: `denied`, `timeout` or null.
    #[serde(default, rename = "stderrMatch")]
    pub stderr_match: Option<String>,
}

impl Status {
    /// Parses a status file's content; anything that is not a JSON object gives `None`.
    pub fn parse(text: &str) -> Option<Status> {
        let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        if !value.is_object() {
            return None;
        }
        serde_json::from_value(value).ok()
    }
}

/// Serializes a payload as ASCII-only JSON.
pub fn payload_json(payload: &Payload) -> String {
    crate::json::to_ascii(&serde_json::to_string(payload).expect("payload serializes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_parses_objects_only() {
        let st = Status::parse(
            r#"{"started":true,"exit":3,"error":"","window":"shown","toast":"skipped","others":0,"signout":"ok","extra":1}"#,
        )
        .unwrap();
        assert!(st.started);
        assert_eq!(st.exit, Some(3));
        assert_eq!(st.window, Some(Value::from("shown")));
        assert_eq!(st.others, Some(Value::from(0)));
        assert!(Status::parse("[1,2]").is_none());
        assert!(Status::parse("junk").is_none());
        assert!(Status::parse("").is_none());
        assert!(Status::parse("{\"started\":\"yes\"}").is_none());
    }

    #[test]
    fn payload_is_ascii_and_round_trips() {
        let p = Payload {
            v: CONTRACT_VERSION,
            mode: "desktop".into(),
            op: Some(r"C:\op.exe".into()),
            args64: vec!["eA==".into()],
            status: None,
            lang: "ja".into(),
            lock: "held".into(),
            lock_wait: 90,
            stderr: "inherit".into(),
            facts: Some(Facts {
                time: "2026-11-15 12:00:00".into(),
                caller: FactsCaller {
                    kind: "human".into(),
                    ..Default::default()
                },
                herdr: None,
                place: Place {
                    repo: None,
                    branch: None,
                    cwd: "~".into(),
                },
                reason: "設定を読む".into(),
                command: "op item get x".into(),
                notes: vec![Note {
                    kind: "shared-cleared".into(),
                    holder_reason: None,
                }],
            }),
            notify: None,
        };
        let json = payload_json(&p);
        assert!(json.is_ascii());
        assert!(json.contains(r#""lockWait":90"#));
        assert!(json.contains(r#""type":"human""#));
        assert!(!json.contains("holderReason"));
        let back: Payload = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}

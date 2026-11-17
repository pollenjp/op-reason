//! The contract between an entry (the WSL binary or the PowerShell entry) and the runner it starts for one call
//! (`launcher.ps1`, or `op-reason.exe --run`): the payload passed in `OPREASON_PAYLOAD` and the status the runner writes
//! back. Section 5 of the design document is the reference; keep the two in step.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::lang::Lang;

/// Environment variable that carries the payload JSON (ASCII only).
pub const ENV_PAYLOAD: &str = "OPREASON_PAYLOAD";
/// Windows named mutex that makes calls from WSL and Windows run one at a time (taken by the runner, or by
/// `op-reason.exe` for the whole call when it is the entry).
pub const MUTEX_NAME: &str = r"Local\op-reason";
/// The only argument of `op-reason.exe` when it runs a call for the WSL binary (the payload carries the rest).
pub const RUN_ARG: &str = "--run";
/// Version of this contract (`v` in the payload).
pub const CONTRACT_VERSION: u32 = 1;
/// The payload must stay below this many characters (a Windows environment variable holds 32767 UTF-16 units).
pub const PAYLOAD_MAX: usize = 32_000;
/// Default seconds to wait for the global lock.
pub const LOCK_WAIT_DEFAULT: u64 = 90;

/// Exit code: usage error, no reason, arguments too long.
pub const EXIT_USAGE: i32 = 2;
/// Exit code: the lock wait timed out.
pub const EXIT_LOCK_TIMEOUT: i32 = 75;
/// Exit code: `op.exe`, `op-reason.exe`, `launcher.ps1`, `powershell.exe` or `wslpath` not found, or the runner could
/// not be started.
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
    /// `en` or `ja` (missing: `en`): the language the entry decided. The runner shows the language chosen in the window
    /// (`settings.json`) instead when there is one.
    #[serde(default = "default_lang")]
    pub lang: String,
    /// `held` (take the global lock) or `skipped` (do not). Missing: `held`.
    #[serde(default = "default_lock")]
    pub lock: String,
    /// Seconds to wait for the global lock. Missing: 90.
    #[serde(rename = "lockWait", default = "default_lock_wait")]
    pub lock_wait: u64,
    /// `inherit` or `scan`. Missing: `inherit`.
    #[serde(default = "default_stderr")]
    pub stderr: String,
    /// What the window shows (desktop mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facts: Option<Facts>,
    /// What the toast tells (notify mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify: Option<Notify>,
}

fn default_lang() -> String {
    "en".to_string()
}
fn default_lock() -> String {
    "held".to_string()
}
fn default_lock_wait() -> u64 {
    LOCK_WAIT_DEFAULT
}
fn default_stderr() -> String {
    "inherit".to_string()
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
    /// The `op://` references the call reads, when there are any (design `2026-11-17-refs-in-window-design.md` §3.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refs: Option<Refs>,
}

/// The `op://` references of a call: `facts.refs` (the first 8) and the log's `start.refs` (all, up to 1000).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Refs {
    /// How many distinct references the call reads.
    pub total: usize,
    /// The sources that gave at least one.
    #[serde(default)]
    pub from: Vec<RefSource>,
    /// The references, in the order first seen.
    #[serde(default)]
    pub list: Vec<String>,
    /// The call's last `--account` value (the payload only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// Where references came from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RefSource {
    /// `args`, `file` (the template of `op inject`), `stdin` (its template on stdin) or `env-file` (of `op run`).
    pub kind: String,
    /// `file` and `env-file`: the file's last path component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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

/// What the runner writes back. Unknown fields are ignored; missing fields are `None`. `op-reason.exe --run` writes it
/// twice: `{"started":false,"error":"","runner":"exe"}` as soon as it has read the payload, then the whole status.
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
    /// Who holds the mutex, on `lock-timeout`: one line of text (an object is accepted too).
    #[serde(default)]
    pub holder: Option<Value>,
    /// `stderr: "scan"` only: `denied`, `timeout` or null.
    #[serde(default, rename = "stderrMatch")]
    pub stderr_match: Option<String>,
    /// `exe` when `op-reason.exe --run` wrote it (the launcher does not write it).
    #[serde(default)]
    pub runner: Option<String>,
    /// The language chosen in the window (`settings.json`) when the runner wrote the status: `Some(Some(code))`,
    /// `Some(None)` for null (nothing is chosen), `None` when the field is missing (an older runner, or one that could
    /// not tell) or is neither text nor null. Use [`Status::chosen`].
    #[serde(default, rename = "chosenLang", deserialize_with = "present")]
    pub chosen_lang: Option<Option<String>>,
}

/// `chosenLang`: null → `Some(None)`, text → `Some(Some(text))`, anything else → `None` (a malformed field must not
/// make the whole status unreadable: the status decides the result of the call).
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Null => Some(None),
        Value::String(s) => Some(Some(s)),
        _ => None,
    })
}

impl Status {
    /// What the runner said about the language chosen in the window: `Some(Some(lang))`, `Some(None)` when nothing is
    /// chosen, `None` when it did not say (or said something else than `en`, `ja` or null).
    pub fn chosen(&self) -> Option<Option<Lang>> {
        match self.chosen_lang.as_ref()? {
            None => Some(None),
            Some(code) => Lang::parse(code).map(Some),
        }
    }

    /// The holder as one line: the text, or the fields of an object joined by spaces.
    pub fn holder_text(&self) -> Option<String> {
        match self.holder.as_ref()? {
            Value::String(s) => Some(s.clone()),
            Value::Object(m) => Some(
                ["time", "caller", "reason", "command"]
                    .iter()
                    .filter_map(|k| m.get(*k).and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            Value::Null => None,
            other => Some(other.to_string()),
        }
    }

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
        let first = Status::parse(r#"{"started":false,"error":"","runner":"exe"}"#).unwrap();
        assert_eq!(
            (first.started, first.runner.as_deref()),
            (false, Some("exe"))
        );
    }

    #[test]
    fn chosen_lang_missing_null_text_or_malformed() {
        let parse = |extra: &str| {
            Status::parse(&format!(r#"{{"started":true,"exit":0{extra}}}"#)).expect("a status")
        };
        assert_eq!(parse("").chosen(), None, "an older runner says nothing");
        assert_eq!(parse(r#","chosenLang":null"#).chosen(), Some(None));
        assert_eq!(
            parse(r#","chosenLang":"ja""#).chosen(),
            Some(Some(Lang::Ja))
        );
        assert_eq!(
            parse(r#","chosenLang":"en""#).chosen(),
            Some(Some(Lang::En))
        );
        assert_eq!(
            parse(r#","chosenLang":"fr""#).chosen(),
            None,
            "a language this version does not know"
        );
        let odd = parse(r#","chosenLang":3"#);
        assert!(
            odd.started,
            "a malformed chosenLang keeps the status readable"
        );
        assert_eq!(odd.chosen(), None);
        assert_eq!(parse(r#","chosenLang":{"x":1}"#).chosen(), None);
    }

    #[test]
    fn payload_fields_have_defaults() {
        let p: Payload = serde_json::from_str(r#"{"v":1,"mode":"notify"}"#).unwrap();
        assert_eq!(
            (
                p.lang.as_str(),
                p.lock.as_str(),
                p.lock_wait,
                p.stderr.as_str()
            ),
            ("en", "held", 90, "inherit")
        );
        let st = Status::parse(
            r#"{"started":false,"error":"lock-timeout","holder":{"time":"12:00:00","caller":"Human","reason":"r","command":"op x"}}"#,
        )
        .unwrap();
        assert_eq!(st.holder_text().as_deref(), Some("12:00:00 Human r op x"));
        let st = Status::parse(r#"{"started":false,"holder":"one line"}"#).unwrap();
        assert_eq!(st.holder_text().as_deref(), Some("one line"));
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
                refs: Some(Refs {
                    total: 2,
                    from: vec![
                        RefSource {
                            kind: "args".into(),
                            name: None,
                        },
                        RefSource {
                            kind: "file".into(),
                            name: Some("設定.tpl".into()),
                        },
                    ],
                    list: vec!["op://v/i/f".into(), "op://金庫/項目/欄".into()],
                    account: Some("my".into()),
                }),
            }),
            notify: None,
        };
        let json = payload_json(&p);
        assert!(json.is_ascii());
        assert!(json.contains(r#""lockWait":90"#));
        assert!(json.contains(r#""type":"human""#));
        assert!(!json.contains("holderReason"));
        assert!(json.contains(r#""from":[{"kind":"args"},{"kind":"file","name":"#));
        let back: Payload = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn facts_without_refs_have_no_refs_field_and_old_payloads_read() {
        let f = Facts::default();
        let json = serde_json::to_string(&f).unwrap();
        assert!(!json.contains("refs"), "{json}");
        let back: Facts = serde_json::from_str(&json).unwrap();
        assert_eq!(back.refs, None);
        let refs: Refs =
            serde_json::from_str(r#"{"total":1,"list":["op://a/b/c"],"later":true}"#).unwrap();
        assert_eq!(refs.from, Vec::new());
        assert_eq!(refs.account, None);
    }
}

//! What `op-reason.exe --run` reads and writes for a call from WSL: the payload (`OPREASON_PAYLOAD`, A' spec §5.2)
//! and the status file (§5.4). Platform-neutral, so that its tests run on Linux too.

use std::ffi::OsString;

use op_reason_core::base64;
use op_reason_core::contract::{CONTRACT_VERSION, Payload};
use serde::Serialize;
use serde_json::Value;

/// The first write, before anything else: the trace that the exe woke up. The WSL binary starts the launcher instead
/// only when no status file exists, so `op.exe` cannot run twice (design `2026-11-15-wsl-through-exe-design.md` §5.1).
pub const FIRST_STATUS: &str = r#"{"started":false,"error":"","runner":"exe"}"#;

fn json(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|e| format!("the payload is not JSON: {e}"))
}

/// The `status` path of a payload, read even when the rest of it cannot be used (`None` when it has none). `Err` when
/// the text is not a JSON object.
pub fn status_path(raw: &str) -> Result<Option<String>, String> {
    let v = json(raw)?;
    let obj = v.as_object().ok_or("the payload is not a JSON object")?;
    Ok(obj
        .get("status")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned))
}

/// The payload, checked: version 1 (the WSL binary and the Windows zip are installed separately and can drift
/// apart) and a known mode.
pub fn parse(raw: &str) -> Result<Payload, String> {
    let v = json(raw)?;
    let version = v.get("v").cloned().unwrap_or(Value::Null);
    if version.as_u64() != Some(u64::from(CONTRACT_VERSION)) {
        return Err(format!(
            "this op-reason.exe reads payload version {CONTRACT_VERSION}, the caller sent {version}: install the same release of op-reason on WSL and on Windows"
        ));
    }
    let p: Payload =
        serde_json::from_value(v).map_err(|e| format!("the payload cannot be read: {e}"))?;
    if !["desktop", "help", "service", "notify"].contains(&p.mode.as_str()) {
        return Err(format!("unknown mode: {}", p.mode));
    }
    Ok(p)
}

/// `args64` decoded: each one base64 of UTF-8 bytes; bytes that are not UTF-8 become U+FFFD, as in the launcher (an
/// argument from Linux may be any bytes, a Windows command line cannot carry them).
pub fn args(p: &Payload) -> Result<Vec<OsString>, String> {
    p.args64
        .iter()
        .enumerate()
        .map(|(i, a)| {
            base64::decode(a)
                .map(|b| OsString::from(String::from_utf8_lossy(&b).into_owned()))
                .ok_or_else(|| format!("argument {} of the payload is not base64", i + 1))
        })
        .collect()
}

/// The status written at the end: the launcher's fields in the launcher's order, plus `runner`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub started: bool,
    pub exit: Option<i32>,
    pub error: String,
    pub window: String,
    pub toast: String,
    pub others: i64,
    pub signout: String,
    pub lock: String,
    /// On `lock-timeout`: who holds the mutex, as one line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holder: Option<String>,
    /// `stderr: "scan"` after `op.exe` started: `denied`, `timeout` or null.
    #[serde(rename = "stderrMatch", skip_serializing_if = "Option::is_none")]
    pub stderr_match: Option<Option<&'static str>>,
    pub runner: &'static str,
    /// The texts of the window and the toast (the launcher's `ui`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<Value>,
}

impl Default for Status {
    /// The launcher's starting values.
    fn default() -> Self {
        Status {
            started: false,
            exit: None,
            error: String::new(),
            window: "skipped".into(),
            toast: "skipped".into(),
            others: 0,
            signout: "skipped".into(),
            lock: "skipped".into(),
            holder: None,
            stderr_match: None,
            runner: "exe",
            ui: None,
        }
    }
}

impl Status {
    /// The JSON text.
    pub fn text(&self) -> String {
        serde_json::to_string(self).expect("the status serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_reason_core::contract;

    #[test]
    fn the_first_write_reads_as_a_status_of_the_exe() {
        let st = contract::Status::parse(FIRST_STATUS).expect("parses");
        assert!(!st.started);
        assert_eq!(st.runner.as_deref(), Some("exe"));
        assert_eq!(st.error.as_deref(), Some(""));
    }

    #[test]
    fn the_status_path_is_read_even_from_a_payload_that_cannot_run() {
        let raw = r#"{"v":2,"mode":"later","status":"\\\\wsl.localhost\\d\\s.json"}"#;
        assert_eq!(
            status_path(raw),
            Ok(Some(r"\\wsl.localhost\d\s.json".to_owned()))
        );
        assert!(
            parse(raw)
                .unwrap_err()
                .contains("version 1, the caller sent 2")
        );
        assert_eq!(status_path(r#"{"v":1,"mode":"notify"}"#), Ok(None));
        assert_eq!(status_path(r#"{"status":""}"#), Ok(None));
        assert!(status_path("[1]").is_err());
        assert!(status_path("nope").is_err());
    }

    #[test]
    fn parse_checks_version_and_mode() {
        let p =
            parse(r#"{"v":1,"mode":"desktop","op":"C:\\op.exe","args64":["aXRlbQ=="]}"#).unwrap();
        assert_eq!(
            (p.mode.as_str(), p.lock.as_str(), p.lock_wait),
            ("desktop", "held", 90)
        );
        assert!(
            parse(r#"{"mode":"desktop"}"#)
                .unwrap_err()
                .contains("sent null")
        );
        assert!(parse(r#"{"v":"1","mode":"desktop"}"#).is_err());
        assert_eq!(
            parse(r#"{"v":1,"mode":"other"}"#).unwrap_err(),
            "unknown mode: other"
        );
        assert!(
            parse(r#"{"v":1,"mode":"help","args64":"x"}"#)
                .unwrap_err()
                .contains("cannot be read")
        );
    }

    #[test]
    fn arguments_are_utf8_with_replacement() {
        let mut p = parse(r#"{"v":1,"mode":"help"}"#).unwrap();
        p.args64 = vec![
            base64::encode("日本語 の 値".as_bytes()),
            base64::encode(b""),
            base64::encode(b"\xffraw"),
        ];
        assert_eq!(
            args(&p).unwrap(),
            vec![
                OsString::from("日本語 の 値"),
                OsString::new(),
                OsString::from("\u{fffd}raw")
            ]
        );
        p.args64.push("***".into());
        assert_eq!(
            args(&p).unwrap_err(),
            "argument 4 of the payload is not base64"
        );
    }

    #[test]
    fn the_status_has_the_launcher_fields_in_order() {
        assert_eq!(
            Status::default().text(),
            r#"{"started":false,"exit":null,"error":"","window":"skipped","toast":"skipped","others":0,"signout":"skipped","lock":"skipped","runner":"exe"}"#
        );
        let st = Status {
            started: true,
            exit: Some(3),
            stderr_match: Some(None),
            holder: Some("12:00:00 Human reason \"r\" op x".into()),
            ..Status::default()
        };
        let back = contract::Status::parse(&st.text()).unwrap();
        assert_eq!(
            (back.started, back.exit, back.stderr_match.as_deref()),
            (true, Some(3), None)
        );
        assert!(st.text().contains(r#""stderrMatch":null"#));
        assert_eq!(
            back.holder_text().as_deref(),
            Some("12:00:00 Human reason \"r\" op x")
        );
    }
}

//! `relay.json`: what the running relay tells about itself (design `2026-11-23-settings-design.md` §8.2).

use serde::{Deserialize, Serialize};

use crate::settings::values::RelayMode;

/// The file, in `%LOCALAPPDATA%\\op-reason`.
pub const RELAY_INFO_FILE: &str = "relay.json";

/// The relay's process, pipe and the values it runs with, written at its start and removed when it exits by itself. A
/// reader trusts it only when its process ID is the one that answers on the pipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayInfo {
    /// The version of the format, 1.
    pub v: u32,
    /// The relay's process ID.
    pub pid: u32,
    /// Its exe.
    pub exe: String,
    /// Its pipe.
    pub pipe: String,
    /// `per-call` or `show-only`.
    pub mode: String,
    /// Seconds an answer covers its call.
    pub time_limit: u64,
    /// Seconds a window waits for a click.
    pub answer_wait: u64,
    /// Its arguments after `--ssh-agent` (the options that won over `settings.json`).
    pub args: Vec<String>,
    /// The task that started it (`--task <name>`: op-reason's task passes its name); none for a relay started
    /// otherwise, or by an op-reason that did not write it.
    #[serde(default)]
    pub task: Option<String>,
}

impl RelayInfo {
    /// The file's text.
    pub fn render(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// A file's text of version 1 (a byte order mark is allowed).
    pub fn parse(text: &str) -> Option<RelayInfo> {
        let info: RelayInfo = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        (info.v == 1).then_some(info)
    }

    /// The mode, when it is one of op-reason's words.
    pub fn mode(&self) -> Option<RelayMode> {
        RelayMode::parse(&self.mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::values::RelayMode;

    #[test]
    fn round_trip() {
        let info = RelayInfo {
            v: 1,
            pid: 42,
            exe: r"C:\x\op-reason.exe".into(),
            pipe: r"\\.\pipe\op-reason-ssh-agent".into(),
            mode: "show-only".into(),
            time_limit: 3600,
            answer_wait: 60,
            args: vec!["--ssh-agent".into()],
            task: Some("op-reason-ssh-relay-S-1".into()),
        };
        let text = info.render();
        assert!(text.contains(r#""timeLimit":3600"#), "{text}");
        assert_eq!(RelayInfo::parse(&text), Some(info.clone()));
        assert_eq!(info.mode(), Some(RelayMode::ShowOnly));
        assert_eq!(RelayInfo::parse("junk"), None);
        let older = r#"{"v":1,"pid":5,"exe":"x","pipe":"p","mode":"per-call","timeLimit":60,"answerWait":60,"args":[]}"#;
        assert_eq!(
            RelayInfo::parse(older).map(|i| i.task),
            Some(None),
            "a relay started without --task"
        );
        assert_eq!(RelayInfo::parse(r#"{"v":2,"pid":1}"#), None);
    }
}

//! `OPREASON_SSH_CALL`: the facts of a call from WSL (design §7). `op-reason-ssh` collects them on Linux, puts them
//! into the environment of `ssh.exe` (through `WSLENV`), and the relay reads them from that process. ASCII JSON, at
//! most [`CALL_MAX`] characters.

use serde::{Deserialize, Serialize};

use crate::args::clip;
use crate::json;
use crate::log::Caller;

/// The facts of a call from WSL.
pub const ENV_CALL: &str = "OPREASON_SSH_CALL";
/// The reason a caller gives (read by `op-reason-ssh` on WSL, and by the relay from a Windows client).
pub const ENV_REASON: &str = "OPREASON_SSH_REASON";
/// The `ssh.exe` that `op-reason-ssh` runs (else `ssh.exe` in `PATH`).
pub const ENV_SSH_EXE: &str = "OPREASON_SSH_EXE";
/// The longest `OPREASON_SSH_CALL`, in characters (the JSON is ASCII).
pub const CALL_MAX: usize = 8_000;
/// The longest reason kept.
pub const REASON_MAX: usize = 500;
/// The longest command line kept.
pub const CMD_MAX: usize = 1_000;

/// A process on Linux: its PID, its start time (clock ticks since boot, `/proc/<pid>/stat` field 22), its `comm` and
/// its masked command line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proc {
    pub pid: u32,
    pub start: u64,
    pub name: String,
    #[serde(default)]
    pub cmd: String,
}

/// What `op-reason-ssh` found.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WslCall {
    /// 1.
    pub v: u32,
    /// `WSL_DISTRO_NAME`.
    pub distro: String,
    /// `claude`, `shell` or `parent` (the rules of [`super::roots`]).
    pub rule: String,
    /// The root of the call.
    pub root: Proc,
    /// Rule 2: the interactive shell (for the answer that allows a terminal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<Proc>,
    /// The caller, as the `op` wrapper collects it.
    pub caller: Caller,
    /// `CLAUDE_CODE_SESSION_ID`.
    #[serde(default)]
    pub session: Option<String>,
    /// The normalized reason (`OPREASON_SSH_REASON`), empty when none was given.
    #[serde(default)]
    pub reason: String,
}

/// The ASCII JSON of `call`, within [`CALL_MAX`]: the command lines, the reason and the directory are cut shorter
/// until it fits.
pub fn encode(call: &WslCall) -> String {
    // The longest command lines, reason, directory and herdr label, tried in turn until the JSON fits
    const STEPS: [(usize, usize, usize, usize); 4] = [
        (CMD_MAX, REASON_MAX, 1_000, 200),
        (300, REASON_MAX, 300, 100),
        (100, 200, 100, 50),
        (0, 100, 0, 0),
    ];
    for (cmd, reason, cwd, label) in STEPS {
        let mut c = call.clone();
        c.root.cmd = cut(&c.root.cmd, cmd);
        if let Some(shell) = &mut c.shell {
            shell.cmd = cut(&shell.cmd, cmd);
        }
        c.reason = cut(&c.reason, reason);
        c.caller.cwd = cut(&c.caller.cwd, cwd);
        if let Some(h) = &mut c.caller.herdr {
            h.label = cut(&h.label, label);
        }
        let text = to_json(&c);
        if text.len() <= CALL_MAX {
            return text;
        }
    }
    // Names longer than anyone uses: what identifies the call, and a short reason
    let c = WslCall {
        v: call.v,
        distro: cut(&call.distro, 50),
        rule: cut(&call.rule, 10),
        root: Proc {
            name: cut(&call.root.name, 50),
            cmd: String::new(),
            ..call.root.clone()
        },
        shell: call.shell.as_ref().map(|s| Proc {
            pid: s.pid,
            start: s.start,
            ..Proc::default()
        }),
        caller: Caller {
            kind: cut(&call.caller.kind, 10),
            claude_pid: call.caller.claude_pid,
            ..Caller::default()
        },
        session: None,
        reason: cut(&call.reason, 50),
    };
    to_json(&c)
}

fn to_json(call: &WslCall) -> String {
    json::to_ascii(&serde_json::to_string(call).expect("a call serializes"))
}

/// `text` cut after `n` characters (with `…`), or nothing for `n` = 0.
fn cut(text: &str, n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        clip(text, n, "…")
    }
}

/// The call in an `OPREASON_SSH_CALL` value (`None` when it is not one of version 1).
pub fn decode(text: &str) -> Option<WslCall> {
    serde_json::from_str::<WslCall>(text)
        .ok()
        .filter(|c| c.v == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::Herdr;

    fn call() -> WslCall {
        WslCall {
            v: 1,
            distro: "Ubuntu".into(),
            rule: "shell".into(),
            root: Proc {
                pid: 4242,
                start: 123_456,
                name: "ansible-playboo".into(),
                cmd: "ansible-playbook site.yml".into(),
            },
            shell: Some(Proc {
                pid: 4000,
                start: 100_000,
                name: "fish".into(),
                cmd: String::new(),
            }),
            caller: Caller {
                kind: "human".into(),
                claude_pid: None,
                herdr: Some(Herdr {
                    workspace: "w1".into(),
                    label: "デプロイ".into(),
                    pane: "w1:p2".into(),
                }),
                cwd: "~/src/infra".into(),
                repo: Some("me/infra".into()),
                branch: Some("main".into()),
            },
            session: None,
            reason: "staging の DB を入れ替える".into(),
        }
    }

    #[test]
    fn round_trips_as_ascii() {
        let text = encode(&call());
        assert!(text.is_ascii(), "{text}");
        assert!(text.len() <= CALL_MAX);
        assert_eq!(decode(&text), Some(call()));
    }

    #[test]
    fn long_values_are_cut_to_fit() {
        let mut c = call();
        c.reason = "理由".repeat(10_000);
        c.root.cmd = "コマンド ".repeat(5_000);
        c.caller.cwd = format!("/{}", "ディレクトリ/".repeat(2_000));
        let text = encode(&c);
        assert!(text.is_ascii());
        assert!(text.len() <= CALL_MAX, "{}", text.len());
        let back = decode(&text).unwrap();
        assert!(!back.reason.is_empty() && c.reason.starts_with(back.reason.trim_end_matches('…')));
        assert!(back.reason.chars().count() <= REASON_MAX + 1);
        assert_eq!(back.root.pid, 4242, "the root survives");
        assert_eq!(back.shell.map(|s| s.pid), Some(4000));
    }

    #[test]
    fn other_versions_and_garbage_are_not_calls() {
        let mut c = call();
        c.v = 2;
        assert_eq!(decode(&encode(&c)), None);
        assert_eq!(decode("not json"), None);
        assert_eq!(decode("{}"), None);
    }
}

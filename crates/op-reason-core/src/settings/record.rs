//! The `settings` record of `log.jsonl` (design `2026-11-23-settings-design.md` §15).

use serde::Serialize;

use crate::log::Caller;
use crate::settings::engine::{Change, Done, Why};
use crate::time::Now;

#[derive(Serialize)]
struct Record<'a> {
    v: u32,
    id: &'a str,
    event: &'static str,
    time: &'a str,
    epoch: i64,
    session: Option<&'a str>,
    from: &'a str,
    caller: &'a Caller,
    changes: Vec<Item<'a>>,
}

#[derive(Serialize)]
struct Item<'a> {
    name: &'a str,
    from: Option<&'a str>,
    to: &'a str,
    result: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    why: Option<String>,
}

/// The code of why a change was not done.
pub fn why_code(w: &Why) -> String {
    match w {
        Why::Locked => "locked".into(),
        Why::NotOurs(_) => "not-ours".into(),
        Why::NoTask => "no-task".into(),
        Why::NoAnswer => "no-answer".into(),
        Why::Foreign(_) => "foreign".into(),
        Why::Link(_) => "link".into(),
        Why::Failed(e) => format!("failed: {}", crate::args::clip(e, 200, "…")),
    }
}

/// The record's line: the changes tried (`done` or `failed`, with why), from the command line, the window or the icon.
pub fn line(
    id: &str,
    now: &Now,
    session: Option<&str>,
    from: &str,
    caller: &Caller,
    changes: &[Change],
) -> String {
    crate::log::line(&Record {
        v: 1,
        id,
        event: "settings",
        time: &now.iso,
        epoch: now.epoch,
        session,
        from,
        caller,
        changes: changes
            .iter()
            .map(|c| Item {
                name: &c.name,
                from: c.from.as_deref(),
                to: &c.to,
                result: match c.done {
                    Done::Failed(_) => "failed",
                    _ => "done",
                },
                why: match &c.done {
                    Done::Failed(w) => Some(why_code(w)),
                    _ => None,
                },
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::Caller;
    use crate::settings::engine::{Change, Done, Why};
    use serde_json::Value;

    #[test]
    fn a_record_names_its_changes_and_caller() {
        let caller = Caller {
            kind: "claude".into(),
            claude_pid: Some(12),
            herdr: None,
            cwd: r"C:\w".into(),
            repo: None,
            branch: None,
        };
        let changes = vec![
            Change {
                name: "relay.mode".into(),
                from: Some("per-call".into()),
                to: "show-only".into(),
                done: Done::Done,
                note: None,
            },
            Change {
                name: "relay.running".into(),
                from: Some("on".into()),
                to: "off".into(),
                done: Done::Failed(Why::Locked),
                note: None,
            },
        ];
        let now = crate::time::at(1_700_000_000);
        let v: Value =
            serde_json::from_str(&line("id1", &now, None, "cli", &caller, &changes)).unwrap();
        assert_eq!(v["event"], "settings");
        assert_eq!(v["v"], 1);
        assert_eq!(v["from"], "cli");
        assert_eq!(v["caller"]["type"], "claude");
        assert_eq!(v["changes"][0]["name"], "relay.mode");
        assert_eq!(v["changes"][0]["from"], "per-call");
        assert_eq!(v["changes"][0]["to"], "show-only");
        assert_eq!(v["changes"][0]["result"], "done");
        assert!(v["changes"][0].get("why").is_none());
        assert_eq!(v["changes"][1]["result"], "failed");
        assert_eq!(v["changes"][1]["why"], "locked");
        assert_eq!(v["epoch"], 1_700_000_000);
    }
}

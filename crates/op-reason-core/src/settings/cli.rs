//! The words of `op-reason --settings` (design `2026-11-23-settings-design.md` §7.1).

use std::io::Write;

use serde_json::{Value, json};

use crate::lang::Lang;
use crate::logview::display_width;
use crate::settings::catalog::{self, Name, Usage, Want};
use crate::settings::engine::{self, Change, Ctx, Done, Item, ShortcutState, State};
use crate::settings::skill::SkillState;
use crate::settings::text;

pub use crate::settings::text::{usage, usage_error};

/// What `op-reason --settings …` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// No more arguments: the window.
    Window,
    /// `help`: the usage of `--settings`.
    Help,
    /// `list`.
    List {
        /// `--json`.
        json: bool,
    },
    /// `get <name>`.
    Get {
        /// The setting.
        name: Name,
        /// `--json`.
        json: bool,
    },
    /// `set <name> <value>` or `set <name>=<value>…`.
    Set(Vec<(Name, Want)>),
    /// `check <name>=<value>…`.
    Check {
        /// The wanted values.
        wants: Vec<(Name, Want)>,
        /// `--json`.
        json: bool,
    },
    /// `relay start|stop|restart`.
    Relay(RelayAct),
    /// `forget-accounts`.
    ForgetAccounts,
    /// `open-records`.
    OpenRecords,
}

/// What `relay …` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayAct {
    /// `relay start`: `set relay.running on`.
    Start,
    /// `relay stop`: `set relay.running off`.
    Stop,
    /// `relay restart`.
    Restart,
}

/// The arguments after `--settings`.
pub fn parse(args: &[String]) -> Result<Command, Usage> {
    let Some((word, rest)) = args.split_first() else {
        return Ok(Command::Window);
    };
    let json = rest.iter().any(|a| a == "--json");
    let plain: Vec<&String> = rest.iter().filter(|a| *a != "--json").collect();
    let no_json = || {
        if json {
            Err(Usage::Extra("--json".into()))
        } else {
            Ok(())
        }
    };
    let none_left = |from: usize| match plain.get(from) {
        Some(a) => Err(Usage::Extra((*a).clone())),
        None => Ok(()),
    };
    match word.as_str() {
        "help" | "--help" | "-h" => {
            no_json()?;
            none_left(0)?;
            Ok(Command::Help)
        }
        "list" => {
            none_left(0)?;
            Ok(Command::List { json })
        }
        "get" => {
            let name = plain.first().ok_or_else(|| Usage::NoValue("get".into()))?;
            none_left(1)?;
            Ok(Command::Get {
                name: Name::parse(name)?,
                json,
            })
        }
        "set" => {
            no_json()?;
            let wants = match plain.as_slice() {
                [] => return Err(Usage::NoValue("set".into())),
                [name, value] if !value.contains('=') => {
                    let name = Name::parse(name)?;
                    let want = catalog::want(&name, value)?;
                    vec![(name, want)]
                }
                pairs => self::pairs(pairs)?,
            };
            Ok(Command::Set(wants))
        }
        "check" => {
            if plain.is_empty() {
                return Err(Usage::NoValue("check".into()));
            }
            Ok(Command::Check {
                wants: pairs(&plain)?,
                json,
            })
        }
        "relay" => {
            no_json()?;
            let act = match plain.first().map(|s| s.as_str()) {
                Some("start") => RelayAct::Start,
                Some("stop") => RelayAct::Stop,
                Some("restart") => RelayAct::Restart,
                Some(other) => return Err(Usage::UnknownWord(other.to_owned())),
                None => return Err(Usage::NoValue("relay".into())),
            };
            none_left(1)?;
            Ok(Command::Relay(act))
        }
        "forget-accounts" | "open-records" => {
            no_json()?;
            none_left(0)?;
            Ok(if word == "forget-accounts" {
                Command::ForgetAccounts
            } else {
                Command::OpenRecords
            })
        }
        other => Err(Usage::UnknownWord(other.to_owned())),
    }
}

/// `<name>=<value>` pairs, each split at its last `=` (folders may hold one, values never do); a name may come once.
fn pairs(args: &[&String]) -> Result<Vec<(Name, Want)>, Usage> {
    let mut out: Vec<(Name, Want)> = Vec::new();
    for a in args {
        let (name, value) = a
            .rsplit_once('=')
            .ok_or_else(|| Usage::NoValue((*a).clone()))?;
        let name = Name::parse(name)?;
        if out.iter().any(|(n, _)| *n == name) {
            return Err(Usage::Twice(name.text()));
        }
        let want = catalog::want(&name, value)?;
        out.push((name, want));
    }
    Ok(out)
}

/// Runs a command on the engine, prints what it found or did, and returns the exit code (design §7.3).
pub fn run(cmd: Command, ctx: &Ctx, lang: Lang, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match cmd {
        Command::Window | Command::Help => {
            let _ = out.write_all(usage(lang).as_bytes());
            0
        }
        Command::List { json } => {
            let items = engine::list(ctx);
            let folder = ctx.sys.records_where();
            let accounts = ctx.sys.accounts();
            if json {
                let v = json!({
                    "v": 1,
                    "settings": items.iter().map(|i| item_json(i, ctx.version)).collect::<Vec<_>>(),
                    "records": {"folder": folder, "accounts": accounts},
                });
                let _ = writeln!(out, "{v}");
            } else {
                let mut rows: Vec<[String; 4]> = vec![text::header(lang).map(str::to_owned)];
                rows.extend(items.iter().map(|i| row(lang, i, ctx.version)));
                for line in table(&rows) {
                    let _ = writeln!(out, "{line}");
                }
                for line in text::records(lang, &folder, accounts) {
                    let _ = writeln!(out, "{line}");
                }
            }
            0
        }
        Command::Get { name, json } => {
            let item = engine::get(ctx, &name);
            if json {
                let v = json!({"v": 1, "settings": [item_json(&item, ctx.version)]});
                let _ = writeln!(out, "{v}");
            } else if let Some(value) = &item.value {
                let _ = writeln!(out, "{value}");
            } else {
                let detail = text::state(lang, &item, ctx.version);
                let _ = writeln!(
                    err,
                    "{}{}",
                    PREFIX,
                    text::unreadable(lang, &item.name.text(), &detail)
                );
            }
            if item.value.is_none() { 2 } else { 0 }
        }
        Command::Set(wants) => changes(lang, &engine::set(ctx, &wants), out),
        Command::Check { wants, json } => {
            let checked = engine::check(ctx, &wants);
            let all_same = checked.iter().all(|c| c.same);
            if json {
                let settings: Vec<Value> = checked
                    .iter()
                    .map(|c| {
                        let mut v = item_json(&c.item, ctx.version);
                        v["wanted"] = Value::from(c.wanted.as_str());
                        v["same"] = Value::from(c.same);
                        v
                    })
                    .collect();
                let _ = writeln!(
                    out,
                    "{}",
                    json!({"v": 1, "same": all_same, "settings": settings})
                );
            } else {
                for c in &checked {
                    let _ = writeln!(out, "{}", text::checked(lang, c, ctx.version));
                }
            }
            if checked
                .iter()
                .any(|c| matches!(c.item.state, State::Unreadable(_)))
            {
                2
            } else if all_same {
                0
            } else {
                1
            }
        }
        Command::Relay(act) => changes(lang, &engine::relay(ctx, act), out),
        Command::ForgetAccounts => changes(lang, &engine::forget_accounts(ctx), out),
        Command::OpenRecords => changes(lang, &engine::open_records(ctx), out),
    }
}

/// The prefix of op-reason's lines on stderr.
const PREFIX: &str = crate::messages::PREFIX;

fn changes(lang: Lang, changes: &[Change], out: &mut dyn Write) -> i32 {
    for c in changes {
        let _ = writeln!(out, "{}", text::change(lang, c));
    }
    if changes.iter().any(|c| matches!(c.done, Done::Failed(_))) {
        1
    } else {
        0
    }
}

fn row(lang: Lang, item: &Item, version: &str) -> [String; 4] {
    let state = text::state(lang, item, version);
    let mut kept = item.where_.clone();
    if !state.is_empty() {
        if !kept.is_empty() {
            kept.push_str(" · ");
        }
        kept.push_str(&state);
    }
    [
        item.name.text(),
        item.value.clone().unwrap_or_else(|| "-".into()),
        item.default.unwrap_or("-").to_owned(),
        kept,
    ]
}

/// The rows padded to their columns (wide characters count twice), the last column as it is.
fn table(rows: &[[String; 4]]) -> Vec<String> {
    let mut widths = [0usize; 4];
    for r in rows {
        for (w, c) in widths.iter_mut().zip(r.iter()) {
            *w = (*w).max(display_width(c));
        }
    }
    rows.iter()
        .map(|r| {
            let mut line = String::new();
            for (k, c) in r.iter().enumerate() {
                line.push_str(c);
                if k + 1 < r.len() {
                    line.push_str(&" ".repeat(widths[k] - display_width(c) + 2));
                }
            }
            line.trim_end().to_owned()
        })
        .collect()
}

/// A setting in the JSON of `list`, `get` and `check`: `state` is a code, `detail` its facts.
fn item_json(item: &Item, version: &str) -> Value {
    let (code, detail) = match &item.state {
        State::Ok => ("ok", json!({})),
        State::Ignored(raw) => ("ignored", json!({"raw": raw})),
        State::RelayDiffers { relay, pid } => {
            ("relay-differs", json!({"relay": relay, "pid": pid}))
        }
        State::TaskDiffers(fields) => ("task-differs", json!({"fields": fields})),
        State::TaskOff => ("task-off", json!({})),
        State::TaskStarting => ("task-starting", json!({})),
        State::OtherRelay { pid, by } => ("other-relay", json!({"pid": pid, "by": by})),
        State::Running { pid, ours, by } => {
            ("running", json!({"pid": pid, "ours": ours, "by": by}))
        }
        State::Skill { state, link } => {
            let (file, placed) = match state {
                SkillState::Missing => ("missing", None),
                SkillState::Ok => ("ok", Some(version.to_owned())),
                SkillState::Old(v) => ("old", Some(v.clone())),
                SkillState::Changed => ("changed", Some(version.to_owned())),
                SkillState::Foreign => ("foreign", None),
            };
            let code = if link.is_some() { "link" } else { file };
            (code, json!({"file": file, "version": placed, "link": link}))
        }
        State::OtherTasks(tasks) => ("other-tasks", json!({"tasks": tasks})),
        State::Shortcut(state) => match state {
            ShortcutState::Missing => ("missing", json!({})),
            ShortcutState::Ok => ("ok", json!({})),
            ShortcutState::Differs(t) => ("shortcut-differs", json!({"target": t})),
            ShortcutState::Foreign(t) => ("foreign", json!({"target": t})),
        },
        State::NotRunning => ("not-running", json!({})),
        State::Unreadable(e) => ("unreadable", json!({"error": e})),
    };
    json!({
        "name": item.name.text(),
        "kind": if item.name.is_file() { "file" } else { "system" },
        "value": item.value,
        "default": item.default,
        "state": code,
        "detail": detail,
        "where": item.where_,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::catalog::{Name, Usage, Want};
    use crate::settings::values::RelayMode;

    fn p(args: &[&str]) -> Result<Command, Usage> {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn words() {
        assert_eq!(p(&[]), Ok(Command::Window));
        assert_eq!(p(&["help"]), Ok(Command::Help));
        assert_eq!(p(&["--help"]), Ok(Command::Help));
        assert_eq!(p(&["list"]), Ok(Command::List { json: false }));
        assert_eq!(p(&["list", "--json"]), Ok(Command::List { json: true }));
        assert_eq!(
            p(&["get", "--json", "relay.mode"]),
            Ok(Command::Get {
                name: Name::RelayMode,
                json: true
            })
        );
        assert_eq!(p(&["relay", "stop"]), Ok(Command::Relay(RelayAct::Stop)));
        assert_eq!(
            p(&["relay", "restart"]),
            Ok(Command::Relay(RelayAct::Restart))
        );
        assert_eq!(p(&["forget-accounts"]), Ok(Command::ForgetAccounts));
        assert_eq!(p(&["open-records"]), Ok(Command::OpenRecords));
    }

    #[test]
    fn set_takes_a_name_and_a_value_or_pairs() {
        assert_eq!(
            p(&["set", "relay.mode", "show-only"]),
            Ok(Command::Set(vec![(
                Name::RelayMode,
                Want::Mode(RelayMode::ShowOnly)
            )]))
        );
        assert_eq!(
            p(&["set", "relay.mode=show-only", "relay.autostart=on"]),
            Ok(Command::Set(vec![
                (Name::RelayMode, Want::Mode(RelayMode::ShowOnly)),
                (Name::RelayAutostart, Want::On(true)),
            ]))
        );
        assert_eq!(
            p(&["set", r"skill.folder.D:\a=b=on"]),
            Ok(Command::Set(vec![(
                Name::SkillFolder(r"D:\a=b".into()),
                Want::On(true)
            )])),
            "a pair splits at its last ="
        );
        assert_eq!(p(&["set"]), Err(Usage::NoValue("set".into())));
        assert_eq!(
            p(&["set", "relay.mode"]),
            Err(Usage::NoValue("relay.mode".into()))
        );
        assert_eq!(
            p(&["set", "relay.mode=show-only", "relay.mode=per-call"]),
            Err(Usage::Twice("relay.mode".into()))
        );
        assert!(p(&["set", "relay.mode", "show-only", "--json"]).is_err());
    }

    #[test]
    fn check_takes_pairs_only() {
        assert_eq!(
            p(&["check", "relay.autostart=on", "--json", "relay.running=on"]),
            Ok(Command::Check {
                wants: vec![
                    (Name::RelayAutostart, Want::On(true)),
                    (Name::RelayRunning, Want::On(true)),
                ],
                json: true
            })
        );
        assert_eq!(
            p(&["check", "relay.mode"]),
            Err(Usage::NoValue("relay.mode".into()))
        );
        assert_eq!(p(&["check"]), Err(Usage::NoValue("check".into())));
    }

    #[test]
    fn mistakes() {
        assert_eq!(p(&["nope"]), Err(Usage::UnknownWord("nope".into())));
        assert_eq!(
            p(&["relay", "pause"]),
            Err(Usage::UnknownWord("pause".into()))
        );
        assert_eq!(p(&["list", "extra"]), Err(Usage::Extra("extra".into())));
        assert_eq!(p(&["get"]), Err(Usage::NoValue("get".into())));
        assert_eq!(p(&["get", "nope"]), Err(Usage::UnknownName("nope".into())));
        assert_eq!(
            p(&["forget-accounts", "--json"]),
            Err(Usage::Extra("--json".into()))
        );
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;
    use crate::lang::Lang;
    use crate::settings::catalog::{Name, Want};
    use crate::settings::fake::{Fake, OURS};
    use crate::settings::skill::{SkillNow, SkillState};
    use crate::settings::values::RelayMode;
    use serde_json::Value;

    fn run_with(f: &Fake, cmd: Command, lang: Lang) -> (i32, String, String) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run(cmd, &f.ctx(), lang, &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn list_prints_a_table() {
        let f = Fake::new().ours_running();
        let (code, out, _) = run_with(&f, Command::List { json: false }, Lang::En);
        assert_eq!(code, 0);
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].starts_with("Setting"), "{out}");
        let col = lines[0].find("Now").unwrap();
        let mode = lines.iter().find(|l| l.starts_with("relay.mode ")).unwrap();
        assert_eq!(&mode[col..col + 8], "per-call", "aligned: {out}");
        assert!(
            out.contains("pid 100, started by op-reason's task"),
            "{out}"
        );
        assert!(
            out.contains("not checked"),
            "the stopped distribution: {out}"
        );
        assert!(out.contains("forget-accounts"), "{out}");
    }

    #[test]
    fn get_prints_the_value_alone() {
        let f = Fake::new();
        let (code, out, _) = run_with(
            &f,
            Command::Get {
                name: Name::RelayMode,
                json: false,
            },
            Lang::En,
        );
        assert_eq!((code, out.as_str()), (0, "per-call\n"));
        let (code, out, _) = run_with(
            &f,
            Command::Get {
                name: Name::RelayTimeLimit,
                json: true,
            },
            Lang::En,
        );
        assert_eq!(code, 0);
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["v"], 1);
        assert_eq!(v["settings"][0]["name"], "relay.time-limit");
        assert_eq!(v["settings"][0]["value"], "60m");
        assert_eq!(v["settings"][0]["kind"], "file");
        assert_eq!(v["settings"][0]["state"], "ok");
    }

    #[test]
    fn get_of_what_cannot_be_read_fails() {
        let f = Fake::new();
        f.skills.borrow_mut().insert(
            "wsl:Gone".into(),
            SkillNow {
                state: SkillState::Missing,
                link: None,
                path: "p".into(),
            },
        );
        *f.fail_skill.borrow_mut() = Some("there is no distribution with that name".into());
        let (code, out, err) = run_with(
            &f,
            Command::Get {
                name: Name::SkillWsl("Gone".into()),
                json: false,
            },
            Lang::En,
        );
        assert_eq!((code, out.as_str()), (2, ""));
        assert!(err.contains("there is no distribution"), "{err}");
    }

    #[test]
    fn check_tells_same_different_and_failed_apart() {
        let f = Fake::new().ours_running();
        let same = vec![
            (Name::RelayAutostart, Want::On(true)),
            (Name::RelayRunning, Want::On(true)),
        ];
        let (code, out, _) = run_with(
            &f,
            Command::Check {
                wants: same,
                json: false,
            },
            Lang::En,
        );
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("relay.autostart: same (on)"), "{out}");
        *f.settings.borrow_mut() = Some(r#"{"relay":{"mode":"show-only"}}"#.into());
        let wants = vec![(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))];
        let (code, out, _) = run_with(
            &f,
            Command::Check {
                wants: wants.clone(),
                json: false,
            },
            Lang::En,
        );
        assert_eq!(code, 1);
        assert!(out.contains("relay.mode: different"), "{out}");
        assert!(out.contains("pid 100"), "{out}");
        let (code, out, _) = run_with(&f, Command::Check { wants, json: true }, Lang::En);
        assert_eq!(code, 1);
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["same"], false);
        assert_eq!(v["settings"][0]["wanted"], "show-only");
        assert_eq!(v["settings"][0]["value"], "show-only");
        assert_eq!(v["settings"][0]["state"], "relay-differs");
        assert_eq!(v["settings"][0]["detail"]["relay"], "per-call");
        assert_eq!(v["settings"][0]["detail"]["pid"], OURS);
        *f.fail_skill.borrow_mut() = Some("wsl.exe failed".into());
        let wants = vec![(Name::SkillWsl("Ubuntu".into()), Want::On(true))];
        let (code, _, _) = run_with(&f, Command::Check { wants, json: false }, Lang::En);
        assert_eq!(code, 2, "a value that could not be read");
    }

    #[test]
    fn set_says_what_it_did_and_fails_with_1() {
        let f = Fake::new();
        let (code, out, _) = run_with(
            &f,
            Command::Set(vec![(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))]),
            Lang::En,
        );
        assert_eq!(code, 0);
        assert!(out.contains("relay.mode: per-call → show-only"), "{out}");
        let (code, out, _) = run_with(
            &f,
            Command::Set(vec![(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))]),
            Lang::En,
        );
        assert_eq!(code, 0);
        assert!(out.contains("relay.mode: show-only (already)"), "{out}");
        let (code, out, _) = run_with(&f, Command::Relay(RelayAct::Start), Lang::En);
        assert_eq!(code, 1);
        assert!(out.contains("set relay.autostart on"), "{out}");
    }

    #[test]
    fn japanese_texts() {
        let f = Fake::new().other_running();
        let (code, out, _) = run_with(&f, Command::Relay(RelayAct::Stop), Lang::Ja);
        assert_eq!(code, 1);
        assert!(out.contains("止めません"), "{out}");
        let (_, out, _) = run_with(&f, Command::List { json: false }, Lang::Ja);
        assert!(out.starts_with("設定"), "{out}");
        assert!(out.contains("別の task"), "{out}");
    }

    #[test]
    fn usage_errors_are_named() {
        let u = Usage::BadValue {
            name: "relay.mode".into(),
            value: "later".into(),
        };
        assert!(usage_error(Lang::En, &u).contains("later"));
        assert!(usage_error(Lang::Ja, &u).contains("later"));
        assert!(usage(Lang::En).contains("op-reason --settings check"));
        assert!(usage(Lang::Ja).contains("op-reason --settings check"));
    }
}

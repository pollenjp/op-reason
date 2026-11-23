//! The words of `op-reason --settings` (design `2026-11-23-settings-design.md` §7.1).

use crate::settings::catalog::{self, Name, Usage, Want};

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

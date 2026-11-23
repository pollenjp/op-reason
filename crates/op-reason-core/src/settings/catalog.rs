//! The list of settings (design `2026-11-23-settings-design.md` §4): their names, which are kept in `settings.json`,
//! the values each takes and its default.

use crate::lang::Lang;
use crate::settings::values::{self, RelayMode};

/// A setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Name {
    /// `lang` (file).
    Lang,
    /// `relay.mode` (file).
    RelayMode,
    /// `relay.time-limit` (file).
    RelayTimeLimit,
    /// `relay.answer-wait` (file).
    RelayAnswerWait,
    /// `relay.autostart`: op-reason's scheduled task.
    RelayAutostart,
    /// `relay.running`: the relay that answers on its pipe.
    RelayRunning,
    /// `skill.windows`: Claude Code's skills folder on Windows.
    SkillWindows,
    /// `skill.wsl.<distribution>`.
    SkillWsl(String),
    /// `skill.folder.<folder>`: a folder the user chose.
    SkillFolder(String),
}

/// The settings that are always there, in the order of `list` (the WSL distributions and the chosen folders follow).
pub fn fixed() -> Vec<Name> {
    vec![
        Name::Lang,
        Name::RelayMode,
        Name::RelayTimeLimit,
        Name::RelayAnswerWait,
        Name::RelayAutostart,
        Name::RelayRunning,
        Name::SkillWindows,
    ]
}

const WSL_PREFIX: &str = "skill.wsl.";
const FOLDER_PREFIX: &str = "skill.folder.";

impl Name {
    /// A name as the command line writes it.
    pub fn parse(text: &str) -> Result<Name, Usage> {
        let name = match text {
            "lang" => Name::Lang,
            "relay.mode" => Name::RelayMode,
            "relay.time-limit" => Name::RelayTimeLimit,
            "relay.answer-wait" => Name::RelayAnswerWait,
            "relay.autostart" => Name::RelayAutostart,
            "relay.running" => Name::RelayRunning,
            "skill.windows" => Name::SkillWindows,
            _ => {
                if let Some(distro) = text.strip_prefix(WSL_PREFIX) {
                    if !distro_name(distro) {
                        return Err(Usage::BadDistro(distro.to_owned()));
                    }
                    Name::SkillWsl(distro.to_owned())
                } else if let Some(folder) = text.strip_prefix(FOLDER_PREFIX) {
                    if !absolute(folder) {
                        return Err(Usage::RelativeFolder(folder.to_owned()));
                    }
                    Name::SkillFolder(trim_folder(folder).to_owned())
                } else {
                    return Err(Usage::UnknownName(text.to_owned()));
                }
            }
        };
        Ok(name)
    }

    /// The name as the command line writes it.
    pub fn text(&self) -> String {
        match self {
            Name::Lang => "lang".into(),
            Name::RelayMode => "relay.mode".into(),
            Name::RelayTimeLimit => "relay.time-limit".into(),
            Name::RelayAnswerWait => "relay.answer-wait".into(),
            Name::RelayAutostart => "relay.autostart".into(),
            Name::RelayRunning => "relay.running".into(),
            Name::SkillWindows => "skill.windows".into(),
            Name::SkillWsl(d) => format!("{WSL_PREFIX}{d}"),
            Name::SkillFolder(f) => format!("{FOLDER_PREFIX}{f}"),
        }
    }

    /// Kept in `settings.json` (read by op-reason itself), rather than where the system reads it.
    pub fn is_file(&self) -> bool {
        matches!(
            self,
            Name::Lang | Name::RelayMode | Name::RelayTimeLimit | Name::RelayAnswerWait
        )
    }

    /// The default as `get` prints it; `relay.running` has none (it is a state).
    pub fn default_text(&self) -> Option<&'static str> {
        match self {
            Name::Lang => Some("default"),
            Name::RelayMode => Some(RelayMode::default().as_str()),
            Name::RelayTimeLimit => Some("60m"),
            Name::RelayAnswerWait => Some("60s"),
            Name::RelayRunning => None,
            _ => Some("off"),
        }
    }
}

/// A distribution's name as `wsl.exe --list` gives it: letters, digits, `.`, `_` and `-`, not starting with `-`.
pub fn distro_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// An absolute folder: `X:\…` or `X:/…`, `\\server\share…`, or `/…`.
pub fn absolute(folder: &str) -> bool {
    let b = folder.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/'))
        || folder.starts_with(r"\\")
        || folder.starts_with('/')
}

/// A folder without its trailing separators, unless it is a root (`C:\`, `/`).
fn trim_folder(folder: &str) -> &str {
    let trimmed = folder.trim_end_matches(['\\', '/']);
    if trimmed.is_empty() || (trimmed.len() == 2 && trimmed.ends_with(':')) {
        &folder[..(trimmed.len() + 1).min(folder.len())]
    } else {
        trimmed
    }
}

/// A value asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Want {
    /// `default` of a relay value: not set in `settings.json` (the default counts).
    Default,
    /// `lang`: `en`, `ja`, or `default` (`None`: not chosen, `OPREASON_LANG` then English).
    Lang(Option<Lang>),
    /// `relay.mode`.
    Mode(RelayMode),
    /// `relay.time-limit`, `relay.answer-wait`: seconds.
    Secs(u64),
    /// `on` or `off`.
    On(bool),
}

/// A mistake on the command line: nothing is done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Usage {
    /// A word that `--settings` does not know.
    UnknownWord(String),
    /// A setting that does not exist.
    UnknownName(String),
    /// A value that the setting does not take.
    BadValue {
        /// The setting.
        name: String,
        /// The value given.
        value: String,
    },
    /// A distribution name that `wsl.exe` would not give.
    BadDistro(String),
    /// A folder that is not absolute.
    RelativeFolder(String),
    /// A word or a setting without what must follow it.
    NoValue(String),
    /// A setting named twice in one command.
    Twice(String),
    /// An argument too many.
    Extra(String),
}

/// The value `text` for the setting `name`.
pub fn want(name: &Name, text: &str) -> Result<Want, Usage> {
    let bad = || Usage::BadValue {
        name: name.text(),
        value: text.to_owned(),
    };
    match name {
        Name::Lang => match text {
            "default" => Ok(Want::Lang(None)),
            _ => Lang::parse(text)
                .map(|l| Want::Lang(Some(l)))
                .ok_or_else(bad),
        },
        _ if text == "default" && name.is_file() => Ok(Want::Default),
        Name::RelayMode => RelayMode::parse(text).map(Want::Mode).ok_or_else(bad),
        Name::RelayTimeLimit => values::duration(text, 60, true)
            .map(Want::Secs)
            .ok_or_else(bad),
        Name::RelayAnswerWait => values::duration(text, 1, false)
            .map(Want::Secs)
            .ok_or_else(bad),
        _ => match text {
            "on" => Ok(Want::On(true)),
            "off" => Ok(Want::On(false)),
            _ => Err(bad()),
        },
    }
}

/// A value as the command line writes it.
pub fn want_text(name: &Name, want: &Want) -> String {
    match want {
        Want::Default | Want::Lang(None) => "default".into(),
        Want::Lang(Some(l)) => l.code().into(),
        Want::Mode(m) => m.as_str().into(),
        Want::Secs(s) => match name {
            Name::RelayTimeLimit => values::minutes_text(*s),
            _ => values::seconds_text(*s),
        },
        Want::On(true) => "on".into(),
        Want::On(false) => "off".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::settings::values::RelayMode;

    #[test]
    fn names_round_trip() {
        for text in [
            "lang",
            "relay.mode",
            "relay.time-limit",
            "relay.answer-wait",
            "relay.autostart",
            "relay.running",
            "skill.windows",
            "skill.wsl.Ubuntu-24.04",
            r"skill.folder.D:\agents\skills",
            "skill.folder./tmp/skills",
        ] {
            assert_eq!(Name::parse(text).unwrap().text(), text, "{text}");
        }
        assert_eq!(
            Name::parse("skill.wsl.Ubuntu-24.04"),
            Ok(Name::SkillWsl("Ubuntu-24.04".into()))
        );
        assert_eq!(
            Name::parse(r"skill.folder.C:\x\"),
            Ok(Name::SkillFolder(r"C:\x".into())),
            "a trailing separator goes"
        );
        assert_eq!(
            Name::parse(r"skill.folder.C:\"),
            Ok(Name::SkillFolder(r"C:\".into())),
            "but not the root's"
        );
    }

    #[test]
    fn names_that_are_refused() {
        for bad in [
            "",
            "relay",
            "relay.modes",
            "Lang",
            "skill.wsl.",
            "skill.wsl.-x",
            "skill.wsl.a b",
        ] {
            assert!(Name::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            Name::parse("skill.folder.relative/skills"),
            Err(Usage::RelativeFolder("relative/skills".into()))
        );
        assert_eq!(
            Name::parse("skill.wsl.a;b"),
            Err(Usage::BadDistro("a;b".into()))
        );
        assert_eq!(Name::parse("nope"), Err(Usage::UnknownName("nope".into())));
    }

    #[test]
    fn kinds_and_defaults() {
        assert!(Name::Lang.is_file() && Name::RelayAnswerWait.is_file());
        assert!(!Name::RelayAutostart.is_file() && !Name::SkillWindows.is_file());
        assert_eq!(Name::Lang.default_text(), Some("default"));
        assert_eq!(Name::RelayMode.default_text(), Some("per-call"));
        assert_eq!(Name::RelayTimeLimit.default_text(), Some("60m"));
        assert_eq!(Name::RelayAnswerWait.default_text(), Some("60s"));
        assert_eq!(Name::RelayAutostart.default_text(), Some("off"));
        assert_eq!(Name::SkillWsl("U".into()).default_text(), Some("off"));
        assert_eq!(Name::RelayRunning.default_text(), None);
        assert_eq!(fixed().len(), 7);
    }

    #[test]
    fn values() {
        assert_eq!(want(&Name::Lang, "ja"), Ok(Want::Lang(Some(Lang::Ja))));
        assert_eq!(want(&Name::Lang, "default"), Ok(Want::Lang(None)));
        assert_eq!(
            want(&Name::RelayMode, "show-only"),
            Ok(Want::Mode(RelayMode::ShowOnly))
        );
        assert_eq!(want(&Name::RelayMode, "default"), Ok(Want::Default));
        assert_eq!(
            want(&Name::RelayMode, "later"),
            Err(Usage::BadValue {
                name: "relay.mode".into(),
                value: "later".into()
            })
        );
        assert_eq!(want(&Name::RelayTimeLimit, "90"), Ok(Want::Secs(5400)));
        assert_eq!(want(&Name::RelayAnswerWait, "90"), Ok(Want::Secs(90)));
        assert!(want(&Name::RelayAnswerWait, "1h").is_err());
        assert_eq!(want(&Name::RelayAutostart, "on"), Ok(Want::On(true)));
        assert_eq!(want(&Name::SkillWindows, "off"), Ok(Want::On(false)));
        assert!(want(&Name::RelayRunning, "yes").is_err());
        assert!(
            want(&Name::RelayRunning, "default").is_err(),
            "only file settings have a default to go back to"
        );
        assert_eq!(want_text(&Name::RelayTimeLimit, &Want::Secs(5400)), "90m");
        assert_eq!(want_text(&Name::RelayAnswerWait, &Want::Secs(60)), "60s");
        assert_eq!(want_text(&Name::Lang, &Want::Lang(None)), "default");
        assert_eq!(want_text(&Name::RelayMode, &Want::Default), "default");
        assert_eq!(want_text(&Name::SkillWindows, &Want::On(true)), "on");
    }
}

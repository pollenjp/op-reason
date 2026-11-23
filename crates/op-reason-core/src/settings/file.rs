//! `settings.json`: the language chosen in the window of `op-reason.exe` (design `2026-11-15-language-toggle-design.md`
//! §3 and §5), and the values of the settings that op-reason reads itself (design `2026-11-23-settings-design.md` §6:
//! the relay's mode and times, whether it shows its icon, the folders where the skill was placed).
//!
//! - Windows: `%LOCALAPPDATA%\op-reason\settings.json`, written by the window's switch, read by `op-reason.exe`, the
//!   launcher and the PowerShell entry.
//! - WSL: `<state>/lang.json`, the WSL executable's copy of the choice, taken from the `chosenLang` of each call's
//!   status.
//!
//! Both hold `{"v":1,"lang":"en"|"ja"}`. Readers use `lang` when it is `en` or `ja`, whatever `v` says; anything else
//! (no file, not a JSON object, no or an unknown `lang`) means that nothing was chosen. Writers keep the fields they
//! do not know, and `v` when there is one.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};

use crate::lang::Lang;
use crate::settings::values::{self, RelayMode};

/// The file on Windows, in `%LOCALAPPDATA%\op-reason`.
pub const SETTINGS_FILE: &str = "settings.json";
/// The WSL executable's copy, in its state directory.
pub const COPY_FILE: &str = "lang.json";
/// The version of the format.
pub const VERSION: u64 = 1;
/// The path of [`Values::ignored`] for a text that is not a JSON object at all.
pub const WHOLE_FILE: &str = "settings.json";

/// How many times a write tries to put the new file in place (another process may have the old one open).
const WRITE_TRIES: u32 = 5;
const WRITE_PAUSE: Duration = Duration::from_millis(20);

/// The chosen language in a file's text (a byte order mark is allowed).
pub fn parse(text: &str) -> Option<Lang> {
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    Lang::parse(value.as_object()?.get("lang")?.as_str()?)
}

/// The values of `settings.json` that op-reason reads itself. `None` is "not set": the default counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values {
    /// `lang`: the language chosen in a window or by `set lang`.
    pub lang: Option<Lang>,
    /// `relay.mode`.
    pub mode: Option<RelayMode>,
    /// `relay.timeLimit`, seconds above zero.
    pub time_limit: Option<u64>,
    /// `relay.answerWait`, seconds above zero.
    pub answer_wait: Option<u64>,
    /// `relay.trayIcon`.
    pub tray_icon: Option<bool>,
    /// `skill.folders`: the folders where `set skill.folder.<folder> on` placed the skill.
    pub folders: Vec<String>,
    /// Fields that were there but not accepted: their path (`relay.mode`) and their JSON text. They count as not set.
    pub ignored: Vec<(String, String)>,
}

/// The values in a file's text (a byte order mark is allowed); no file, or a text that is not a JSON object, has none.
pub fn values(text: Option<&str>) -> Values {
    let Some(root) = text.and_then(object) else {
        // A text that is not a JSON object counts as no file; it is named, as the next write replaces it
        let mut v = Values::default();
        if let Some(t) = text.map(|t| t.trim_start_matches('\u{feff}').trim())
            && !t.is_empty()
        {
            v.ignored
                .push((WHOLE_FILE.to_owned(), crate::args::clip(t, 40, "…")));
        }
        return v;
    };
    let mut ignored = Vec::new();
    let mut ignore = |path: &str, value: &Value| ignored.push((path.to_owned(), value.to_string()));
    let mut lang = None;
    if let Some(l) = root.get("lang") {
        lang = l.as_str().and_then(Lang::parse);
        if lang.is_none() {
            ignore("lang", l);
        }
    }
    let mut mode = None;
    let mut time_limit = None;
    let mut answer_wait = None;
    let mut tray_icon = None;
    match root.get("relay") {
        Some(Value::Object(relay)) => {
            if let Some(m) = relay.get("mode") {
                mode = m.as_str().and_then(RelayMode::parse);
                if mode.is_none() {
                    ignore("relay.mode", m);
                }
            }
            for (key, slot, max) in [
                ("timeLimit", &mut time_limit, values::MAX_TIME_LIMIT),
                ("answerWait", &mut answer_wait, values::MAX_ANSWER_WAIT),
            ] {
                if let Some(n) = relay.get(key) {
                    *slot = n.as_u64().filter(|s| *s > 0 && *s <= max);
                    if slot.is_none() {
                        ignore(&format!("relay.{key}"), n);
                    }
                }
            }
            if let Some(t) = relay.get("trayIcon") {
                tray_icon = t.as_bool();
                if tray_icon.is_none() {
                    ignore("relay.trayIcon", t);
                }
            }
        }
        Some(other) => ignore("relay", other),
        None => {}
    }
    let mut folders = Vec::new();
    match root.get("skill") {
        Some(Value::Object(skill)) => match skill.get("folders") {
            Some(Value::Array(list)) => {
                for f in list {
                    match f.as_str() {
                        Some(f) => folders.push(f.to_owned()),
                        None => ignore("skill.folders", f),
                    }
                }
            }
            Some(other) => ignore("skill.folders", other),
            None => {}
        },
        Some(other) => ignore("skill", other),
        None => {}
    }
    Values {
        lang,
        mode,
        time_limit,
        answer_wait,
        tray_icon,
        folders,
        ignored,
    }
}

/// A text's JSON object (a byte order mark is allowed).
fn object(text: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok()? {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

/// The field at `path` in a file's text, when there is one.
pub fn field(text: Option<&str>, path: &[&str]) -> Option<Value> {
    let mut here = Value::Object(text.and_then(object)?);
    for key in path {
        here = here.as_object_mut()?.remove(*key)?;
    }
    Some(here)
}

/// The text of a file after `change` changed its fields: `v` first (the old one, else 1), then `lang` when there is
/// one, then the other fields in the order of their names. A missing or broken old text starts from no fields.
pub fn edit(old: Option<&str>, change: impl FnOnce(&mut Map<String, Value>)) -> String {
    let mut fields = old.and_then(object).unwrap_or_default();
    let v = fields.remove("v").unwrap_or(Value::from(VERSION));
    change(&mut fields);
    let mut out = format!(r#"{{"v":{v}"#);
    let lang = fields.remove("lang");
    let rest = lang.iter().map(|l| ("lang", l)).chain(
        fields
            .iter()
            .filter(|(k, _)| *k != "v")
            .map(|(k, v)| (k.as_str(), v)),
    );
    for (key, value) in rest {
        out.push(',');
        out.push_str(&Value::from(key).to_string());
        out.push(':');
        out.push_str(&value.to_string());
    }
    out.push('}');
    out
}

/// Puts `value` at `path` (`["relay", "mode"]`), making the objects on the way (a field on the way that is not an
/// object is replaced); `None` removes it, and then the objects on the way that it left empty.
pub fn put(map: &mut Map<String, Value>, path: &[&str], value: Option<Value>) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    match value {
        Some(value) => {
            let mut here = map;
            for key in parents {
                let slot = here
                    .entry((*key).to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if !slot.is_object() {
                    *slot = Value::Object(Map::new());
                }
                here = slot.as_object_mut().expect("an object was just put here");
            }
            here.insert((*last).to_owned(), value);
        }
        None => {
            remove_at(map, parents, last);
        }
    }
}

/// Removes `parents…/last`; returns whether `map` is empty afterwards (so that the caller removes it).
fn remove_at(map: &mut Map<String, Value>, parents: &[&str], last: &str) -> bool {
    match parents.split_first() {
        None => {
            map.remove(last);
        }
        Some((first, rest)) => {
            if let Some(Value::Object(child)) = map.get_mut(*first)
                && remove_at(child, rest, last)
            {
                map.remove(*first);
            }
        }
    }
    map.is_empty()
}

/// The text of a file that says `lang`: `{"v":1,"lang":"…"}`, or, when `old` is a JSON object, its `v` (1 when it has
/// none), `lang`, then its other fields as they were.
pub fn render(old: Option<&str>, lang: Lang) -> String {
    edit(old, |fields| {
        fields.insert("lang".to_owned(), Value::from(lang.code()));
    })
}

/// Reads the chosen language. `Ok(None)` when the file is missing or holds no choice; `Err` for other read errors
/// (the caller cannot tell).
pub fn read(path: &Path) -> io::Result<Option<Lang>> {
    match fs::read(path) {
        Ok(bytes) => Ok(parse(&String::from_utf8_lossy(&bytes))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Writes `lang` into the file, keeping its other fields: a temporary file in the same folder (created when missing)
/// renamed over it, so that a reader never sees half a file.
pub fn write(path: &Path, lang: Lang) -> io::Result<()> {
    let old = fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    replace(path, &render(old.as_deref(), lang))
}

/// Puts `text` in place of the file: a temporary file in the same folder (the folder is created when missing),
/// renamed over the file and retried for a moment when another process has the old one open, so that a reader never
/// sees half a file. Also used for `account.json` (design `2026-11-17-refs-in-window-design.md` §5.2).
pub fn replace(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let tmp = temporary(path);
    fs::write(&tmp, text.as_bytes())?;
    let mut last = None;
    for attempt in 0..WRITE_TRIES {
        if attempt > 0 {
            std::thread::sleep(WRITE_PAUSE);
        }
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    let _ = fs::remove_file(&tmp);
    Err(last.unwrap_or_else(|| io::Error::other("rename failed")))
}

/// Removes the file; a missing file is fine.
pub fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// `<file>.<pid>.tmp` next to the file.
fn temporary(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::values::RelayMode;

    #[test]
    fn parse_reads_lang_whatever_v_says() {
        assert_eq!(parse(r#"{"v":1,"lang":"ja"}"#), Some(Lang::Ja));
        assert_eq!(parse(r#"{"lang":"en"}"#), Some(Lang::En));
        assert_eq!(
            parse(r#"{"v":7,"lang":"ja","theme":"dark"}"#),
            Some(Lang::Ja)
        );
        assert_eq!(parse("\u{feff}{\"v\":1,\"lang\":\"en\"}"), Some(Lang::En));
    }

    #[test]
    fn parse_treats_anything_else_as_no_choice() {
        for text in [
            "",
            "junk",
            "{",
            r#""ja""#,
            r#"["ja"]"#,
            r#"{"v":1}"#,
            r#"{"v":1,"lang":"fr"}"#,
            r#"{"v":1,"lang":"JA"}"#,
            r#"{"v":1,"lang":1}"#,
            r#"{"v":1,"lang":null}"#,
        ] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn render_writes_a_fresh_file_or_keeps_what_it_does_not_know() {
        assert_eq!(render(None, Lang::Ja), r#"{"v":1,"lang":"ja"}"#);
        assert_eq!(render(Some("junk"), Lang::En), r#"{"v":1,"lang":"en"}"#);
        assert_eq!(render(Some(r#"["x"]"#), Lang::En), r#"{"v":1,"lang":"en"}"#);
        assert_eq!(
            render(Some(r#"{"v":1,"lang":"en"}"#), Lang::Ja),
            r#"{"v":1,"lang":"ja"}"#
        );
        assert_eq!(
            render(
                Some(r#"{"theme":{"dark":true},"lang":"en","v":2}"#),
                Lang::Ja
            ),
            r#"{"v":2,"lang":"ja","theme":{"dark":true}}"#
        );
        assert_eq!(
            render(Some(r#"{"lang":"fr","x":"日本"}"#), Lang::En),
            r#"{"v":1,"lang":"en","x":"日本"}"#
        );
        for lang in [Lang::En, Lang::Ja] {
            assert_eq!(parse(&render(Some(r#"{"a":1}"#), lang)), Some(lang));
        }
    }

    #[test]
    fn values_reads_the_relay_and_the_skill() {
        let v = values(Some(
            r#"{"v":1,"lang":"ja","relay":{"mode":"show-only","timeLimit":90,"answerWait":5,"trayIcon":false},"skill":{"folders":["D:\\a","E:\\b"]}}"#,
        ));
        assert_eq!(v.lang, Some(Lang::Ja));
        assert_eq!(v.mode, Some(RelayMode::ShowOnly));
        assert_eq!((v.time_limit, v.answer_wait), (Some(90), Some(5)));
        assert_eq!(v.tray_icon, Some(false));
        assert_eq!(v.folders, vec![r"D:\a".to_owned(), r"E:\b".to_owned()]);
        assert!(v.ignored.is_empty());
        assert_eq!(values(None), Values::default());
        assert_eq!(
            values(Some("junk")).ignored,
            [(WHOLE_FILE.to_owned(), "junk".to_owned())],
            "a broken file is named"
        );
        assert_eq!(
            values(Some("  ")),
            Values::default(),
            "an empty file is no file"
        );
        assert_eq!(
            values(Some("\u{feff}{\"relay\":{\"mode\":\"per-call\"}}")).mode,
            Some(RelayMode::PerCall)
        );
    }

    #[test]
    fn values_ignores_what_it_does_not_accept() {
        let v = values(Some(
            r#"{"lang":"fr","relay":{"mode":"later","timeLimit":0,"answerWait":"60","trayIcon":"yes"},"skill":{"folders":["D:\\a",3]}}"#,
        ));
        assert_eq!(v.lang, None);
        assert_eq!(v.mode, None);
        assert_eq!(
            (v.time_limit, v.answer_wait, v.tray_icon),
            (None, None, None)
        );
        assert_eq!(v.folders, vec![r"D:\a".to_owned()]);
        let keys: Vec<&str> = v.ignored.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "lang",
                "relay.mode",
                "relay.timeLimit",
                "relay.answerWait",
                "relay.trayIcon",
                "skill.folders"
            ]
        );
        assert_eq!(v.ignored[1].1, r#""later""#);
        assert_eq!(values(Some(r#"{"relay":7}"#)).ignored[0].0, "relay");
    }

    #[test]
    fn values_beyond_a_day_or_an_hour_are_not_taken() {
        let huge = values(Some(r#"{"relay":{"timeLimit":86401,"answerWait":3601}}"#));
        assert_eq!((huge.time_limit, huge.answer_wait), (None, None));
        assert_eq!(huge.ignored.len(), 2);
        let most = values(Some(r#"{"relay":{"timeLimit":86400,"answerWait":3600}}"#));
        assert_eq!(
            (most.time_limit, most.answer_wait),
            (Some(86400), Some(3600))
        );
    }

    #[test]
    fn edit_puts_v_and_lang_first_and_keeps_the_rest() {
        let text = edit(Some(r#"{"lang":"ja","theme":1}"#), |m| {
            put(m, &["relay", "mode"], Some(Value::from("per-call")))
        });
        assert_eq!(
            text,
            r#"{"v":1,"lang":"ja","relay":{"mode":"per-call"},"theme":1}"#
        );
        let text = edit(Some(&text), |m| {
            put(m, &["relay", "timeLimit"], Some(Value::from(90)))
        });
        assert_eq!(
            text,
            r#"{"v":1,"lang":"ja","relay":{"mode":"per-call","timeLimit":90},"theme":1}"#
        );
        let text = edit(Some(&text), |m| {
            put(m, &["relay", "mode"], None);
            put(m, &["relay", "timeLimit"], None);
            put(m, &["lang"], None);
        });
        assert_eq!(text, r#"{"v":1,"theme":1}"#, "empty parents go");
        assert_eq!(edit(Some(r#"{"v":3}"#), |_| {}), r#"{"v":3}"#);
        assert_eq!(edit(Some("junk"), |_| {}), r#"{"v":1}"#);
        assert_eq!(
            edit(Some(r#"{"relay":"x"}"#), |m| put(
                m,
                &["relay", "mode"],
                Some(Value::from("show-only"))
            )),
            r#"{"v":1,"relay":{"mode":"show-only"}}"#,
            "a parent that is not an object is replaced when a value goes in"
        );
        assert_eq!(
            edit(Some(r#"{"relay":"x"}"#), |m| put(
                m,
                &["relay", "mode"],
                None
            )),
            r#"{"v":1,"relay":"x"}"#,
            "and left alone when a value goes"
        );
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("op-reason-settings-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn write_read_and_remove() {
        let dir = scratch("rw");
        let file = dir.join("deeper").join(SETTINGS_FILE);
        assert_eq!(read(&file).unwrap(), None, "a missing file is no choice");
        write(&file, Lang::Ja).unwrap();
        assert_eq!(read(&file).unwrap(), Some(Lang::Ja));
        assert_eq!(fs::read_to_string(&file).unwrap(), r#"{"v":1,"lang":"ja"}"#);
        fs::write(&file, r#"{"v":3,"lang":"ja","keep":[1,2]}"#).unwrap();
        write(&file, Lang::En).unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            r#"{"v":3,"lang":"en","keep":[1,2]}"#
        );
        fs::write(&file, "broken").unwrap();
        assert_eq!(read(&file).unwrap(), None, "a broken file is no choice");
        write(&file, Lang::Ja).unwrap();
        assert_eq!(read(&file).unwrap(), Some(Lang::Ja), "and is replaced");
        let left: Vec<_> = fs::read_dir(file.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            left,
            vec![std::ffi::OsString::from(SETTINGS_FILE)],
            "no temporary file is left"
        );
        remove(&file).unwrap();
        assert_eq!(read(&file).unwrap(), None);
        remove(&file).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_tells_a_missing_file_from_one_it_cannot_read() {
        let dir = scratch("dir");
        // A directory where the file should be: not "missing", so the caller cannot tell
        fs::create_dir_all(dir.join(COPY_FILE)).unwrap();
        assert!(read(&dir.join(COPY_FILE)).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}

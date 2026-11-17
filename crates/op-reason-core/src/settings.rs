//! The language chosen in the window of `op-reason.exe` (design `2026-11-15-language-toggle-design.md` §3 and §5).
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

use serde_json::Value;

use crate::lang::Lang;

/// The file on Windows, in `%LOCALAPPDATA%\op-reason`.
pub const SETTINGS_FILE: &str = "settings.json";
/// The WSL executable's copy, in its state directory.
pub const COPY_FILE: &str = "lang.json";
/// The version of the format.
pub const VERSION: u64 = 1;

/// How many times a write tries to put the new file in place (another process may have the old one open).
const WRITE_TRIES: u32 = 5;
const WRITE_PAUSE: Duration = Duration::from_millis(20);

/// The chosen language in a file's text (a byte order mark is allowed).
pub fn parse(text: &str) -> Option<Lang> {
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    Lang::parse(value.as_object()?.get("lang")?.as_str()?)
}

/// The text of a file that says `lang`: `{"v":1,"lang":"…"}`, or, when `old` is a JSON object, its `v` (1 when it has
/// none), `lang`, then its other fields as they were.
pub fn render(old: Option<&str>, lang: Lang) -> String {
    let lang_json = Value::from(lang.code()).to_string();
    let old = old
        .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        });
    let Some(mut fields) = old else {
        return format!(r#"{{"v":{VERSION},"lang":{lang_json}}}"#);
    };
    let v = fields.remove("v").unwrap_or(Value::from(VERSION));
    fields.remove("lang");
    let mut out = format!(r#"{{"v":{v},"lang":{lang_json}"#);
    for (key, value) in &fields {
        out.push(',');
        out.push_str(&Value::from(key.as_str()).to_string());
        out.push(':');
        out.push_str(&value.to_string());
    }
    out.push('}');
    out
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

//! `op-reason --settings …` on WSL (design `2026-11-23-settings-design.md` §7.5): runs `op-reason.exe --settings` with
//! the same arguments, the terminal's input and output, and returns its exit code.
//!
//! A chosen folder given as a WSL path is passed as its Windows path (`wslpath -w`); the caller's kind goes along in
//! `OPREASON_SETTINGS_CALLER` (through `WSLENV`) for the record; a `set` of `lang` that succeeded updates this
//! executable's copy of the language.

use std::ffi::OsString;
use std::path::Path;

use op_reason_core::lang::Lang;
use op_reason_core::{messages, settings};

use crate::launch::{self, print_stderr};
use crate::runner::Lookup;
use crate::wrapper;

const FOLDER: &str = "skill.folder.";
const CALLER: &str = "OPREASON_SETTINGS_CALLER";

/// `op-reason --settings <args>`. Returns the exit code.
pub fn run(args: &[OsString], lang: Lang) -> i32 {
    let env = |k: &str| std::env::var(k).ok();
    let lookup = Lookup {
        env: &env,
        which: &launch::which,
    };
    let exe = match lookup.exe() {
        Some(p) if p.is_file() => p,
        other => {
            let looked = other.map(|p| p.display().to_string()).unwrap_or_default();
            print_stderr(&format!("{}{}\n", messages::PREFIX, no_exe(lang, &looked)));
            return 127;
        }
    };
    let args: Vec<String> = args
        .iter()
        .map(|a| translate(&a.to_string_lossy()))
        .collect();
    let claude = env("CLAUDECODE").as_deref() == Some("1")
        || env("CLAUDE_CODE_SESSION_ID").is_some_and(|v| !v.is_empty());
    let status = launch::command(&exe)
        .arg("--settings")
        .args(&args)
        .env(CALLER, if claude { "claude" } else { "human" })
        .env("WSLENV", launch::wslenv_plus(&[CALLER]))
        .status();
    let code = match status {
        Ok(s) => launch::exit_code(s),
        Err(e) => {
            print_stderr(&format!(
                "{}{}\n",
                messages::PREFIX,
                no_exe(lang, &format!("{}: {e}", exe.display()))
            ));
            return 127;
        }
    };
    if code == 0 {
        remember_lang(&args);
    }
    code
}

fn no_exe(lang: Lang, looked: &str) -> String {
    match lang {
        Lang::En => format!(
            "op-reason --settings runs op-reason.exe on Windows, which was not found ({looked}): set OPREASON_EXE or add its folder to the Windows PATH"
        ),
        Lang::Ja => format!(
            "op-reason --settings は Windows の op-reason.exe を起こしますが、見つかりませんでした ({looked})。OPREASON_EXE を設定するか、Windows の PATH にそのフォルダを足してください"
        ),
    }
}

/// A chosen folder given as a WSL path (`skill.folder./mnt/d/x`, also with `=on`) as its Windows path.
fn translate(arg: &str) -> String {
    let Some(rest) = arg.strip_prefix(FOLDER) else {
        return arg.to_owned();
    };
    if !rest.starts_with('/') {
        return arg.to_owned();
    }
    let (path, value) = match rest.rsplit_once('=') {
        Some((p, v)) if matches!(v, "on" | "off") => (p, Some(v)),
        _ => (rest, None),
    };
    let windows = launch::wslpath_w(Path::new(path));
    if windows.is_empty() {
        return arg.to_owned();
    }
    match value {
        Some(v) => format!("{FOLDER}{windows}={v}"),
        None => format!("{FOLDER}{windows}"),
    }
}

/// After a `set` that succeeded: the language it set, in this executable's copy (`default` removes it).
fn remember_lang(args: &[String]) {
    let Some(("set", rest)) = args.split_first().map(|(w, r)| (w.as_str(), r)) else {
        return;
    };
    let value = match rest {
        [name, value] if name == "lang" && !value.contains('=') => Some(value.as_str()),
        pairs => pairs.iter().find_map(|p| p.strip_prefix("lang=")),
    };
    let copy = wrapper::state_dir().join(settings::COPY_FILE);
    match value {
        Some("default") => {
            let _ = settings::remove(&copy);
        }
        Some(v) => {
            if let Some(l) = Lang::parse(v) {
                let _ = settings::write(&copy, l);
            }
        }
        None => {}
    }
}

//! The runner face: `op-reason.exe --run` takes one call from the WSL binary (the payload in `OPREASON_PAYLOAD`) and
//! does with it what `launcher.ps1` does: the mutex and its holder file, the window and the toast, `op.exe`, the
//! sign-out and the status file (design `2026-11-15-wsl-through-exe-design.md` §5, A' spec §5.1–§5.5). The WSL binary
//! writes the records; this face writes none.
//!
//! The language: the one chosen in the window (`settings.json`) first, then the payload's `lang`. The final status
//! says what `settings.json` holds as `chosenLang`, so that the WSL binary can keep its copy (design
//! `2026-11-15-language-toggle-design.md` §2 and §5).

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use op_reason_core::args::Mode;
use op_reason_core::contract::{self, Payload};
use op_reason_core::lang::Lang;
use op_reason_core::result::{self, StderrMatch};
use op_reason_core::{messages, settings, time};

use super::app::{self, How, Outcome};
use super::{console, holder, mutex, opexe, toast};
use crate::payload::{self, Status};
use crate::ui;

const EXIT_INTERRUPTED: i32 = 130;

fn say(line: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{}{line}", messages::PREFIX);
}

/// Where the status goes (nowhere when the payload has no `status`), and the settings file whose choice it reports.
struct Out {
    path: Option<PathBuf>,
    settings: Option<PathBuf>,
}

impl Out {
    /// Writes the whole status (failures are ignored, as in the launcher: the first write already left the trace),
    /// with `chosenLang` read from the settings file now (after any click in this call): left out when it cannot be
    /// read.
    fn write(&self, st: &Status) {
        if let Some(p) = &self.path {
            let mut st = st.clone();
            st.chosen_lang = self
                .settings
                .as_deref()
                .and_then(|f| settings::read(f).ok())
                .map(|chosen| chosen.map(Lang::code));
            let _ = std::fs::write(p, st.text());
        }
    }

    /// Reports an error that stops the call before `op.exe`: one stderr line, the status, the exit code.
    fn fail(&self, mut st: Status, error: String, code: i32) -> i32 {
        say(&error);
        st.error = error;
        self.write(&st);
        code
    }
}

/// Runs the call of `OPREASON_PAYLOAD`. Returns the exit code: op's, 75 (lock time-out), 127 (no `op.exe`), 1 (other
/// failures before `op.exe`), 0 for `notify`.
pub fn run() -> i32 {
    let raw = std::env::var(contract::ENV_PAYLOAD).unwrap_or_default();
    if raw.trim().is_empty() {
        say("OPREASON_PAYLOAD is empty: op-reason.exe --run takes its call from op-reason on WSL");
        return 1;
    }
    // The first write comes before anything else. When the WSL binary finds no status file afterwards, it starts the
    // launcher instead: so nothing with an effect may happen unless the file was written.
    let out = match payload::status_path(&raw) {
        Ok(path) => Out {
            path: path.map(PathBuf::from),
            settings: app::settings_file(),
        },
        Err(e) => {
            say(&e);
            return 1;
        }
    };
    if let Some(path) = &out.path
        && let Err(e) = std::fs::write(path, payload::FIRST_STATUS)
    {
        say(&format!(
            "cannot write the status file {}: {e}",
            path.display()
        ));
        return 1;
    }
    let p = match payload::parse(&raw) {
        Ok(p) => p,
        Err(e) => return out.fail(Status::default(), e, 1),
    };
    let lang = app::chosen().unwrap_or_else(|| Lang::from_code(&p.lang));
    if p.mode == "notify" {
        return notify(&p, lang, &out);
    }
    call(&p, lang, &out)
}

/// `notify`: the toast only.
fn notify(p: &Payload, lang: Lang, out: &Out) -> i32 {
    let mut st = Status::default();
    if let Some(n) = &p.notify {
        let texts = ui::build_notify(n, lang);
        st.ui = Some(texts.json());
        st.toast = toast::start(texts.toast.clone()).result();
    }
    out.write(&st);
    0
}

/// The holder file's reason and command, by the launcher's `Write-Holder`.
fn holder_reason_command(p: &Payload) -> (String, String) {
    let command = p
        .facts
        .as_ref()
        .map(|f| f.command.clone())
        .unwrap_or_default();
    let reason = match &p.facts {
        Some(f) if p.mode != "help" => f.reason.clone(),
        _ => "(no reason: help)".to_owned(),
    };
    (reason, command)
}

/// `desktop`, `help` and `service`.
fn call(p: &Payload, lang: Lang, out: &Out) -> i32 {
    let mut st = Status::default();
    let texts = if p.mode == "desktop" {
        p.facts.as_ref().map(ui::Both::build)
    } else {
        None
    };
    st.ui = texts.as_ref().map(|t| t.get(lang).json());
    let args = match payload::args(p) {
        Ok(a) => a,
        Err(e) => return out.fail(st, e, 1),
    };

    // A call that did not wait for the lock (the sender of a pipe whose receiver holds it) leaves the mutex and the
    // sessions alone: signout --all would close the receiver's session while the receiver's op.exe still runs.
    let close = p.lock != "skipped";
    let state_dir = app::state_dir();
    let held = if close {
        let wait = p.lock_wait;
        match mutex::acquire(
            contract::MUTEX_NAME,
            Duration::from_secs(wait),
            Duration::from_secs(1),
            || say(&messages::lock_waiting(lang, wait)),
            console::interrupted,
        ) {
            Ok(mutex::Acquired::Held(h)) => {
                st.lock = "held".into();
                if let Some(dir) = &state_dir {
                    let (reason, command) = holder_reason_command(p);
                    holder::write(
                        dir,
                        &time::now().hms,
                        &ui::caller_short(p.facts.as_ref(), lang),
                        &reason,
                        &command,
                    );
                }
                Some(h)
            }
            Ok(mutex::Acquired::TimedOut) => {
                st.error = "lock-timeout".into();
                st.lock = "timeout".into();
                st.holder = Some(holder::describe(state_dir.as_deref(), lang));
                out.write(&st);
                return contract::EXIT_LOCK_TIMEOUT;
            }
            Ok(mutex::Acquired::Interrupted) => {
                st.error = "interrupted".into();
                out.write(&st);
                return EXIT_INTERRUPTED;
            }
            // The mutex could not be used at all: run without it rather than not at all (as the entry face does).
            Err(_) => {
                st.lock = "failed".into();
                None
            }
        }
    } else {
        None
    };

    let op = PathBuf::from(p.op.clone().unwrap_or_default());
    let code = if op.as_os_str().is_empty() || !op.is_file() {
        let error = format!("op.exe not found: {}", op.display());
        say(&error);
        st.error = error;
        contract::EXIT_MISSING
    } else {
        let scan = p.stderr == "scan";
        let how = How {
            env: opexe::Env::AsIs,
            stderr: if scan {
                opexe::Stderr::Tee
            } else {
                opexe::Stderr::Inherit
            },
        };
        let mut o = Outcome::default();
        let code = match p.mode.as_str() {
            "desktop" => app::run_desktop(&op, &args, how, lang, texts.as_ref(), close, &mut o),
            "help" => app::run_plain(&op, &args, how, Mode::Help, lang, close, &mut o),
            _ => app::run_plain(&op, &args, how, Mode::Service, lang, false, &mut o),
        };
        st.started = o.started;
        st.exit = o.started.then_some(code);
        st.error = o.error;
        if o.result == "interrupted" {
            st.error = "interrupted".into();
        }
        st.window = o.window.unwrap_or_else(|| "skipped".into());
        st.toast = o.toast.unwrap_or_else(|| "skipped".into());
        st.others = o.others.unwrap_or(0);
        st.signout = o.signout.unwrap_or_else(|| "skipped".into());
        if scan && o.started {
            st.stderr_match = Some(result::scan_stderr(&o.stderr).map(|m| match m {
                StderrMatch::Denied => "denied",
                StderrMatch::Timeout => "timeout",
            }));
        }
        code
    };
    // As the launcher: release the lock (and remove the holder file), then write the status.
    if held.is_some()
        && let Some(dir) = &state_dir
    {
        holder::remove(dir);
    }
    drop(held);
    out.write(&st);
    code
}

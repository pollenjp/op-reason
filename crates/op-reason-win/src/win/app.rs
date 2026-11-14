//! The whole call in one process: plan (core) → op.exe → lock → `start` → window and toast → op.exe → sign-out →
//! `end`. The approval of the 1Password app binds to this process, which exits after the call.

use std::ffi::OsString;
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use op_reason_core::args::{self, Invocation, Kind, Mode, Parsed, Plan, Refusal};
use op_reason_core::contract::{self, Facts, FactsCaller, Place, Status};
use op_reason_core::lang::{self, Lang};
use op_reason_core::log::{self, Caller, Watch};
use op_reason_core::{logview, messages, place, result, time};
use serde_json::Value;
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::Globalization::GetUserDefaultUILanguage;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::UI::WindowsAndMessaging::{
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, QS_ALLINPUT,
};

use super::opexe::{self, Found, Running};
use super::{console, holder, mutex, procs, toast, window};
use crate::{cmdline, ui};

/// The version of op-reason.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The skill file for AI agents (the same file the WSL binary prints).
const SKILL: &str = include_str!("../../../../skill/SKILL.md");
/// The fish completion; `__OPREASON_CMD__` is the command name.
const COMPLETION_FISH: &str = include_str!("../../../../completions/op.fish");
/// How long op.exe gets to exit by itself after Ctrl+C before it is terminated.
const INTERRUPT_GRACE: Duration = Duration::from_secs(2);
const TICK: Duration = Duration::from_millis(250);
/// Exit codes when interrupted (the WSL numbers for SIGINT and SIGHUP).
const EXIT_INTERRUPTED: i32 = 130;
const EXIT_CONSOLE_CLOSED: i32 = 129;

fn env_nonempty(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

fn out(text: &str) {
    let _ = std::io::stdout().lock().write_all(text.as_bytes());
}

fn say(lines: &[String]) {
    let mut err = std::io::stderr().lock();
    for l in lines {
        let _ = writeln!(err, "{}{l}", messages::PREFIX);
    }
}

/// `%LOCALAPPDATA%\op-reason`
fn state_dir() -> Option<PathBuf> {
    env_nonempty("LOCALAPPDATA").map(|d| PathBuf::from(d).join("op-reason"))
}

/// `OPREASON_LANG`, then `LC_ALL`/`LC_MESSAGES`/`LANG` (the core's rule), then the user's Windows UI language (the A'
/// PowerShell entry uses the UI culture).
fn resolve_lang() -> Lang {
    if ["OPREASON_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .any(|k| env_nonempty(k).is_some())
    {
        return lang::from_env();
    }
    // SAFETY: no arguments; reads the user's setting.
    let id = unsafe { GetUserDefaultUILanguage() };
    // primary language 0x11 = LANG_JAPANESE
    if id & 0x3ff == 0x11 {
        Lang::Ja
    } else {
        Lang::En
    }
}

/// Entry point of `op-reason.exe`. Returns the exit code.
pub fn run() -> i32 {
    console::install();
    window::set_dpi_awareness();
    let mut argv = std::env::args_os();
    let argv0 = argv.next().unwrap_or_default();
    let rest: Vec<OsString> = argv.collect();
    let lang = resolve_lang();
    let name = match args::invocation(&argv0) {
        Invocation::LogViewer => return log_viewer(&rest, lang),
        Invocation::Wrapper(name) => name,
    };
    let env = |k: &str| std::env::var(k).ok();
    let (parsed, plan) = args::plan(name, &rest, &env);
    let (mode, kind) = match plan {
        Plan::Usage => {
            out(&messages::usage_full(lang, VERSION));
            return 0;
        }
        Plan::Version => {
            out(&format!("op-reason {VERSION}\n"));
            return 0;
        }
        Plan::Skill => {
            out(SKILL);
            return 0;
        }
        Plan::Completion { command, .. } => {
            out(&COMPLETION_FISH.replace("__OPREASON_CMD__", &command));
            return 0;
        }
        Plan::UsageError(message) => {
            say(&[message]);
            return contract::EXIT_USAGE;
        }
        Plan::Log(rest) => return log_viewer(&rest, lang),
        Plan::Refuse(refusal) => {
            let line = match refusal {
                Refusal::EmptyReason => messages::empty_reason(lang),
                Refusal::NoReason => messages::no_reason(lang),
            };
            return Ctx::collect(lang, &parsed).refuse(
                refusal.why(),
                args::EXIT_NO_REASON,
                true,
                &[line],
            );
        }
        Plan::Run { mode, kind } => (mode, kind),
    };
    run_call(&Ctx::collect(lang, &parsed), &parsed, mode, kind)
}

/// The log viewer (`op-reason --log`, or the exe named `op-reason-log.exe`), reading `%LOCALAPPDATA%\op-reason`.
fn log_viewer(argv: &[OsString], lang: Lang) -> i32 {
    let now = env_nonempty("OPREASON_LOG_NOW")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or_else(time::epoch_now);
    let mut o = std::io::stdout().lock();
    let mut e = std::io::stderr().lock();
    logview::run(
        argv,
        &state_dir().unwrap_or_default(),
        now,
        lang,
        &mut o,
        &mut e,
    )
}

/// Everything known about the call before it runs.
struct Ctx {
    lang: Lang,
    state_dir: Option<PathBuf>,
    id: String,
    session: Option<String>,
    caller: Caller,
    facts_caller: FactsCaller,
    is_claude: bool,
    reason: String,
    masked: Vec<String>,
    display: String,
    place: Place,
}

impl Ctx {
    fn collect(lang: Lang, parsed: &Parsed) -> Ctx {
        let masked = args::mask(&parsed.op_args_text());
        let display = args::display(&masked);
        let cwd = std::env::current_dir().unwrap_or_default();
        let cwd_text = cwd.display().to_string();
        let info = place::detect(&cwd);
        let home = env_nonempty("USERPROFILE");

        // The caller as the A' entry finds it: Claude Code when it set its variables or a claude.exe is up the
        // process tree (its pid from CLAUDE_PID, else the tree); the detail is the process chain (this process and up
        // to five parents), then "Windows Terminal" for a Windows Terminal tab.
        let lineage = procs::lineage(&procs::snapshot());
        let tree_claude = lineage
            .iter()
            .skip(1)
            .find(|(_, name)| name.eq_ignore_ascii_case("claude.exe"))
            .map(|(pid, _)| *pid);
        let is_claude = env_nonempty("CLAUDECODE").as_deref() == Some("1")
            || env_nonempty("CLAUDE_CODE_SESSION_ID").is_some()
            || tree_claude.is_some();
        let claude_pid = if is_claude {
            env_nonempty("CLAUDE_PID")
                .and_then(|p| p.parse().ok())
                .or(tree_claude)
        } else {
            None
        };
        let mut detail = Vec::new();
        if !lineage.is_empty() {
            detail.push(
                lineage
                    .iter()
                    .take(6)
                    .map(|(_, name)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(" ← "),
            );
        }
        if env_nonempty("WT_SESSION").is_some() {
            detail.push("Windows Terminal".to_owned());
        }
        let kind = if is_claude { "claude" } else { "human" };
        let session = env_nonempty("CLAUDE_CODE_SESSION_ID");
        Ctx {
            lang,
            state_dir: state_dir(),
            id: log::new_id(&time::now()),
            session: session.clone(),
            caller: Caller {
                kind: kind.to_owned(),
                claude_pid,
                herdr: None,
                cwd: cwd_text.clone(),
                repo: info.repo.clone(),
                branch: info.branch.clone(),
            },
            facts_caller: FactsCaller {
                kind: kind.to_owned(),
                pid: claude_pid,
                session,
                detail: (!detail.is_empty()).then(|| detail.join(" · ")),
            },
            is_claude,
            reason: parsed.reason.clone(),
            masked,
            display,
            place: Place {
                repo: info.repo,
                branch: info.branch,
                cwd: place::tilde(&cwd_text, home.as_deref()),
            },
        }
    }

    fn append(&self, line: String) {
        if let Some(dir) = &self.state_dir {
            log::append(dir, &line);
        }
    }

    /// Writes `refused`, prints the lines (after the short usage when `usage`) and returns the exit code.
    fn refuse(&self, why: &str, code: i32, usage: bool, lines: &[String]) -> i32 {
        let now = time::now();
        self.append(log::line(&log::Refused {
            v: 1,
            id: &self.id,
            event: "refused",
            time: &now.iso,
            epoch: now.epoch,
            session: self.session.as_deref(),
            why,
            caller: &self.caller,
            reason: &args::clip(&self.reason, 500, "…"),
            args: &self.masked,
        }));
        if usage {
            let _ = std::io::stderr()
                .lock()
                .write_all(messages::usage_short(self.lang).as_bytes());
        }
        say(lines);
        code
    }
}

/// What the end of a call found (`None` is written as null: nothing was tried).
#[derive(Default)]
struct Outcome {
    result: String,
    exit: i32,
    window: Option<String>,
    toast: Option<String>,
    others: Option<i64>,
    signout: Option<String>,
}

/// The fields an `end` line needs besides the outcome (cloned into the console-close hook).
#[derive(Clone)]
struct EndKey {
    id: String,
    session: Option<String>,
    start_epoch: i64,
}

/// The `end` line, timed now.
fn end_line(key: &EndKey, o: &Outcome) -> String {
    let now = time::now();
    log::line(&log::End {
        v: 1,
        id: &key.id,
        event: "end",
        time: &now.iso,
        epoch: now.epoch,
        session: key.session.as_deref(),
        result: &o.result,
        exit: o.exit,
        seconds: (now.epoch - key.start_epoch).max(0),
        window: o.window.clone().map(Value::from),
        toast: o.toast.clone().map(Value::from),
        others: o.others.map(Value::from),
        signout: o.signout.clone().map(Value::from),
        watch: Watch {
            start: None,
            end: None,
        },
    })
}

fn run_call(ctx: &Ctx, parsed: &Parsed, mode: Mode, kind: Kind) -> i32 {
    let lang = ctx.lang;
    let op = match opexe::find() {
        Found::At(p) => p,
        Found::Missing(tried) => {
            let m = crate::messages::no_op_exe(lang, tried.as_deref().unwrap_or(""));
            return ctx.refuse("no-op-exe", contract::EXIT_MISSING, false, &[m]);
        }
    };
    let len = cmdline::command_line_len(op.as_os_str(), &opexe::as_os_strs(&parsed.op_args));
    if len > cmdline::MAX_COMMAND_LINE {
        return ctx.refuse(
            "too-long",
            contract::EXIT_USAGE,
            false,
            &crate::messages::too_long(lang, len),
        );
    }

    let wait = env_nonempty("OPREASON_LOCK_WAIT")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(contract::LOCK_WAIT_DEFAULT);
    let on_wait = || {
        if console::stderr_is_console() {
            say(&[messages::lock_waiting(lang, wait)]);
        }
    };
    let (held, lock_field) = match mutex::acquire(
        contract::MUTEX_NAME,
        Duration::from_secs(wait),
        on_wait,
        console::interrupted,
    ) {
        Ok(mutex::Acquired::Held(h)) => (Some(h), "held"),
        Ok(mutex::Acquired::TimedOut) => {
            let lines =
                messages::mutex_timeout(lang, &holder::describe(ctx.state_dir.as_deref(), lang));
            return ctx.refuse("lock-timeout", contract::EXIT_LOCK_TIMEOUT, false, &lines);
        }
        Ok(mutex::Acquired::Interrupted) => {
            return ctx.refuse("interrupted", EXIT_INTERRUPTED, false, &[]);
        }
        // The mutex could not be used at all: run without it (two windows may then show at once) rather than block.
        Err(_) => (None, "failed"),
    };

    // Ctrl+C while the lock was being taken: stop before anything is shown or started.
    if console::interrupted() {
        drop(held);
        return ctx.refuse("interrupted", EXIT_INTERRUPTED, false, &[]);
    }

    let start = time::now();
    if held.is_some()
        && let Some(dir) = &ctx.state_dir
    {
        holder::write(
            dir,
            &start.hms,
            &messages::caller_short(lang, ctx.is_claude, ""),
            &ctx.reason,
            &ctx.display,
        );
    }
    ctx.append(log::line(&log::Start {
        v: 1,
        id: &ctx.id,
        event: "start",
        time: &start.iso,
        epoch: start.epoch,
        session: ctx.session.as_deref(),
        mode: mode.as_str(),
        kind: kind.as_str(),
        lock: lock_field,
        caller: &ctx.caller,
        reason: &args::clip(&ctx.reason, 500, "…"),
        args: &ctx.masked,
    }));

    // Closing the console ends the process a few seconds after the handler: the hook writes `end` (timed then) and
    // removes the holder file. The main flow removes the hook before writing its own `end`, under the same lock, so
    // exactly one of them writes it.
    let key = EndKey {
        id: ctx.id.clone(),
        session: ctx.session.clone(),
        start_epoch: start.epoch,
    };
    {
        let key = key.clone();
        let dir = ctx.state_dir.clone();
        let holds = held.is_some();
        console::on_close(Box::new(move || {
            if let Some(dir) = &dir {
                let o = Outcome {
                    result: "interrupted".to_owned(),
                    exit: EXIT_CONSOLE_CLOSED,
                    ..Outcome::default()
                };
                log::append(dir, &end_line(&key, &o));
                if holds {
                    holder::remove(dir);
                }
            }
        }));
    }

    let facts = Facts {
        time: start.ymd_hms.clone(),
        caller: ctx.facts_caller.clone(),
        herdr: None,
        place: ctx.place.clone(),
        reason: args::clip(&ctx.reason, 200, "…"),
        command: args::clip(&ctx.display, 300, "…"),
        notes: Vec::new(),
    };
    let mut o = Outcome::default();
    let code = match mode {
        Mode::Desktop => run_desktop(&op, &parsed.op_args, kind, lang, &facts, &mut o),
        Mode::Help => run_plain(&op, &parsed.op_args, kind, mode, lang, true, &mut o),
        Mode::Service => run_plain(&op, &parsed.op_args, kind, mode, lang, false, &mut o),
    };
    console::without_close_hook(|| {
        ctx.append(end_line(&key, &o));
        if held.is_some()
            && let Some(dir) = &ctx.state_dir
        {
            holder::remove(dir);
        }
    });
    drop(held);
    code
}

/// Waits for op.exe. `tick` runs every 250 ms (and the message queue is served when `with_messages`).
/// After Ctrl+C, op.exe gets two seconds to exit by itself, then it is terminated. Returns op's exit code.
fn wait_op(running: &mut Running, with_messages: bool, mut tick: impl FnMut()) -> i32 {
    let handle = HANDLE(running.child.as_raw_handle());
    let mut interrupted_at: Option<Instant> = None;
    let mut last_tick = Instant::now();
    loop {
        // SAFETY: the process handle stays valid while `running.child` lives.
        let r = unsafe {
            if with_messages {
                MsgWaitForMultipleObjectsEx(
                    Some(&[handle]),
                    TICK.as_millis() as u32,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                )
            } else {
                WaitForSingleObject(handle, TICK.as_millis() as u32)
            }
        };
        if r == WAIT_OBJECT_0 {
            break;
        }
        if with_messages {
            window::pump();
        }
        if last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            tick();
        }
        if console::interrupted() {
            let at = *interrupted_at.get_or_insert_with(Instant::now);
            if at.elapsed() >= INTERRUPT_GRACE {
                let _ = running.child.kill();
            }
        }
    }
    running
        .child
        .wait()
        .ok()
        .and_then(|s| s.code())
        .unwrap_or(1)
}

/// The `result` and the exit code, once op.exe has run.
fn finish(o: &mut Outcome, code: i32, mode: Mode, stderr_copy: &[u8]) -> i32 {
    if console::interrupted() {
        o.result = "interrupted".to_owned();
        o.exit = EXIT_INTERRUPTED;
        return EXIT_INTERRUPTED;
    }
    let started = Status {
        started: true,
        ..Status::default()
    };
    o.result = result::classify(Some(&started), code, mode, stderr_copy).to_owned();
    o.exit = code;
    code
}

/// op.exe could not be started after `start` was written.
fn start_failed(lang: Lang, err: &std::io::Error, o: &mut Outcome) -> i32 {
    say(&[crate::messages::start_failed(lang, &err.to_string())]);
    o.result = result::classify(None, 1, Mode::Desktop, b"").to_owned();
    o.exit = 1;
    1
}

/// Desktop mode: window, toast, op.exe, then sign-out.
fn run_desktop(
    op: &Path,
    args: &[OsString],
    kind: Kind,
    lang: Lang,
    facts: &Facts,
    o: &mut Outcome,
) -> i32 {
    let texts = ui::build(facts, lang);
    let image = opexe::image_name(op);
    // Before op.exe starts, any op.exe is someone else's (but not this exe, if it was installed as op.exe).
    let me = std::env::current_exe().ok();
    let own = std::process::id();
    let mut others = procs::count_named(&image, &[own], me.as_deref()) as i64;
    let win = match window::Window::create(&texts, others > 0) {
        Ok(w) => {
            w.show();
            // The first frame is drawn while the queue is served; `shown` means it was presented.
            let t0 = Instant::now();
            loop {
                window::pump();
                if w.painted() || t0.elapsed() >= Duration::from_millis(500) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            o.window = Some(if w.painted() {
                "shown".to_owned()
            } else {
                "failed: the window was not painted".to_owned()
            });
            Some(w)
        }
        Err(e) => {
            o.window = Some(format!("failed: {}", opexe::short(&e.message(), 120)));
            None
        }
    };
    let pending = toast::start(texts.toast.clone());
    if console::interrupted() {
        // Ctrl+C before op.exe started: no prompt is raised
        drop(win);
        o.toast = Some(pending.result());
        o.others = Some(others);
        o.result = "interrupted".to_owned();
        o.exit = EXIT_INTERRUPTED;
        return EXIT_INTERRUPTED;
    }
    let mut running = match opexe::start(op, args, kind) {
        Ok(r) => r,
        Err(e) => {
            drop(win);
            o.toast = Some(pending.result());
            o.others = Some(others);
            return start_failed(lang, &e, o);
        }
    };
    let pid = running.child.id();
    if let Some(w) = win.as_ref() {
        w.watch(HANDLE(running.child.as_raw_handle()));
    }
    let code = wait_op(&mut running, win.is_some(), || {
        let n = procs::count_named(&image, &[own, pid], me.as_deref()) as i64;
        others = others.max(n);
        if n > 0
            && let Some(w) = win.as_ref()
        {
            w.set_others(true);
        }
    });
    drop(win);
    let stderr_copy = running.stderr_copy();
    o.signout = Some(opexe::close_session(op));
    o.toast = Some(pending.result());
    o.others = Some(others);
    finish(o, code, Mode::Desktop, &stderr_copy)
}

/// Help and service modes: op.exe without a window. Help closes a session in case one was opened.
fn run_plain(
    op: &Path,
    args: &[OsString],
    kind: Kind,
    mode: Mode,
    lang: Lang,
    close: bool,
    o: &mut Outcome,
) -> i32 {
    o.window = Some("skipped".to_owned());
    o.toast = Some("skipped".to_owned());
    o.others = Some(0);
    if console::interrupted() {
        o.result = "interrupted".to_owned();
        o.exit = EXIT_INTERRUPTED;
        return EXIT_INTERRUPTED;
    }
    let mut running = match opexe::start(op, args, kind) {
        Ok(r) => r,
        Err(e) => return start_failed(lang, &e, o),
    };
    let code = wait_op(&mut running, false, || {});
    let stderr_copy = running.stderr_copy();
    o.signout = Some(if close {
        opexe::close_session(op)
    } else {
        "skipped".to_owned()
    });
    finish(o, code, mode, &stderr_copy)
}

//! One call of the wrapper (design section 6.1): plan the arguments, collect the caller, take the lock, watch for a
//! shared approval, start a fresh runner (`op-reason.exe --run`, or the launcher as the fallback) for `op.exe`, and log
//! the call.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use op_reason_core::args::{self, Kind, Mode, Name, Parsed, Plan, Refusal};
use op_reason_core::contract::{
    self, Facts, FactsCaller, Herdr, Note, Notify, Payload, Place, Status,
};
use op_reason_core::lang::{self, Lang};
use op_reason_core::log::{self, Caller, Fallback};
use op_reason_core::messages::FallbackWhy;
use op_reason_core::{base64, logview, messages, place, refs, result, settings, time};

use crate::launch::{self, print_stderr, print_stdout};
use crate::lock::{self, Holder, Outcome};
use crate::runner::{self, Missing, Runner, RunnerPlan};
use crate::{procfs, signals};

/// The version of op-reason.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The skill file for AI agents.
const SKILL: &str = include_str!("../../../skill/SKILL.md");
/// The fish completion; `__OPREASON_CMD__` is the command name.
const COMPLETION_FISH: &str = include_str!("../../../completions/op.fish");
/// How much of `op`'s stderr is kept to classify the result.
const STDERR_TAIL: usize = 256 * 1024;
/// Leftovers of calls killed by SIGKILL older than this are removed.
const STALE_RUN: Duration = Duration::from_secs(3600);
/// How long the shared approval toast waits for `op-reason.exe`'s first write before it gives up on a fallback.
const NOTIFY_WAKE: Duration = Duration::from_secs(5);

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn say(lines: &[String]) {
    for line in lines {
        print_stderr(&format!("{}{line}\n", messages::PREFIX));
    }
}

/// `${XDG_STATE_HOME:-~/.local/state}/op-reason`
pub fn state_dir() -> PathBuf {
    let base = env_nonempty("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env_nonempty("HOME").unwrap_or_else(|| "/".to_string()))
                .join(".local/state")
        });
    base.join("op-reason")
}

/// The language of this invocation: the copy of the language chosen in the window (`<state>/lang.json`), then
/// `OPREASON_LANG`, then English (design `2026-11-15-language-toggle-design.md` §2).
pub(crate) fn wsl_lang() -> Lang {
    lang::pick(
        settings::read(&state_dir().join(settings::COPY_FILE))
            .ok()
            .flatten(),
    )
}

/// Keeps the copy of the language chosen in the window in step with what the runner said in its status: `en` or `ja`
/// → written (when it changes), null → removed, nothing (an older runner) or anything else → left as it is (design
/// §5).
fn remember_choice(state_dir: &Path, status: Option<&Status>) {
    let path = state_dir.join(settings::COPY_FILE);
    match status.and_then(Status::chosen) {
        Some(Some(chosen)) => {
            if settings::read(&path).ok().flatten() != Some(chosen) {
                let _ = settings::write(&path, chosen);
            }
        }
        Some(None) => {
            let _ = settings::remove(&path);
        }
        None => {}
    }
}

/// The fish completion for `command`.
pub fn completion(command: &str) -> String {
    COMPLETION_FISH.replace("__OPREASON_CMD__", command)
}

/// What every record of one call shares.
struct Call {
    lang: Lang,
    state_dir: PathBuf,
    id: String,
    session: Option<String>,
    caller: Caller,
    is_claude: bool,
    herdr_label: String,
    reason: String,
    masked: Vec<String>,
    display: String,
}

impl Call {
    fn log_refused(&self, why: &str) {
        let now = time::now();
        log::append(
            &self.state_dir,
            &log::line(&log::Refused {
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
            }),
        );
    }

    /// Logs a refusal, prints it (with the short usage when asked) and returns the exit code.
    fn refuse(&self, why: &str, code: i32, usage: bool, lines: &[String]) -> i32 {
        self.log_refused(why);
        if usage {
            print_stderr(&messages::usage_short(self.lang));
        }
        say(lines);
        code
    }

    #[allow(clippy::too_many_arguments)]
    fn write_end(
        &self,
        start_epoch: i64,
        result: &str,
        exit: i32,
        status: Option<&Status>,
        watch_start: Option<&str>,
        watch_end: Option<&str>,
        runner: Option<&str>,
        fallback: Option<&Fallback>,
    ) {
        let now = time::now();
        log::append(
            &self.state_dir,
            &log::line(&log::End {
                v: 1,
                id: &self.id,
                event: "end",
                time: &now.iso,
                epoch: now.epoch,
                session: self.session.as_deref(),
                result,
                exit,
                seconds: (now.epoch - start_epoch).max(0),
                window: status.and_then(|s| s.window.clone()),
                toast: status.and_then(|s| s.toast.clone()),
                others: status.and_then(|s| s.others.clone()),
                signout: status.and_then(|s| s.signout.clone()),
                watch: log::Watch {
                    start: watch_start,
                    end: watch_end,
                },
                runner,
                fallback,
            }),
        );
    }

    fn caller_short(&self) -> String {
        messages::caller_short(self.lang, self.is_claude, &self.herdr_label)
    }
}

/// What the signal handler needs to log an interrupted call and clean up.
#[derive(Default)]
struct Progress {
    started_epoch: Option<i64>,
    watch_start: Option<String>,
    run_dir: Option<PathBuf>,
    wrote_current: bool,
    /// The runner while it runs (TERM and HUP are forwarded to it; on WSL this is the interop process that stands for
    /// the Windows program).
    child: Option<u32>,
    /// The runner started last (`exe` or `launcher`) and why `auto` moved to the launcher, for the `end` record.
    runner: Option<&'static str>,
    fallback: Option<Fallback>,
    /// The `end` record is written: a late signal only exits.
    ended: bool,
}

fn clean_up(state_dir: &Path, progress: &Progress) {
    if let Some(dir) = &progress.run_dir {
        let _ = fs::remove_dir_all(dir);
    }
    // Only our own holder record: the lock may already belong to the next call
    if progress.wrote_current
        && Holder::read(state_dir).and_then(|h| h.pid) == Some(std::process::id())
    {
        let _ = fs::remove_file(state_dir.join("current.json"));
    }
}

/// Removes leftovers on every return path after the run directory exists.
struct CleanUp {
    state_dir: PathBuf,
    progress: Arc<Mutex<Progress>>,
}

impl Drop for CleanUp {
    fn drop(&mut self) {
        if let Ok(p) = self.progress.lock() {
            clean_up(&self.state_dir, &p);
        }
    }
}

/// Runs the wrapper for one invocation and returns the exit code.
pub fn run(name: Name, argv: Vec<OsString>) -> i32 {
    let lang = wsl_lang();
    let env = |key: &str| std::env::var(key).ok();
    let (parsed, plan) = args::plan(name, &argv, &env);
    let (mode, kind) = match plan {
        Plan::Usage => {
            print_stdout(messages::usage_full(lang, VERSION));
            return 0;
        }
        Plan::Version => {
            print_stdout(format!("op-reason {VERSION}\n"));
            return 0;
        }
        Plan::Skill => {
            print_stdout(SKILL);
            return 0;
        }
        Plan::Completion { command, .. } => {
            print_stdout(completion(&command));
            return 0;
        }
        Plan::UsageError(got) => {
            say(&[messages::completion_usage(lang, &got)]);
            return contract::EXIT_USAGE;
        }
        Plan::Log(rest) => return log_viewer(&rest),
        Plan::Refuse(_) | Plan::Run { .. } => match plan {
            Plan::Run { mode, kind } => (Some(mode), Some(kind)),
            _ => (None, None),
        },
    };
    let call = Arc::new(collect(lang, &parsed));
    let (Some(mode), Some(kind)) = (mode, kind) else {
        let Plan::Refuse(refusal) = plan else {
            unreachable!("only refusals are left")
        };
        let line = match refusal {
            Refusal::EmptyReason => messages::empty_reason(lang),
            Refusal::NoReason => messages::no_reason(lang),
        };
        return call.refuse(refusal.why(), args::EXIT_NO_REASON, true, &[line]);
    };
    run_call(call, &parsed, mode, kind)
}

/// The log viewer (`op-reason-log`, `op-reason --log`).
pub fn log_viewer(argv: &[OsString]) -> i32 {
    let now = env_nonempty("OPREASON_LOG_NOW")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or_else(time::epoch_now);
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    logview::run(argv, &state_dir(), now, wsl_lang(), &mut out, &mut err)
}

/// The caller, the place and the masked arguments.
fn collect(lang: Lang, parsed: &Parsed) -> Call {
    let op_args = parsed.op_args_text();
    let masked = args::mask(&op_args);
    let display = args::display(&masked);

    let proc_root = env_nonempty("OPREASON_PROC_ROOT").unwrap_or_else(|| "/proc".to_string());
    let start = env_nonempty("OPREASON_PROC_START")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or_else(std::process::id);
    let claude_pid = procfs::find_claude(Path::new(&proc_root), start);

    let mut herdr_label = String::new();
    let herdr = if std::env::var("HERDR_ENV").as_deref() == Ok("1") {
        let workspace = std::env::var("HERDR_WORKSPACE_ID").unwrap_or_default();
        if !workspace.is_empty() && launch::which("herdr").is_some() {
            herdr_label = herdr_workspace_label(&workspace, Duration::from_secs(2))
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
        }
        Some(Herdr {
            workspace,
            label: herdr_label.clone(),
            pane: std::env::var("HERDR_PANE_ID").unwrap_or_default(),
        })
    } else {
        None
    };

    let cwd = current_dir_text();
    let info = place::detect(Path::new(&cwd));
    let caller = Caller {
        kind: if claude_pid.is_some() {
            "claude"
        } else {
            "human"
        }
        .to_string(),
        claude_pid,
        herdr,
        cwd,
        repo: info.repo,
        branch: info.branch,
    };
    let now = time::now();
    Call {
        lang,
        state_dir: state_dir(),
        id: log::new_id(&now),
        session: env_nonempty("CLAUDE_CODE_SESSION_ID"),
        is_claude: claude_pid.is_some(),
        caller,
        herdr_label,
        reason: parsed.reason.clone(),
        masked,
        display,
    }
}

/// The current directory as the shell shows it: `$PWD` when it is the same directory (keeps symlinks), otherwise the
/// real path, otherwise `$PWD` (the directory may have been removed), otherwise `?`.
pub(crate) fn current_dir_text() -> String {
    use std::os::unix::fs::MetadataExt;
    let real = std::env::current_dir().ok();
    let pwd = env_nonempty("PWD");
    match (pwd, real) {
        (Some(p), Some(r)) => {
            let same = match (fs::metadata(&p), fs::metadata(&r)) {
                (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
                _ => false,
            };
            if same { p } else { r.display().to_string() }
        }
        (None, Some(r)) => r.display().to_string(),
        (Some(p), None) => p,
        (None, None) => "?".to_string(),
    }
}

/// `herdr workspace get <id>` → `.result.workspace.label` (within `limit`; empty on any failure).
pub(crate) fn herdr_workspace_label(workspace: &str, limit: Duration) -> String {
    launch::output_timeout(
        launch::command("herdr").args(["workspace", "get", workspace]),
        limit,
    )
    .filter(|(s, _)| s.success())
    .and_then(|(_, out)| serde_json::from_slice::<serde_json::Value>(&out).ok())
    .and_then(|v| {
        v.pointer("/result/workspace/label")
            .and_then(|l| l.as_str())
            .map(str::to_string)
    })
    .unwrap_or_default()
}

fn lock_wait() -> u64 {
    env_nonempty("OPREASON_LOCK_WAIT")
        .filter(|v| v.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|v| v.parse().ok())
        .unwrap_or(contract::LOCK_WAIT_DEFAULT)
}

fn run_call(call: Arc<Call>, parsed: &Parsed, mode: Mode, kind: Kind) -> i32 {
    let lang = call.lang;

    // Tools
    let op_exe = match env_nonempty("OPREASON_OP_EXE") {
        Some(p) => Some(PathBuf::from(p)),
        None => launch::which("op.exe"),
    };
    let op_exe = match op_exe {
        Some(p) if p.is_file() => p,
        other => {
            let shown = other.map(|p| p.display().to_string()).unwrap_or_default();
            return call.refuse(
                "no-op-exe",
                contract::EXIT_MISSING,
                false,
                &[messages::no_op_exe(lang, &shown)],
            );
        }
    };
    // The runner (design 2026-11-15-wsl-through-exe-design.md §4)
    let lookup_env = |k: &str| std::env::var(k).ok();
    let lookup_which = |k: &str| launch::which(k);
    let lookup = runner::Lookup {
        env: &lookup_env,
        which: &lookup_which,
    };
    let choice = runner::Choice::parse(env_nonempty(runner::ENV_RUNNER).as_deref());
    let plan = match lookup.plan(choice) {
        Ok(plan) => plan,
        Err(missing) => {
            return call.refuse(
                missing.why(),
                contract::EXIT_MISSING,
                false,
                &[missing_line(lang, &missing)],
            );
        }
    };
    if launch::which("wslpath").is_none() {
        return call.refuse(
            "no-wslpath",
            contract::EXIT_MISSING,
            false,
            &[messages::no_wslpath(lang)],
        );
    }

    // Arguments travel in a Windows environment variable
    let b64_len = args::args_b64_len(&parsed.op_args);
    if b64_len > args::ARGS_B64_MAX {
        return call.refuse(
            "too-long",
            contract::EXIT_USAGE,
            false,
            &messages::too_long(lang, b64_len),
        );
    }

    // From here a signal is logged (interrupted) and the leftovers are removed
    let progress = Arc::new(Mutex::new(Progress::default()));
    {
        let call = Arc::clone(&call);
        let progress = Arc::clone(&progress);
        signals::install(move |code| {
            let p = progress.lock().unwrap_or_else(|e| e.into_inner());
            // Stop the runner too when only this process got TERM or HUP (a Ctrl+C reaches the whole group)
            if let Some(pid) = p.child
                && code != 130
            {
                // SAFETY: kill only sends a signal; the pid is our child, not reaped yet.
                unsafe {
                    libc::kill(pid as libc::pid_t, code - 128);
                }
            }
            if !p.ended {
                match p.started_epoch {
                    Some(epoch) => call.write_end(
                        epoch,
                        "interrupted",
                        code,
                        None,
                        p.watch_start.as_deref(),
                        None,
                        p.runner,
                        p.fallback.as_ref(),
                    ),
                    None => call.log_refused("interrupted"),
                }
            }
            clean_up(&call.state_dir, &p);
            std::process::exit(code);
        });
    }
    let run_dir = call.state_dir.join("run").join(&call.id);
    let _ = log::ensure_dir(&call.state_dir);
    let _ = log::ensure_dir(&run_dir);
    progress.lock().unwrap().run_dir = Some(run_dir.clone());
    // Declared before the clean-up guard so that it is dropped after it: current.json goes before the lock is released
    let mut _lock_file: Option<File> = None;
    let _cleanup = CleanUp {
        state_dir: call.state_dir.clone(),
        progress: Arc::clone(&progress),
    };

    // A pipe from another op-reason: read stdin before waiting for the lock (design 6.4). op inject without -i reads its
    // template from stdin: read it here too (not from a terminal), to list its references; the runner gets the same
    // bytes (design 2026-11-17-refs-in-window-design.md §2)
    let op_text = parsed.op_args_text();
    let template_on_stdin = mode == Mode::Desktop && refs::plan(&op_text).stdin;
    let mut drained = ((template_on_stdin && !std::io::stdin().is_terminal())
        || procfs::op_upstream())
    .then(lock::drain_stdin);

    // Lock
    let wait = lock_wait();
    let acquired = lock::acquire(
        &call.state_dir,
        Duration::from_secs(wait),
        lang,
        &mut drained,
        || {
            if signals::isatty(2) {
                say(&[messages::lock_waiting(lang, wait)]);
            }
        },
    );
    let mut holder_reason = None;
    let mut remaining = wait;
    let lock_mode = match acquired {
        Ok((Outcome::Held(f), left)) => {
            _lock_file = Some(f);
            remaining = left;
            "held"
        }
        Ok((Outcome::Skipped { holder_reason: r }, left)) => {
            holder_reason = Some(r);
            remaining = left;
            "skipped"
        }
        Ok((Outcome::Timeout { holder }, _)) => {
            return call.refuse(
                "lock-timeout",
                contract::EXIT_LOCK_TIMEOUT,
                false,
                &messages::lock_timeout(lang, wait, &holder),
            );
        }
        // The lock file cannot be opened: run without it rather than not at all
        Err(_) => "held",
    };
    let held = lock_mode == "held";

    remove_stale_runs(&call.state_dir.join("run"), &run_dir);
    if held {
        let reason = if mode == Mode::Help {
            match lang {
                Lang::En => "(no reason: help)".to_string(),
                Lang::Ja => "(理由なし: help)".to_string(),
            }
        } else {
            args::clip(&call.reason, 100, "…")
        };
        let holder = Holder {
            time: time::now().hms,
            caller: call.caller_short(),
            reason,
            command: args::clip(&call.display, 100, "…"),
            pid: Some(std::process::id()),
        };
        if holder.write(&call.state_dir) {
            progress.lock().unwrap().wrote_current = true;
        }
    }

    // Shared approval watchdog (the lock holder runs it when this call skipped the lock)
    let watch_start = held.then(|| watch_shared(&call, &op_exe, &plan, &lookup, &run_dir, "start"));
    progress.lock().unwrap().watch_start = watch_start.clone();

    // Payload
    let start = time::now();
    let status_file = run_dir.join("status.json");
    let mut notes = Vec::new();
    if watch_start.as_deref() == Some("cleared") {
        notes.push(Note {
            kind: "shared-cleared".into(),
            holder_reason: None,
        });
    }
    if let Some(r) = &holder_reason {
        notes.push(Note {
            kind: "lock-skipped".into(),
            holder_reason: Some(args::clip(r, 60, "…")),
        });
    }
    // The references the call reads (files relative to the current directory, the template from the drained stdin)
    let collected = (mode == Mode::Desktop).then(|| {
        let stdin = drained
            .as_deref()
            .filter(|_| template_on_stdin)
            .map(String::from_utf8_lossy);
        refs::collect(&op_text, &refs::read_file, stdin.as_deref())
    });
    let log_refs = collected.as_ref().and_then(refs::Collected::log);
    let home = env_nonempty("HOME");
    let facts = (mode == Mode::Desktop).then(|| Facts {
        time: start.ymd_hms.clone(),
        caller: FactsCaller {
            kind: call.caller.kind.clone(),
            pid: call.caller.claude_pid,
            session: call.session.clone(),
            detail: None,
        },
        herdr: call.caller.herdr.clone(),
        place: Place {
            repo: call.caller.repo.clone(),
            branch: call.caller.branch.clone(),
            cwd: place::tilde(&call.caller.cwd, home.as_deref()),
        },
        reason: args::clip(&call.reason, 200, "…"),
        command: args::clip(&call.display, 300, "…"),
        notes,
        refs: collected.as_ref().and_then(refs::Collected::facts),
    });
    // The runner needs both Windows paths: with an empty status path it could not leave the trace that keeps the
    // fallback from running op.exe a second time
    let op_win = launch::wslpath_w(&op_exe);
    let status_win = launch::wslpath_w(&status_file);
    for (linux, windows) in [(&op_exe, &op_win), (&status_file, &status_win)] {
        if windows.is_empty() {
            return call.refuse(
                "no-wslpath",
                contract::EXIT_MISSING,
                false,
                &[messages::wslpath_failed(lang, &linux.display().to_string())],
            );
        }
    }
    let payload = Payload {
        v: contract::CONTRACT_VERSION,
        mode: mode.as_str().to_string(),
        op: Some(op_win),
        args64: parsed
            .op_args
            .iter()
            .map(|a| base64::encode(&args::os_bytes(a)))
            .collect(),
        status: Some(status_win),
        lang: lang.code().to_string(),
        lock: lock_mode.to_string(),
        lock_wait: remaining.max(1),
        stderr: "inherit".to_string(),
        facts,
        notify: None,
    };
    let payload_json = contract::payload_json(&payload);
    if payload_json.len() > contract::PAYLOAD_MAX {
        return call.refuse(
            "too-long",
            contract::EXIT_USAGE,
            false,
            &messages::payload_too_long(lang, payload_json.len()),
        );
    }

    // start (marked first: from here on a signal logs an end, never a refusal)
    progress.lock().unwrap().started_epoch = Some(start.epoch);
    log::append(
        &call.state_dir,
        &log::line(&log::Start {
            v: 1,
            id: &call.id,
            event: "start",
            time: &start.iso,
            epoch: start.epoch,
            session: call.session.as_deref(),
            mode: mode.as_str(),
            kind: kind.as_str(),
            lock: lock_mode,
            caller: &call.caller,
            reason: &args::clip(&call.reason, 500, "…"),
            args: &call.masked,
            refs: log_refs.as_ref(),
        }),
    );

    // The runner, and in auto mode the launcher when op-reason.exe left no status file (op.exe never runs twice: the
    // exe writes the status file before anything else). Only when the run directory is still there: without it, a
    // missing status file proves nothing.
    let mut fallback: Option<Fallback> = None;
    let mut used = match &plan {
        RunnerPlan::Exe { exe, .. } => Runner::Exe(exe.clone()),
        RunnerPlan::Launcher {
            launcher,
            not_found,
        } => {
            if let Some(path) = not_found {
                say(&[messages::runner_fallback(
                    lang,
                    FallbackWhy::NotFound(&path.display().to_string()),
                )]);
                fallback = Some(Fallback {
                    why: "not-found",
                    exit: None,
                });
            }
            Runner::Launcher(launcher.clone())
        }
    };
    let set_runner = |used: &Runner, fallback: &Option<Fallback>| {
        let mut p = progress.lock().unwrap();
        p.runner = Some(used.name());
        p.fallback = fallback.clone();
    };
    set_runner(&used, &fallback);
    let mut outcome = start_runner(&used, &payload_json, kind, drained.as_deref(), &progress);
    if let RunnerPlan::Exe {
        folder,
        fallback: true,
        ..
    } = &plan
        && no_trace(&run_dir, &status_file)
    {
        let (spawn_error, record) = match &outcome {
            Err(e) => (
                Some(e.to_string()),
                Fallback {
                    why: "not-started",
                    exit: None,
                },
            ),
            Ok((rc, _)) => (
                None,
                Fallback {
                    why: "no-status",
                    exit: Some(*rc),
                },
            ),
        };
        let why = match &spawn_error {
            Some(e) => FallbackWhy::NotStarted(e),
            None => FallbackWhy::NoStatus(record.exit.unwrap_or(1)),
        };
        match lookup.launcher(Some(folder)) {
            Ok(launcher) => {
                say(&[messages::runner_fallback(lang, why)]);
                used = Runner::Launcher(launcher);
                fallback = Some(record);
                set_runner(&used, &fallback);
                outcome = start_runner(&used, &payload_json, kind, drained.as_deref(), &progress);
            }
            Err(missing) => {
                say(&[
                    messages::no_fallback(lang, why),
                    missing_line(lang, &missing),
                ]);
                outcome = Ok((contract::EXIT_MISSING, Vec::new()));
            }
        }
    }
    // exe mode: say why nothing ran when the exe left no status file (it never falls back)
    if let (
        RunnerPlan::Exe {
            fallback: false, ..
        },
        Ok((rc, _)),
    ) = (&plan, &outcome)
        && no_trace(&run_dir, &status_file)
    {
        say(&[messages::exe_no_status(lang, *rc)]);
    }
    let (op_rc, stderr_tail) = match outcome {
        Ok(done) => done,
        Err(e) => {
            say(&[messages::cannot_start(
                lang,
                &used.program().display().to_string(),
                &e.to_string(),
            )]);
            (contract::EXIT_MISSING, Vec::new())
        }
    };
    let status = fs::read_to_string(&status_file)
        .ok()
        .and_then(|t| Status::parse(&t));
    remember_choice(&call.state_dir, status.as_ref());
    let result = result::classify(status.as_ref(), op_rc, mode, &stderr_tail);
    if result == "lock-timeout" {
        let holder = status
            .as_ref()
            .and_then(Status::holder_text)
            .unwrap_or_else(|| messages::unknown(lang).to_string());
        say(&messages::mutex_timeout(lang, &holder));
    }

    let watch_end = held.then(|| watch_shared(&call, &op_exe, &plan, &lookup, &run_dir, "end"));
    let mut p = progress.lock().unwrap();
    call.write_end(
        start.epoch,
        result,
        op_rc,
        status.as_ref(),
        watch_start.as_deref(),
        watch_end.as_deref(),
        Some(used.name()),
        fallback.as_ref(),
    );
    p.ended = true;
    op_rc
}

/// `op-reason.exe` left no trace: the run directory is there (no other call removed it) and the status file is not.
fn no_trace(run_dir: &Path, status_file: &Path) -> bool {
    run_dir.is_dir() && fs::symlink_metadata(status_file).is_err()
}

/// The message for a missing runner part.
fn missing_line(lang: Lang, missing: &Missing) -> String {
    match missing {
        Missing::Exe(tried) => messages::no_exe(lang, tried),
        Missing::Launcher(tried) => messages::no_launcher(lang, tried),
        Missing::PowerShell(name) => messages::no_powershell(lang, name),
    }
}

/// The payload and `WSLENV` for a runner: `OPREASON_PAYLOAD`, plus the token variables with `/w` in service mode;
/// desktop and help remove the token variables from the environment.
fn runner_env(cmd: &mut std::process::Command, payload_json: &str, kind: Kind) {
    cmd.env(contract::ENV_PAYLOAD, payload_json);
    let base = contract::ENV_PAYLOAD;
    match kind {
        Kind::ServiceAccount => {
            cmd.env("OP_BIOMETRIC_UNLOCK_ENABLED", "false").env(
                "WSLENV",
                launch::wslenv_plus(&[
                    base,
                    "OP_SERVICE_ACCOUNT_TOKEN/w",
                    "OP_BIOMETRIC_UNLOCK_ENABLED/w",
                ]),
            );
        }
        Kind::Connect => {
            cmd.env_remove("OP_SERVICE_ACCOUNT_TOKEN")
                .env("OP_BIOMETRIC_UNLOCK_ENABLED", "false")
                .env(
                    "WSLENV",
                    launch::wslenv_plus(&[
                        base,
                        "OP_CONNECT_HOST/w",
                        "OP_CONNECT_TOKEN/w",
                        "OP_BIOMETRIC_UNLOCK_ENABLED/w",
                    ]),
                );
        }
        Kind::Desktop | Kind::Help => {
            cmd.env_remove("OP_SERVICE_ACCOUNT_TOKEN")
                .env_remove("OP_CONNECT_HOST")
                .env_remove("OP_CONNECT_TOKEN")
                .env("WSLENV", launch::wslenv_plus(&[base]));
        }
    }
}

/// Starts a runner and waits for it. stdin is ours (or the drained copy, given again to a fallback), stdout is ours,
/// stderr is relayed to ours while its tail is kept. Returns the runner's exit code (`op`'s) and the tail of its
/// stderr.
fn start_runner(
    runner: &Runner,
    payload_json: &str,
    kind: Kind,
    drained: Option<&[u8]>,
    progress: &Arc<Mutex<Progress>>,
) -> std::io::Result<(i32, Vec<u8>)> {
    let mut cmd = runner.command();
    runner_env(&mut cmd, payload_json, kind);
    cmd.stdout(Stdio::inherit()).stderr(Stdio::piped());
    cmd.stdin(if drained.is_some() {
        Stdio::piped()
    } else {
        Stdio::inherit()
    });
    let mut child = cmd.spawn()?;
    progress.lock().unwrap().child = Some(child.id());
    let writer = match (drained, child.stdin.take()) {
        (Some(data), Some(mut stdin)) => {
            let data = data.to_vec();
            Some(thread::spawn(move || {
                let _ = stdin.write_all(&data);
            }))
        }
        _ => None,
    };
    let relay = child.stderr.take().map(|mut err| {
        thread::spawn(move || {
            let mut tail: Vec<u8> = Vec::new();
            let mut buf = [0u8; 8192];
            let mut out = std::io::stderr();
            loop {
                match err.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = out.write_all(&buf[..n]);
                        tail.extend_from_slice(&buf[..n]);
                        if tail.len() > 2 * STDERR_TAIL {
                            tail.drain(..tail.len() - STDERR_TAIL);
                        }
                    }
                }
            }
            tail
        })
    });
    let rc = child.wait().map(launch::exit_code).unwrap_or(1);
    progress.lock().unwrap().child = None;
    if let Some(w) = writer {
        let _ = w.join();
    }
    let tail = relay.and_then(|r| r.join().ok()).unwrap_or_default();
    Ok((rc, tail))
}

/// Removes leftovers of calls killed by SIGKILL (`<state>/run/*` older than an hour), but not the directory of a call
/// whose process still runs (an `op run` can take longer than an hour; its status file must stay).
fn remove_stale_runs(run_root: &Path, mine: &Path) {
    let Ok(entries) = fs::read_dir(run_root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == mine {
            continue;
        }
        // The id is YYYYmmddTHHMMSS-<pid>-<random>
        let alive = entry
            .file_name()
            .to_str()
            .and_then(|name| name.split('-').nth(1))
            .and_then(|pid| pid.parse::<u32>().ok())
            .is_some_and(|pid| Path::new("/proc").join(pid.to_string()).exists());
        if alive {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > STALE_RUN);
        if old {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// The shared approval watchdog (design 6.5): `op.exe whoami` straight from WSL; when it succeeds, sign out, check
/// again, log and notify. Returns `none`, `cleared` or `error`.
fn watch_shared(
    call: &Call,
    op_exe: &Path,
    plan: &RunnerPlan,
    lookup: &runner::Lookup,
    run_dir: &Path,
    when: &str,
) -> String {
    let run = |args: &[&str]| {
        launch::status_timeout(
            launch::command(op_exe)
                .args(args)
                .env_remove("OP_SERVICE_ACCOUNT_TOKEN")
                .env_remove("OP_CONNECT_HOST")
                .env_remove("OP_CONNECT_TOKEN")
                .env("WSLENV", launch::wslenv_without_op()),
            Duration::from_secs(30),
        )
        .is_some_and(|s| s.success())
    };
    if !run(&["whoami"]) {
        return "none".to_string();
    }
    let cleared = run(&["signout", "--all"]) && !run(&["whoami"]);
    let now = time::now();
    log::append(
        &call.state_dir,
        &log::line(&log::Shared {
            v: 1,
            id: &call.id,
            event: "shared-approval",
            time: &now.iso,
            epoch: now.epoch,
            when,
            cleared,
        }),
    );
    notify_shared(call, plan, lookup, run_dir, when, cleared, &now.hms);
    if cleared { "cleared" } else { "error" }.to_string()
}

/// Starts a runner for a `notify` payload in the background (stdin, stdout and stderr discarded).
fn spawn_notify(runner: &Runner, payload: &Payload) -> Option<std::process::Child> {
    let mut cmd = runner.command();
    cmd.env(contract::ENV_PAYLOAD, contract::payload_json(payload))
        .env("WSLENV", launch::wslenv_plus(&[contract::ENV_PAYLOAD]))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn().ok()
}

/// Tells about a shared approval: the runner's toast (not waited for, except that in auto mode the exe's first write is
/// awaited to know whether the launcher must show it instead) and a herdr notification inside herdr.
#[allow(clippy::too_many_arguments)]
fn notify_shared(
    call: &Call,
    plan: &RunnerPlan,
    lookup: &runner::Lookup,
    run_dir: &Path,
    when: &str,
    cleared: bool,
    hms: &str,
) {
    let mut payload = Payload {
        v: contract::CONTRACT_VERSION,
        mode: "notify".into(),
        op: None,
        args64: Vec::new(),
        status: None,
        lang: call.lang.code().into(),
        lock: "skipped".into(),
        lock_wait: 0,
        stderr: "inherit".into(),
        facts: None,
        notify: Some(Notify {
            kind: "shared-approval".into(),
            when: when.into(),
            cleared,
            time: hms.into(),
        }),
    };
    match plan {
        RunnerPlan::Exe {
            exe,
            folder,
            fallback,
        } => {
            let runner = Runner::Exe(exe.clone());
            if !*fallback {
                let _ = spawn_notify(&runner, &payload);
            } else {
                let status = run_dir.join(format!("notify-{when}.json"));
                let status_win = launch::wslpath_w(&status);
                if status_win.is_empty() {
                    // No way to tell whether the exe woke up: no fallback, rather than two toasts
                    let _ = spawn_notify(&runner, &payload);
                    return notify_herdr(call, when, cleared);
                }
                payload.status = Some(status_win);
                let woke = match spawn_notify(&runner, &payload) {
                    None => false,
                    Some(mut child) => {
                        let deadline = std::time::Instant::now() + NOTIFY_WAKE;
                        loop {
                            if fs::symlink_metadata(&status).is_ok() {
                                break true;
                            }
                            // Exited without the first write: the exe did not run
                            if !matches!(child.try_wait(), Ok(None)) {
                                break fs::symlink_metadata(&status).is_ok();
                            }
                            // Still starting after the wait: leave it be (it may still show the toast)
                            if std::time::Instant::now() >= deadline {
                                break true;
                            }
                            thread::sleep(Duration::from_millis(20));
                        }
                    }
                };
                if !woke
                    && run_dir.is_dir()
                    && let Ok(launcher) = lookup.launcher(Some(folder))
                {
                    payload.status = None;
                    let _ = spawn_notify(&Runner::Launcher(launcher), &payload);
                }
            }
        }
        RunnerPlan::Launcher { launcher, .. } => {
            let _ = spawn_notify(&Runner::Launcher(launcher.clone()), &payload);
        }
    }
    notify_herdr(call, when, cleared);
}

/// The herdr notification of a shared approval, inside herdr.
fn notify_herdr(call: &Call, when: &str, cleared: bool) {
    if std::env::var("HERDR_ENV").as_deref() == Ok("1") && launch::which("herdr").is_some() {
        let (title, body) = messages::shared_notice(call.lang, when, cleared);
        let _ = launch::status_timeout(
            launch::command("herdr").args([
                "notification",
                "show",
                &format!("🔑 {title}"),
                "--body",
                &body,
                "--sound",
                "request",
            ]),
            Duration::from_secs(2),
        );
    }
}

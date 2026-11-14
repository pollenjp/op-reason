//! One call of the wrapper (design section 6.1): plan the arguments, collect the caller, take the lock, watch for a
//! shared approval, start a fresh launcher (`powershell.exe`) for `op.exe`, and log the call.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use op_reason_core::args::{self, Kind, Mode, Name, Parsed, Plan, Refusal};
use op_reason_core::contract::{
    self, Facts, FactsCaller, Herdr, Note, Notify, Payload, Place, Status,
};
use op_reason_core::lang::{self, Lang};
use op_reason_core::log::{self, Caller};
use op_reason_core::{base64, launcher, logview, messages, place, result, time};

use crate::launch::{self, print_stderr, print_stdout};
use crate::lock::{self, Holder, Outcome};
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

    fn write_end(
        &self,
        start_epoch: i64,
        result: &str,
        exit: i32,
        status: Option<&Status>,
        watch_start: Option<&str>,
        watch_end: Option<&str>,
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
}

fn clean_up(state_dir: &Path, progress: &Progress) {
    if let Some(dir) = &progress.run_dir {
        let _ = fs::remove_dir_all(dir);
    }
    if progress.wrote_current {
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
    let lang = lang::from_env();
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
        Plan::UsageError(message) => {
            say(&[message]);
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
    logview::run(
        argv,
        &state_dir(),
        now,
        lang::from_env(),
        &mut out,
        &mut err,
    )
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
            herdr_label = herdr_workspace_label(&workspace);
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
fn current_dir_text() -> String {
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

/// `herdr workspace get <id>` → `.result.workspace.label` (2-second timeout; empty on any failure).
fn herdr_workspace_label(workspace: &str) -> String {
    launch::output_timeout(
        Command::new("herdr").args(["workspace", "get", workspace]),
        Duration::from_secs(2),
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
    let ps_name =
        env_nonempty("OPREASON_POWERSHELL").unwrap_or_else(|| "powershell.exe".to_string());
    let Some(ps) = launch::which(&ps_name) else {
        return call.refuse(
            "no-powershell",
            contract::EXIT_MISSING,
            false,
            &[messages::no_powershell(lang, &ps_name)],
        );
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
            match p.started_epoch {
                Some(epoch) => call.write_end(
                    epoch,
                    "interrupted",
                    code,
                    None,
                    p.watch_start.as_deref(),
                    None,
                ),
                None => call.log_refused("interrupted"),
            }
            clean_up(&call.state_dir, &p);
            std::process::exit(code);
        });
    }
    let run_dir = call.state_dir.join("run").join(&call.id);
    let _ = log::ensure_dir(&call.state_dir);
    let _ = log::ensure_dir(&run_dir);
    progress.lock().unwrap().run_dir = Some(run_dir.clone());
    let _cleanup = CleanUp {
        state_dir: call.state_dir.clone(),
        progress: Arc::clone(&progress),
    };

    // A pipe from another op-reason: read stdin before waiting for the lock (design 6.4)
    let mut drained = procfs::op_upstream().then(lock::drain_stdin);

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
    let mut _lock_file: Option<File> = None;
    let mut holder_reason = None;
    let mut waited = 0;
    let lock_mode = match acquired {
        Ok((Outcome::Held(f), w)) => {
            _lock_file = Some(f);
            waited = w;
            "held"
        }
        Ok((Outcome::Skipped { holder_reason: r }, w)) => {
            holder_reason = Some(r);
            waited = w;
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
    let watch_start = held.then(|| watch_shared(&call, &op_exe, &ps, "start"));
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
    });
    let payload = Payload {
        v: contract::CONTRACT_VERSION,
        mode: mode.as_str().to_string(),
        op: Some(launch::wslpath_w(&op_exe)),
        args64: parsed
            .op_args
            .iter()
            .map(|a| base64::encode(&args::os_bytes(a)))
            .collect(),
        status: Some(launch::wslpath_w(&status_file)),
        lang: lang.code().to_string(),
        lock: lock_mode.to_string(),
        lock_wait: wait.saturating_sub(waited).max(1),
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

    // start
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
        }),
    );
    progress.lock().unwrap().started_epoch = Some(start.epoch);

    // Launcher
    let (op_rc, stderr_tail) = start_launcher(&ps, &payload_json, kind, drained);
    let status = fs::read_to_string(&status_file)
        .ok()
        .and_then(|t| Status::parse(&t));
    let result = result::classify(status.as_ref(), op_rc, mode, &stderr_tail);
    if result == "lock-timeout" {
        let holder = status
            .as_ref()
            .and_then(|s| s.holder.clone())
            .unwrap_or_else(|| messages::unknown(lang).to_string());
        say(&messages::mutex_timeout(lang, &holder));
    }

    let watch_end = held.then(|| watch_shared(&call, &op_exe, &ps, "end"));
    call.write_end(
        start.epoch,
        result,
        op_rc,
        status.as_ref(),
        watch_start.as_deref(),
        watch_end.as_deref(),
    );
    op_rc
}

/// Starts the launcher and waits for it. stdin is ours (or the drained copy), stdout is ours, stderr is relayed to ours
/// while its tail is kept. Returns `op`'s exit code and the tail of its stderr.
fn start_launcher(
    ps: &Path,
    payload_json: &str,
    kind: Kind,
    drained: Option<Vec<u8>>,
) -> (i32, Vec<u8>) {
    let mut cmd = Command::new(ps);
    cmd.args(launcher::POWERSHELL_ARGS)
        .arg(launcher::encoded_command())
        .env(contract::ENV_SCRIPT, launcher::script_b64())
        .env(contract::ENV_PAYLOAD, payload_json);
    let base = [contract::ENV_SCRIPT, contract::ENV_PAYLOAD];
    match kind {
        Kind::ServiceAccount => {
            cmd.env("OP_BIOMETRIC_UNLOCK_ENABLED", "false").env(
                "WSLENV",
                launch::wslenv_plus(&[
                    base[0],
                    base[1],
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
                        base[0],
                        base[1],
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
                .env("WSLENV", launch::wslenv_plus(&base));
        }
    }
    cmd.stdout(Stdio::inherit()).stderr(Stdio::piped());
    cmd.stdin(if drained.is_some() {
        Stdio::piped()
    } else {
        Stdio::inherit()
    });
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return (contract::EXIT_MISSING, Vec::new()),
    };
    let writer = match (drained, child.stdin.take()) {
        (Some(data), Some(mut stdin)) => Some(thread::spawn(move || {
            let _ = stdin.write_all(&data);
        })),
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
    if let Some(w) = writer {
        let _ = w.join();
    }
    let tail = relay.and_then(|r| r.join().ok()).unwrap_or_default();
    (rc, tail)
}

/// Removes leftovers of calls killed by SIGKILL (`<state>/run/*` older than an hour).
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
fn watch_shared(call: &Call, op_exe: &Path, ps: &Path, when: &str) -> String {
    let run = |args: &[&str]| {
        launch::status_timeout(
            Command::new(op_exe)
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
    notify_shared(call, ps, when, cleared, &now.hms);
    if cleared { "cleared" } else { "error" }.to_string()
}

/// Tells about a shared approval: the launcher's toast (started in the background, not waited for) and a herdr
/// notification inside herdr.
fn notify_shared(call: &Call, ps: &Path, when: &str, cleared: bool, hms: &str) {
    let payload = Payload {
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
    let _ = Command::new(ps)
        .args(launcher::POWERSHELL_ARGS)
        .arg(launcher::encoded_command())
        .env(contract::ENV_SCRIPT, launcher::script_b64())
        .env(contract::ENV_PAYLOAD, contract::payload_json(&payload))
        .env(
            "WSLENV",
            launch::wslenv_plus(&[contract::ENV_SCRIPT, contract::ENV_PAYLOAD]),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if std::env::var("HERDR_ENV").as_deref() == Ok("1") && launch::which("herdr").is_some() {
        let (title, body) = messages::shared_notice(call.lang, when, cleared);
        let _ = launch::status_timeout(
            Command::new("herdr").args([
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

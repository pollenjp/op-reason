//! The SSH relay, `op-reason.exe --ssh-agent` (design `2026-11-17-ssh-relay-design.md`): a resident process that
//! serves the SSH agent protocol on its own pipe and passes every request on to the 1Password agent. per-call mode asks
//! once per call in op-reason's window and remembers the answer for the call; show-only mode decides nothing and shows
//! who and why while 1Password asks.
//!
//! Threads: the main thread accepts connections; one thread per connection reads its requests, decides and passes them
//! on, and shows the window when its turn comes ([`ask`]); one forgets decisions once a minute; one watches the relay's
//! parent process (§15); one waits for Ctrl+C while a console is attached; one records a window that does not appear
//! within five seconds (design `2026-11-22-front-after-allow-design.md` §12); and, from the first signature that passes
//! without a window, one shows the toasts (design `2026-11-18-pass-toasts-design.md` §4).

mod ask;
mod conn;
mod peer;
mod pipe;

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use op_reason_core::contract;
use op_reason_core::lang::{self, Lang};
use op_reason_core::log;
use op_reason_core::settings::relayinfo::{RELAY_INFO_FILE, RelayInfo};
use op_reason_core::ssh::decide::{Book, Queue};
use op_reason_core::ssh::record::{self, ProcRec, Relay};
use op_reason_core::{messages, settings, time};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows::core::HSTRING;

use super::{app, console, pass_window, procs, toast, window};
use crate::relay_front::Ancestor;
use crate::relay_opts::{self, Opts};
use crate::{relay_ui, relay_watch};

/// One relay per logon session.
const SINGLE: &str = r"Local\op-reason-ssh-relay";
/// How often decisions that ran out are forgotten.
const SWEEP: Duration = Duration::from_secs(60);
/// How often a parent that cannot be waited for is looked for.
const PARENT_POLL: Duration = Duration::from_secs(10);
/// How many instances of the pipe wait for clients at once (ansible's forks connect together).
const LISTENERS: usize = 4;
/// How long a stop waits for the requests that are already with 1Password (design `2026-11-23-settings-design.md`
/// §8.2): an unlock prompt may be up.
const STOP_DRAIN: Duration = Duration::from_secs(25);

/// How a window ended without an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum End {
    Timeout,
    Dismissed,
}

/// What the relay keeps in memory, behind one lock.
pub(super) struct State {
    pub book: Book,
    pub queue: Queue,
    /// How many requests follow each ticket's ask.
    pub followers: HashMap<u64, usize>,
    /// The keys' comments (their names in 1Password) by fingerprint, from the answers to the key list.
    pub comments: HashMap<String, String>,
    /// The windows that ended without an answer, by ticket, for the requests that waited for them.
    pub ended: HashMap<u64, (Instant, End)>,
}

/// Shared by every thread of the relay.
pub(super) struct Shared {
    pub opts: Opts,
    pub state_dir: Option<PathBuf>,
    /// `OPREASON_LOCK_WAIT`: how long a window waits for op-reason's lock.
    pub lock_wait: u64,
    pub state: Mutex<State>,
    /// Signalled whenever the queue or the decisions change.
    pub turn: Condvar,
    next: AtomicU64,
    /// The relay's parent process is missing or has ended.
    pub parent_gone: AtomicBool,
    pub told_no_prompt: AtomicBool,
    pub told_no_answer: AtomicBool,
    /// The toasts' thread, started by the first signature that passes without a window (none when it cannot start).
    toasts: OnceLock<Option<pass_window::Toasts>>,
    /// The relay's ancestors at start, nearest first: whether the window in front belongs to the program that started
    /// the relay (design `2026-11-22-front-after-allow-design.md` §5.1).
    pub ancestors: Vec<Ancestor>,
    /// Its stop event was set: new requests fail at once (design `2026-11-23-settings-design.md` §8.2).
    pub stopping: AtomicBool,
    /// Requests being handled now (read, and not yet answered).
    pub in_flight: AtomicUsize,
}

impl Shared {
    fn new(opts: Opts) -> Shared {
        let lock_wait = std::env::var("OPREASON_LOCK_WAIT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(contract::LOCK_WAIT_DEFAULT);
        Shared {
            state: Mutex::new(State {
                book: Book::new(opts.limit_secs as i64),
                queue: Queue::default(),
                followers: HashMap::new(),
                comments: HashMap::new(),
                ended: HashMap::new(),
            }),
            opts,
            state_dir: app::state_dir(),
            lock_wait,
            turn: Condvar::new(),
            next: AtomicU64::new(1),
            parent_gone: AtomicBool::new(false),
            told_no_prompt: AtomicBool::new(false),
            told_no_answer: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            toasts: OnceLock::new(),
            ancestors: peer::chain(std::process::id())
                .into_iter()
                .skip(1)
                .map(|l| Ancestor {
                    pid: l.pid,
                    start: l.start,
                    name: l.name,
                })
                .collect(),
        }
    }

    /// The toasts of signatures that pass without a window; a call's count is kept for the time limit.
    pub fn toasts(&self) -> Option<&pass_window::Toasts> {
        self.toasts
            .get_or_init(|| pass_window::Toasts::start(Duration::from_secs(self.opts.limit_secs)))
            .as_ref()
    }

    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A number used once (record IDs, requests that are calls of their own).
    pub fn unique(&self) -> u64 {
        self.next.fetch_add(1, Ordering::SeqCst)
    }

    /// A record ID: `<time>-<pid>-<n>`.
    pub fn new_id(&self) -> String {
        format!(
            "{}-{}-{}",
            time::now().id_stamp,
            std::process::id(),
            self.unique()
        )
    }

    /// The language of a window or a toast: the one chosen in a window, then `OPREASON_LANG` (read every time).
    pub fn lang(&self) -> Lang {
        lang::pick(app::chosen())
    }

    pub fn record(&self, line: String) {
        if let Some(dir) = &self.state_dir {
            record::append(dir, &line);
        }
    }

    /// An `ssh-relay` record.
    pub fn relay_record(&self, what: &str, parent: Option<ProcRec>) {
        self.relay_record_with(what, parent, None);
    }

    /// An `ssh-relay` record with a detail (the step of a window that does not appear).
    pub fn relay_note(&self, what: &str, detail: String) {
        self.relay_record_with(what, None, Some(detail));
    }

    fn relay_record_with(&self, what: &str, parent: Option<ProcRec>, detail: Option<String>) {
        let now = time::now();
        self.record(log::line(&Relay {
            v: 1,
            id: self.new_id(),
            event: "ssh-relay",
            time: now.iso,
            epoch: now.epoch,
            what: what.to_owned(),
            mode: self.opts.mode.as_str().to_owned(),
            pipe: self.opts.pipe.clone(),
            upstream: self.opts.upstream.clone(),
            limit: self.opts.limit_secs,
            pid: std::process::id(),
            parent,
            detail,
        }));
    }
}

fn say(line: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{}{line}", messages::PREFIX);
}

/// `op-reason.exe --ssh-agent [options]`: runs the relay until Ctrl+C, the end of the logon session or a kill. Returns
/// the exit code of a start that fails (2: options, 1: another relay or the pipe taken).
pub fn run(args: &[OsString]) -> i32 {
    let lang = lang::pick(app::chosen());
    let args: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // The values of settings.json first, then the options (design 2026-11-23-settings-design.md §8.1)
    let values = settings::values(
        app::settings_file()
            .and_then(|f| std::fs::read(f).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .as_deref(),
    );
    let opts = match relay_opts::parse_with(&args, relay_opts::base(&values)) {
        Ok(o) => o,
        Err(e) => {
            say(&e.message(lang));
            return contract::EXIT_USAGE;
        }
    };
    if opts.status {
        return status(&opts, lang);
    }
    let Some(single) = single_instance() else {
        say(&crate::messages::relay_running(lang));
        return 1;
    };
    let first = match pipe::Server::create(&opts.pipe, true) {
        Ok(s) => s,
        Err(e) => {
            say(&crate::messages::relay_pipe_taken(
                lang,
                &opts.pipe,
                &e.to_string(),
            ));
            return 1;
        }
    };
    say(&crate::messages::relay_started(
        lang,
        opts.mode.as_str(),
        &opts.pipe,
        &opts.upstream,
    ));
    let attached = !detach_console();
    let shared = Arc::new(Shared::new(opts));
    watch_stop(&shared);
    write_info(&shared, &args);
    let parent = Parent::find();
    shared.relay_record("start", parent.as_ref().map(Parent::rec));
    watch_parent(&shared, parent);
    {
        let shared = Arc::clone(&shared);
        let _ = thread::Builder::new().spawn(move || watch_windows(&shared));
    }
    {
        let shared = Arc::clone(&shared);
        let _ = thread::Builder::new().spawn(move || {
            loop {
                thread::sleep(SWEEP);
                let alive = |p: &op_reason_core::ssh::decide::ProcId| {
                    !p.scope.is_empty() || peer::alive(p.pid, p.start)
                };
                shared.lock().book.sweep(time::epoch_now(), &alive);
            }
        });
    }
    if attached {
        let s = Arc::clone(&shared);
        console::on_close(Box::new(move || {
            s.relay_record("exit", None);
            remove_info(&s);
        }));
        let shared = Arc::clone(&shared);
        let _ = thread::Builder::new().spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(250));
                if console::interrupted() {
                    console::without_close_hook(|| {
                        shared.relay_record("exit", None);
                        remove_info(&shared);
                    });
                    std::process::exit(130);
                }
            }
        });
    }
    for _ in 1..LISTENERS {
        let shared = Arc::clone(&shared);
        let _ = thread::Builder::new().spawn(move || accept(&shared, None));
    }
    accept(&shared, Some(first));
    drop(single);
    0
}

/// Accepts clients for ever, one thread each. The next instance is created as soon as a client is connected, so that
/// the pipe is busy for as short a time as possible (clients wait for a busy pipe, but only so long).
fn accept(shared: &Arc<Shared>, first: Option<pipe::Server>) {
    let create = || loop {
        match pipe::Server::create(&shared.opts.pipe, false) {
            Ok(s) => break s,
            Err(_) => thread::sleep(Duration::from_secs(1)),
        }
    };
    let mut server = first.unwrap_or_else(create);
    loop {
        let connected = server.accept().is_ok();
        let next = create();
        if connected {
            let s = Arc::clone(shared);
            // A thread that cannot be started drops the connection; the relay goes on.
            let _ = thread::Builder::new().spawn(move || conn::serve(s, server));
        }
        server = next;
    }
}

/// `--status`: whether a relay answers on the pipe (exit 0, with its process ID) or not (exit 1).
fn status(opts: &Opts, lang: Lang) -> i32 {
    match pipe::probe(&opts.pipe) {
        Some(pid) => {
            let _ = writeln!(
                std::io::stdout().lock(),
                "{}",
                crate::messages::relay_status(lang, &opts.pipe, Some(pid))
            );
            0
        }
        None => {
            let _ = writeln!(
                std::io::stdout().lock(),
                "{}",
                crate::messages::relay_status(lang, &opts.pipe, None)
            );
            1
        }
    }
}

/// `relay.json`: the relay's process, pipe and values, for `op-reason --settings` (design
/// `2026-11-23-settings-design.md` §8.2). Quiet when it cannot be written.
fn write_info(shared: &Shared, args: &[String]) {
    let Some(dir) = &shared.state_dir else {
        return;
    };
    let info = RelayInfo {
        v: 1,
        pid: std::process::id(),
        exe: std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        pipe: shared.opts.pipe.clone(),
        mode: shared.opts.mode.as_str().to_owned(),
        time_limit: shared.opts.limit_secs,
        answer_wait: shared.opts.answer_secs,
        args: args.to_vec(),
    };
    let _ = settings::replace(&dir.join(RELAY_INFO_FILE), &info.render());
}

/// Removes `relay.json` when it is this relay's.
fn remove_info(shared: &Shared) {
    let Some(file) = shared.state_dir.as_ref().map(|d| d.join(RELAY_INFO_FILE)) else {
        return;
    };
    let mine = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| RelayInfo::parse(&t))
        .is_some_and(|i| i.pid == std::process::id());
    if mine {
        let _ = std::fs::remove_file(&file);
    }
}

/// A handle that a thread waits on for the life of the relay.
struct Waitable(HANDLE);

// SAFETY: an event handle may be waited on from any thread; it is never closed (the relay exits after it).
unsafe impl Send for Waitable {}

/// The stop event, `Local\op-reason-ssh-relay-stop-<pid>` (design `2026-11-23-settings-design.md` §8.2): when it is
/// set, new requests fail at once, the requests in progress get up to 25 seconds, and the relay records `stop`,
/// removes `relay.json` and exits with 0.
fn watch_stop(shared: &Arc<Shared>) {
    let name = format!(r"Local\op-reason-ssh-relay-stop-{}", std::process::id());
    // SAFETY: a valid name; the handle lives until the process exits.
    let Ok(event) = (unsafe { CreateEventW(None, true, false, &HSTRING::from(name)) }) else {
        return;
    };
    let event = Waitable(event);
    let shared = Arc::clone(shared);
    let _ = thread::Builder::new().spawn(move || {
        let event = event;
        // SAFETY: a valid event handle.
        unsafe {
            let _ = WaitForSingleObject(event.0, INFINITE);
        }
        shared.stopping.store(true, Ordering::SeqCst);
        let t0 = Instant::now();
        while shared.in_flight.load(Ordering::SeqCst) > 0 && t0.elapsed() < STOP_DRAIN {
            thread::sleep(Duration::from_millis(50));
        }
        let left = shared.in_flight.load(Ordering::SeqCst);
        if left > 0 {
            shared.relay_note("stop", format!("{left} requests still in progress"));
        } else {
            shared.relay_record("stop", None);
        }
        remove_info(&shared);
        console::without_close_hook(|| std::process::exit(0));
    });
}

/// The mutex that keeps one relay per logon session, while it is held.
struct Single(HANDLE);

impl Drop for Single {
    fn drop(&mut self) {
        // SAFETY: we own the handle.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn single_instance() -> Option<Single> {
    // SAFETY: a valid name; the handle is owned by `Single`.
    unsafe {
        let h = CreateMutexW(None, true, &HSTRING::from(SINGLE)).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(h);
            return None;
        }
        Some(Single(h))
    }
}

/// Lets go of a console that was made for the relay alone (started from a logon task or by a double click), so that
/// no console window stays. Returns whether the relay has no console now.
fn detach_console() -> bool {
    let mut list = [0u32; 4];
    // SAFETY: a valid buffer.
    let n = unsafe { GetConsoleProcessList(&mut list) };
    if n == 1 {
        // SAFETY: plain call.
        unsafe {
            let _ = FreeConsole();
        }
    }
    n <= 1
}

/// The relay's parent process (design §8).
struct Parent {
    pid: u32,
    start: u64,
    name: String,
    /// A handle to wait on, when the relay may open the parent.
    handle: Option<HANDLE>,
}

// SAFETY: a process handle may be waited on from any thread.
unsafe impl Send for Parent {}

impl Parent {
    /// The parent, when it is there: a process with our parent's PID that started before us (a later one reused the
    /// PID).
    fn find() -> Option<Parent> {
        let me = std::process::id();
        let snap = procs::snapshot();
        let ppid = snap.iter().find(|p| p.pid == me)?.ppid;
        let p = snap.iter().find(|p| p.pid == ppid)?;
        let start = procs::created(ppid).unwrap_or(0);
        if let Some(mine) = procs::created(me)
            && start > mine
        {
            return None;
        }
        // SAFETY: the handle is owned by the watcher, which never closes it (the relay runs until it exits).
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, ppid).ok() };
        Some(Parent {
            pid: ppid,
            start,
            name: p.exe.clone(),
            handle,
        })
    }

    fn rec(&self) -> ProcRec {
        ProcRec {
            pid: self.pid,
            start: self.start,
            name: self.name.clone(),
            cmd: String::new(),
        }
    }

    /// Waits until the parent ends: on its handle, or by looking for it every few seconds.
    fn wait(&self) {
        if let Some(h) = self.handle {
            // SAFETY: a valid process handle.
            unsafe {
                let _ = WaitForSingleObject(h, INFINITE);
            }
            return;
        }
        loop {
            thread::sleep(PARENT_POLL);
            let there = procs::snapshot()
                .iter()
                .any(|p| p.pid == self.pid && p.exe == self.name);
            if !there {
                return;
            }
        }
    }
}

/// Records a window that has not appeared within five seconds, with the step its thread waits in, and when it goes on
/// ([`relay_watch`]; design `2026-11-22-front-after-allow-design.md` §12). Started before any window, so that it runs
/// whatever a window's thread waits for.
fn watch_windows(shared: &Shared) {
    let mut watch = relay_watch::Watch::default();
    loop {
        thread::sleep(Duration::from_secs(1));
        let busy = window::busy().map(|(since, step)| relay_watch::Busy { since, step });
        match watch.look(busy, window::clock_ms()) {
            Some(relay_watch::Note::Stuck(step)) => {
                shared.relay_note("window-stuck", step.to_owned())
            }
            Some(relay_watch::Note::WentOn { after_ms }) => shared.relay_note(
                "window-unstuck",
                format!("after {:.1} s", after_ms as f64 / 1000.0),
            ),
            None => {}
        }
    }
}

/// Says so (a toast and a record) when the parent is missing now or ends later.
fn watch_parent(shared: &Arc<Shared>, parent: Option<Parent>) {
    let Some(parent) = parent else {
        shared.parent_gone.store(true, Ordering::SeqCst);
        shared.relay_record("no-parent", None);
        let _ = toast::start(relay_ui::parent_toast(shared.lang(), false));
        return;
    };
    let shared = Arc::clone(shared);
    let _ = thread::Builder::new().spawn(move || {
        parent.wait();
        shared.parent_gone.store(true, Ordering::SeqCst);
        shared.relay_record("parent-ended", Some(parent.rec()));
        let _ = toast::start(relay_ui::parent_toast(shared.lang(), true));
    });
}

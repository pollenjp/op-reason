//! The SSH relay, `op-reason.exe --ssh-agent` (design `2026-11-17-ssh-relay-design.md`): a resident process that
//! serves the SSH agent protocol on its own pipe and passes every request on to the 1Password agent. per-call mode asks
//! once per call in op-reason's window and remembers the answer for the call; show-only mode decides nothing and shows
//! who and why while 1Password asks.
//!
//! Threads: the main thread accepts connections; one thread per connection reads its requests, decides and passes them
//! on, and shows the window when its turn comes ([`ask`]); one forgets decisions once a minute; one watches the relay's
//! parent process (1Password leaves a relay without one unanswered, §15); one waits for Ctrl+C while a console is
//! attached; and, from the first signature that passes without a window, one shows the toasts (design
//! `2026-11-18-pass-toasts-design.md` §4).

mod ask;
mod conn;
mod peer;
mod pipe;

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use op_reason_core::contract;
use op_reason_core::lang::{self, Lang};
use op_reason_core::log;
use op_reason_core::ssh::decide::{Book, Queue};
use op_reason_core::ssh::record::{self, ProcRec, Relay};
use op_reason_core::{messages, time};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
use windows::Win32::System::Threading::{
    CreateMutexW, INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows::core::HSTRING;

use super::{app, console, pass_window, procs, toast};
use crate::relay_opts::{self, Opts};
use crate::relay_ui;

/// One relay per logon session.
const SINGLE: &str = r"Local\op-reason-ssh-relay";
/// How often decisions that ran out are forgotten.
const SWEEP: Duration = Duration::from_secs(60);
/// How often a parent that cannot be waited for is looked for.
const PARENT_POLL: Duration = Duration::from_secs(10);
/// How many instances of the pipe wait for clients at once (ansible's forks connect together).
const LISTENERS: usize = 4;

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
            toasts: OnceLock::new(),
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
    let opts = match relay_opts::parse(&args) {
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
    let parent = Parent::find();
    shared.relay_record("start", parent.as_ref().map(Parent::rec));
    watch_parent(&shared, parent);
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
        console::on_close(Box::new(move || s.relay_record("exit", None)));
        let shared = Arc::clone(&shared);
        let _ = thread::Builder::new().spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(250));
                if console::interrupted() {
                    console::without_close_hook(|| shared.relay_record("exit", None));
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

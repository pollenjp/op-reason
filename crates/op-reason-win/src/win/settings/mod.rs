//! `op-reason.exe --settings …` (design `2026-11-23-settings-design.md`): the core's engine on Windows. [`WinSystem`]
//! implements the core's `System` with Task Scheduler ([`taskschd`]), the relay's pipe and named objects
//! ([`relayctl`]), the skill places on this machine and in WSL distributions ([`wslexe`]), and the records.

mod relayctl;
mod taskschd;
mod user;
mod wslexe;

use std::cell::OnceCell;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use op_reason_core::contract;
use op_reason_core::lang::Lang;
use op_reason_core::settings::cli;
use op_reason_core::settings::engine::{Change, Ctx, OtherTask, Place, System};
use op_reason_core::settings::relayinfo::RelayInfo;
use op_reason_core::settings::skill::{self, SkillAct, SkillNow};
use op_reason_core::settings::task::{self, TaskNow, TaskWant};
use op_reason_core::settings::wsl::Distro;
use op_reason_core::{log, messages, settings, time};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

use super::{app, mutex};
use crate::account;

/// The lock of `settings.json`'s writers (design §6).
const SETTINGS_LOCK: &str = r"Local\op-reason-settings";

/// Runs `f` while this process holds the lock of `settings.json`'s writers (design §6), waiting for it at most five
/// seconds (and running `f` without it after that: a writer that hangs must not keep others from writing).
pub(crate) fn with_lock<T>(f: impl FnOnce() -> T) -> T {
    let _held = match mutex::acquire(
        SETTINGS_LOCK,
        Duration::from_secs(5),
        Duration::MAX,
        || {},
        || false,
    ) {
        Ok(mutex::Acquired::Held(h)) => Some(h),
        _ => None,
    };
    f()
}

/// `op-reason.exe --settings <args>`. Returns the exit code.
pub fn run(args: &[OsString], lang: Lang) -> i32 {
    let args: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let cmd = match cli::parse(&args) {
        Ok(c) => c,
        Err(u) => {
            let mut err = io::stderr().lock();
            let _ = writeln!(err, "{}{}", messages::PREFIX, cli::usage_error(lang, &u));
            let _ = writeln!(
                err,
                "{}{}",
                messages::PREFIX,
                match lang {
                    Lang::En => "op-reason --settings help: the words and the settings",
                    Lang::Ja => "op-reason --settings help: 語と設定の一覧",
                }
            );
            return contract::EXIT_USAGE;
        }
    };
    // SAFETY: initializes COM for this thread (Task Scheduler); a thread initialized already is fine.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let sys = WinSystem::new();
    let ctx = Ctx {
        sys: &sys,
        version: app::VERSION,
        skill: app::SKILL,
        from: "cli",
    };
    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    cli::run(cmd, &ctx, lang, &mut out, &mut err)
}

/// The system on Windows.
pub struct WinSystem {
    state_dir: Option<PathBuf>,
    scheduler: OnceCell<Result<taskschd::Scheduler, String>>,
    sid: OnceCell<Result<String, String>>,
}

impl WinSystem {
    pub fn new() -> WinSystem {
        WinSystem {
            state_dir: app::state_dir(),
            scheduler: OnceCell::new(),
            sid: OnceCell::new(),
        }
    }

    fn scheduler(&self) -> Result<&taskschd::Scheduler, String> {
        self.scheduler
            .get_or_init(taskschd::Scheduler::connect)
            .as_ref()
            .map_err(Clone::clone)
    }

    fn sid(&self) -> Result<&str, String> {
        self.sid
            .get_or_init(user::sid)
            .as_deref()
            .map_err(Clone::clone)
    }

    /// op-reason's task: `op-reason-ssh-relay-<SID>`, or the test's name (`OPREASON_RELAY_TASK`, honored where `CI`
    /// is set).
    fn task_name(&self) -> Result<String, String> {
        if std::env::var_os("CI").is_some()
            && let Some(name) = std::env::var("OPREASON_RELAY_TASK")
                .ok()
                .filter(|v| !v.is_empty())
        {
            return Ok(name);
        }
        Ok(task::task_name(self.sid()?))
    }

    fn settings_file(&self) -> Option<PathBuf> {
        self.state_dir
            .as_ref()
            .map(|d| d.join(settings::SETTINGS_FILE))
    }

    fn account_file(&self) -> Option<PathBuf> {
        self.state_dir
            .as_ref()
            .map(|d| d.join(account::ACCOUNT_FILE))
    }
}

impl Default for WinSystem {
    fn default() -> WinSystem {
        WinSystem::new()
    }
}

fn read_text(path: &Path) -> io::Result<Option<String>> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

impl System for WinSystem {
    fn read_settings(&self) -> io::Result<Option<String>> {
        match self.settings_file() {
            Some(f) => read_text(&f),
            None => Ok(None),
        }
    }

    fn write_settings(&self, edit: &mut dyn FnMut(Option<&str>) -> String) -> io::Result<()> {
        let file = self
            .settings_file()
            .ok_or_else(|| io::Error::other("LOCALAPPDATA is not set"))?;
        // Other writers (a click on the language in a call's window, the settings window) wait for this one
        with_lock(|| {
            let old = read_text(&file)?;
            settings::replace(&file, &edit(old.as_deref()))
        })
    }

    fn settings_where(&self) -> String {
        self.settings_file()
            .map(|f| f.display().to_string())
            .unwrap_or_default()
    }

    fn task_where(&self) -> String {
        match self.task_name() {
            Ok(n) => format!(r"Task Scheduler \{n}"),
            Err(e) => e,
        }
    }

    fn task(&self) -> Result<Option<TaskNow>, String> {
        self.scheduler()?.get(&self.task_name()?)
    }

    fn task_want(&self) -> Result<TaskWant, String> {
        let exe = std::env::current_exe().map_err(|e| format!("op-reason.exe: {e}"))?;
        Ok(TaskWant {
            exe: exe.display().to_string(),
            sid: self.sid()?.to_owned(),
        })
    }

    fn same_user(&self, a: &str, b: &str) -> bool {
        user::same(a, b)
    }

    fn now_local(&self) -> String {
        time::now().ymd_hms.replace(' ', "T")
    }

    fn register_task(&self, xml: &str) -> Result<(), String> {
        self.scheduler()?.register(&self.task_name()?, xml)
    }

    fn remove_task(&self) -> Result<(), String> {
        self.scheduler()?.delete(&self.task_name()?)
    }

    fn enable_task(&self, on: bool) -> Result<(), String> {
        self.scheduler()?.set_enabled(&self.task_name()?, on)
    }

    fn run_task(&self) -> Result<(), String> {
        self.scheduler()?.run(&self.task_name()?)
    }

    fn other_tasks(&self, all: bool) -> Vec<OtherTask> {
        match (self.scheduler(), self.task_name()) {
            (Ok(s), Ok(name)) => s.relay_tasks(&name, all),
            _ => Vec::new(),
        }
    }

    fn relay_pid(&self) -> Option<u32> {
        relayctl::pid()
    }

    fn relay_info(&self) -> Option<RelayInfo> {
        relayctl::info(self.state_dir.as_deref())
    }

    fn lock(&self) -> Option<Box<dyn std::any::Any>> {
        match mutex::acquire(
            contract::MUTEX_NAME,
            Duration::from_secs(2),
            Duration::MAX,
            || {},
            || false,
        ) {
            Ok(mutex::Acquired::Held(h)) => Some(Box::new(h)),
            _ => None,
        }
    }

    fn stop_relay(&self, pid: u32) -> Result<(), String> {
        relayctl::stop(pid)
    }

    fn wait_relay(&self) -> Option<u32> {
        relayctl::wait_up()
    }

    fn skill(&self, place: &Place, act: SkillAct, want: &str) -> Result<SkillNow, String> {
        match place {
            Place::Windows => {
                let home = std::env::var_os("USERPROFILE")
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| "USERPROFILE is not set".to_owned())?;
                let base = PathBuf::from(home).join(".claude").join("skills");
                skill::local(&base, true, act, want, app::VERSION)
            }
            Place::Folder(f) => skill::local(Path::new(f), true, act, want, app::VERSION),
            Place::Wsl(d) => wslexe::skill(d, act, want, app::VERSION),
        }
    }

    fn distributions(&self) -> Result<Vec<Distro>, String> {
        wslexe::distributions()
    }

    fn records_where(&self) -> String {
        self.state_dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_default()
    }

    fn accounts(&self) -> usize {
        self.account_file()
            .map(|f| account::read(&f).len())
            .unwrap_or(0)
    }

    fn forget_accounts(&self) -> Result<(), String> {
        match self.account_file() {
            Some(f) => settings::remove(&f).map_err(|e| format!("{}: {e}", f.display())),
            None => Ok(()),
        }
    }

    fn open_records(&self) -> Result<(), String> {
        let dir = self
            .state_dir
            .clone()
            .ok_or_else(|| "LOCALAPPDATA is not set".to_owned())?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::process::Command::new("explorer.exe")
            .arg(&dir)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("explorer.exe: {e}"))
    }

    fn record(&self, from: &str, changes: &[Change]) {
        let Some(dir) = &self.state_dir else {
            return;
        };
        let now = time::now();
        let (caller, session) = user::caller();
        let line = settings::record::line(
            &log::new_id(&now),
            &now,
            session.as_deref(),
            from,
            &caller,
            changes,
        );
        log::append(dir, &line);
    }
}

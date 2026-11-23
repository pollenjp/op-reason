//! The decisions of the settings (design `2026-11-23-settings-design.md` §5, §9, §10): `list`, `get`, `check`, `set`
//! and the relay's words, over a [`System`] that reads and changes what lives outside `settings.json`. Everything that
//! decides what to stop, start, write or refuse is here, so that it is tested with a fake system.

use std::cell::OnceCell;
use std::io;

use serde_json::Value;

use crate::settings::catalog::{self, Name, Want};
use crate::settings::cli::RelayAct;
use crate::settings::file::{self, Values};
use crate::settings::relayinfo::RelayInfo;
use crate::settings::skill::{self, SkillAct, SkillNow, SkillState};
use crate::settings::task::{self, TaskNow, TaskWant};
use crate::settings::values;
use crate::settings::wsl::Distro;

/// A place for the skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// `%USERPROFILE%\.claude\skills`.
    Windows,
    /// `~/.claude/skills` of a WSL distribution.
    Wsl(String),
    /// A folder the user chose.
    Folder(String),
}

/// A scheduled task, not op-reason's, whose action starts `op-reason.exe --ssh-agent` (design §9.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherTask {
    /// Its path in Task Scheduler (`\pjp\op-reason-ssh-relay`).
    pub path: String,
    /// An instance runs.
    pub running: bool,
    /// Turned on.
    pub enabled: bool,
}

/// What lives outside the decisions: `settings.json`, op-reason's task, the relay, the lock, the skill places, the
/// records. `op-reason-win` implements it on Windows; the tests with a fake.
pub trait System {
    /// The text of `settings.json` (`None`: no file).
    fn read_settings(&self) -> io::Result<Option<String>>;
    /// Writes `settings.json` with the text `edit` makes from the current one, while no other writer writes.
    fn write_settings(&self, edit: &mut dyn FnMut(Option<&str>) -> String) -> io::Result<()>;
    /// Where `settings.json` is.
    fn settings_where(&self) -> String;
    /// Where op-reason's task is (its path in Task Scheduler).
    fn task_where(&self) -> String;
    /// op-reason's task's name (what its relay tells in `relay.json`).
    fn task_name(&self) -> Result<String, String>;
    /// op-reason's task, when it is there.
    fn task(&self) -> Result<Option<TaskNow>, String>;
    /// What op-reason's task is made from (this exe, the user).
    fn task_want(&self) -> Result<TaskWant, String>;
    /// Whether two users of a task (a SID or a name) are the same user.
    fn same_user(&self, a: &str, b: &str) -> bool;
    /// The local time now, for the task's repeating trigger (`2026-11-23T10:00:00`).
    fn now_local(&self) -> String;
    /// Makes op-reason's task from its XML (made again when it is there).
    fn register_task(&self, xml: &str) -> Result<(), String>;
    /// Removes op-reason's task.
    fn remove_task(&self) -> Result<(), String>;
    /// Turns op-reason's task on or off.
    fn enable_task(&self, on: bool) -> Result<(), String>;
    /// Runs op-reason's task now.
    fn run_task(&self) -> Result<(), String>;
    /// Other tasks that start a relay: the running ones, or all (`all`, slower).
    fn other_tasks(&self, all: bool) -> Vec<OtherTask>;
    /// The process that answers on the relay's pipe.
    fn relay_pid(&self) -> Option<u32>;
    /// `relay.json` as it is (the engine checks its process ID).
    fn relay_info(&self) -> Option<RelayInfo>;
    /// op-reason's lock (`Local\op-reason`), waiting at most two seconds; held until the value is dropped.
    fn lock(&self) -> Option<Box<dyn std::any::Any>>;
    /// Ends the relay `pid` (its stop event, then `TerminateProcess`) and waits until it has ended.
    fn stop_relay(&self, pid: u32) -> Result<(), String>;
    /// Waits up to 15 seconds for a relay to answer on the pipe.
    fn wait_relay(&self) -> Option<u32>;
    /// Looks at, places or removes the skill `want` (marked) at a place.
    fn skill(&self, place: &Place, act: SkillAct, want: &str) -> Result<SkillNow, String>;
    /// The WSL distributions (none without WSL).
    fn distributions(&self) -> Result<Vec<Distro>, String>;
    /// The folder of the records.
    fn records_where(&self) -> String;
    /// How many accounts are remembered for links.
    fn accounts(&self) -> usize;
    /// Forgets them (`account.json`).
    fn forget_accounts(&self) -> Result<(), String>;
    /// Opens the folder of the records.
    fn open_records(&self) -> Result<(), String>;
    /// Writes a `settings` record of the changes tried.
    fn record(&self, from: &str, changes: &[Change]);
}

/// What a command works with.
pub struct Ctx<'a> {
    /// The system.
    pub sys: &'a dyn System,
    /// op-reason's version (the skill's mark).
    pub version: &'a str,
    /// `SKILL.md` as `op-reason --skill` prints it.
    pub skill: &'a str,
    /// `cli`, `window` or `tray` (the record's `from`).
    pub from: &'a str,
}

/// What a setting's value now comes with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Nothing to say.
    Ok,
    /// `settings.json` holds a value that is not accepted (its JSON); the default counts.
    Ignored(String),
    /// The running relay uses another value than the file's.
    RelayDiffers {
        /// The relay's value.
        relay: String,
        /// Its process ID.
        pid: u32,
    },
    /// op-reason's task differs in these fields.
    TaskDiffers(Vec<&'static str>),
    /// op-reason's task is turned off (a stop): no relay until a start.
    TaskOff,
    /// op-reason's task is on and no relay answers yet: it starts one within a minute.
    TaskStarting,
    /// op-reason's task is on while a relay it did not start answers: it starts `op-reason.exe` every minute, which
    /// exits at once.
    OtherRelay {
        /// The relay's process ID.
        pid: u32,
        /// The task that started it, when one did.
        by: Option<String>,
    },
    /// A relay answers.
    Running {
        /// Its process ID.
        pid: u32,
        /// op-reason's task runs it.
        ours: bool,
        /// Another task that runs it, when one does.
        by: Option<String>,
    },
    /// A skill place.
    Skill {
        /// What is there.
        state: SkillState,
        /// A link on the way.
        link: Option<String>,
    },
    /// Other tasks start a relay too (`list` only: it walks every task folder).
    OtherTasks(Vec<String>),
    /// A WSL distribution that does not run: not looked at.
    NotRunning,
    /// The value could not be read.
    Unreadable(String),
}

/// A setting now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The setting.
    pub name: Name,
    /// Its value now (`None`: not read).
    pub value: Option<String>,
    /// Its default.
    pub default: Option<&'static str>,
    /// What it comes with.
    pub state: State,
    /// Where it is kept.
    pub where_: String,
}

/// A wanted value compared with the value now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The value now.
    pub item: Item,
    /// The wanted value as the command line writes it.
    pub wanted: String,
    /// The same.
    pub same: bool,
}

/// How a change ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// Nothing to do: it was so.
    Same,
    /// Done.
    Done,
    /// Not done, and why.
    Failed(Why),
}

/// Why a change was not done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// op-reason's lock is held: the relay has a window open, or an `op` call waits for 1Password.
    Locked,
    /// The relay was not started by op-reason's task (by another task, when known): op-reason never stops it.
    NotOurs(Option<String>),
    /// op-reason's task is not there: op-reason starts the relay from its task only.
    NoTask,
    /// The relay did not answer within 15 seconds.
    NoAnswer,
    /// A `SKILL.md` that op-reason did not place.
    Foreign(String),
    /// A junction or a symbolic link on the way.
    Link(String),
    /// The system said no.
    Failed(String),
}

/// Something a change says besides how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// op-reason's task was made turned off while another relay answers (by that task, when known).
    TaskMadeOff(Option<String>),
    /// The relay was started again.
    Restarted,
}

/// One change tried by `set` or an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The setting (or `records.accounts`, `records.folder`).
    pub name: String,
    /// The value before.
    pub from: Option<String>,
    /// The value asked for.
    pub to: String,
    /// How it ended.
    pub done: Done,
    /// What it says besides.
    pub note: Option<Note>,
}

impl Change {
    fn new(name: &str, from: Option<&str>, to: &str, done: Done) -> Change {
        Change {
            name: name.to_owned(),
            from: from.map(str::to_owned),
            to: to.to_owned(),
            done,
            note: None,
        }
    }
}

/// The relay now: op-reason's task, the process that answers, and the values it tells.
struct Relay {
    task: Result<Option<TaskNow>, String>,
    pid: Option<u32>,
    info: Option<RelayInfo>,
    by: Option<String>,
    ours: bool,
}

impl Relay {
    fn read(sys: &dyn System) -> Relay {
        let task = sys.task();
        let pid = sys.relay_pid();
        let info = sys
            .relay_info()
            .filter(|i| Some(i.pid) == pid && i.pipe.eq_ignore_ascii_case(values::RELAY_PIPE));
        let name = sys.task_name().ok();
        let ours =
            matches!((&task, &info), (Ok(Some(_)), Some(i)) if i.task.is_some() && i.task == name);
        let by = if pid.is_some() && !ours {
            sys.other_tasks(false)
                .into_iter()
                .find(|t| t.running)
                .map(|t| t.path)
        } else {
            None
        };
        Relay {
            task,
            pid,
            info,
            by,
            ours,
        }
    }

    fn task(&self) -> Option<&TaskNow> {
        self.task.as_ref().ok().and_then(Option::as_ref)
    }

    /// op-reason's task runs the relay that answers: the relay that answers wrote `relay.json` with op-reason's task
    /// in it (`--task <name>`, which only op-reason's task passes; design §9.5). A Running task alone is not enough:
    /// while another relay runs, the instance of every minute starts an `op-reason.exe` that exits at once.
    fn ours(&self) -> bool {
        self.ours
    }
}

/// What every look shares: the file's values, and the relay read once when a setting needs it.
struct Now<'a> {
    ctx: &'a Ctx<'a>,
    values: Values,
    file_error: Option<String>,
    relay: OnceCell<Relay>,
}

impl<'a> Now<'a> {
    fn read(ctx: &'a Ctx<'a>) -> Now<'a> {
        let text = ctx.sys.read_settings();
        let file_error = text.as_ref().err().map(|e| e.to_string());
        Now {
            ctx,
            values: file::values(text.ok().flatten().as_deref()),
            file_error,
            relay: OnceCell::new(),
        }
    }

    fn relay(&self) -> &Relay {
        self.relay.get_or_init(|| Relay::read(self.ctx.sys))
    }

    fn ignored(&self, path: &str) -> Option<String> {
        self.values
            .ignored
            .iter()
            .find(|(k, _)| k == path)
            .map(|(_, v)| v.clone())
    }

    /// A setting now; `distros`, when given, keeps distributions that do not run from being looked at.
    fn item(&self, name: &Name, distros: Option<&[Distro]>) -> Item {
        let sys = self.ctx.sys;
        let mut item = Item {
            name: name.clone(),
            value: None,
            default: name.default_text(),
            state: State::Ok,
            where_: String::new(),
        };
        if name.is_file() {
            item.where_ = sys.settings_where();
            if let Some(e) = &self.file_error {
                item.state = State::Unreadable(e.clone());
                return item;
            }
            let (path, value, relay_value) = self.file_value(name);
            item.value = Some(value.clone());
            item.state = if let Some(raw) = self
                .ignored(path)
                .or_else(|| self.ignored(file::WHOLE_FILE))
            {
                State::Ignored(raw)
            } else if let Some(relay_value) = relay_value
                && let Some(pid) = self.relay().pid
                && relay_value != value
            {
                State::RelayDiffers {
                    relay: relay_value,
                    pid,
                }
            } else {
                State::Ok
            };
            return item;
        }
        match name {
            Name::RelayAutostart => {
                item.where_ = sys.task_where();
                let relay = self.relay();
                match &relay.task {
                    Err(e) => item.state = State::Unreadable(e.clone()),
                    Ok(None) => item.value = Some("off".into()),
                    Ok(Some(t)) => {
                        item.value = Some("on".into());
                        item.state = match task_differences(sys, t) {
                            Err(e) => State::Unreadable(e),
                            Ok(d) if !d.is_empty() => State::TaskDiffers(d),
                            Ok(_) => match relay.pid {
                                Some(pid) if t.enabled && !relay.ours() => State::OtherRelay {
                                    pid,
                                    by: relay.by.clone(),
                                },
                                _ => State::Ok,
                            },
                        };
                    }
                }
            }
            Name::RelayRunning => {
                item.where_ = values::RELAY_PIPE.to_owned();
                let relay = self.relay();
                match relay.pid {
                    Some(pid) => {
                        item.value = Some("on".into());
                        item.state = State::Running {
                            pid,
                            ours: relay.ours(),
                            by: relay.by.clone(),
                        };
                    }
                    None => {
                        item.value = Some("off".into());
                        item.state = match relay.task() {
                            Some(t) if !t.enabled => State::TaskOff,
                            Some(_) => State::TaskStarting,
                            None => State::Ok,
                        };
                    }
                }
            }
            _ => {
                let place = place_of(name).expect("a skill place");
                if let (Place::Wsl(d), Some(distros)) = (&place, distros)
                    && !distros.iter().any(|x| &x.name == d && x.running)
                {
                    item.state = State::NotRunning;
                    return item;
                }
                let want = skill::marked(self.ctx.skill, self.ctx.version);
                match sys.skill(&place, SkillAct::Look, &want) {
                    Ok(now) => {
                        item.value = Some(skill_value(&now.state).into());
                        item.where_ = now.path;
                        item.state = State::Skill {
                            state: now.state,
                            link: now.link,
                        };
                    }
                    Err(e) => item.state = State::Unreadable(e),
                }
            }
        }
        item
    }

    /// A file setting: its path in `settings.json`, its value (the default when not set), and the running relay's
    /// value when the relay tells it.
    fn file_value(&self, name: &Name) -> (&'static str, String, Option<String>) {
        let v = &self.values;
        if *name == Name::Lang {
            return (
                "lang",
                v.lang.map_or("default", |l| l.code()).to_owned(),
                None,
            );
        }
        let info = self.relay_info();
        match name {
            Name::RelayMode => (
                "relay.mode",
                v.mode.unwrap_or_default().as_str().to_owned(),
                info.map(|i| i.mode.clone()),
            ),
            Name::RelayTimeLimit => (
                "relay.timeLimit",
                values::minutes_text(v.time_limit.unwrap_or(values::DEFAULT_TIME_LIMIT)),
                info.map(|i| values::minutes_text(i.time_limit)),
            ),
            _ => (
                "relay.answerWait",
                values::seconds_text(v.answer_wait.unwrap_or(values::DEFAULT_ANSWER_WAIT)),
                info.map(|i| values::seconds_text(i.answer_wait)),
            ),
        }
    }

    /// The relay's values, only for the relay settings (no relay is read for `lang`).
    fn relay_info(&self) -> Option<&RelayInfo> {
        self.relay().info.as_ref()
    }
}

/// The fields in which op-reason's task differs from the task this exe would make.
fn task_differences(sys: &dyn System, t: &TaskNow) -> Result<Vec<&'static str>, String> {
    let want = sys.task_want()?;
    Ok(task::differences(
        &t.shape,
        &task::wanted(&want),
        &|a, b| sys.same_user(a, b),
    ))
}

fn place_of(name: &Name) -> Option<Place> {
    match name {
        Name::SkillWindows => Some(Place::Windows),
        Name::SkillWsl(d) => Some(Place::Wsl(d.clone())),
        Name::SkillFolder(f) => Some(Place::Folder(f.clone())),
        _ => None,
    }
}

/// `on` while op-reason's skill is there (current or not), `off` otherwise.
fn skill_value(state: &SkillState) -> &'static str {
    match state {
        SkillState::Ok | SkillState::Old(_) | SkillState::Changed => "on",
        SkillState::Missing | SkillState::Foreign => "off",
    }
}

/// Every setting now: the fixed ones, the WSL distributions (those that do not run are not looked at) and the folders
/// that `settings.json` remembers.
pub fn list(ctx: &Ctx) -> Vec<Item> {
    let now = Now::read(ctx);
    let distros = ctx.sys.distributions().unwrap_or_default();
    let mut names = catalog::fixed();
    names.extend(distros.iter().map(|d| Name::SkillWsl(d.name.clone())));
    names.extend(
        now.values
            .folders
            .iter()
            .map(|f| Name::SkillFolder(f.clone())),
    );
    let mut items: Vec<Item> = names.iter().map(|n| now.item(n, Some(&distros))).collect();
    if let Some(autostart) = items.iter_mut().find(|i| i.name == Name::RelayAutostart)
        && autostart.state == State::Ok
    {
        let others: Vec<String> = ctx
            .sys
            .other_tasks(true)
            .into_iter()
            .map(|t| t.path)
            .collect();
        if !others.is_empty() {
            autostart.state = State::OtherTasks(others);
        }
    }
    items
}

/// One setting now (a named distribution is looked at, which starts it).
pub fn get(ctx: &Ctx, name: &Name) -> Item {
    Now::read(ctx).item(name, None)
}

/// The wanted values compared with the values now.
pub fn check(ctx: &Ctx, wants: &[(Name, Want)]) -> Vec<Checked> {
    let now = Now::read(ctx);
    wants
        .iter()
        .map(|(name, want)| {
            let item = now.item(name, None);
            let same = same(&now, name, want, &item);
            Checked {
                wanted: catalog::want_text(name, want),
                item,
                same,
            }
        })
        .collect()
}

fn same(now: &Now, name: &Name, want: &Want, item: &Item) -> bool {
    let value = item.value.as_deref();
    let relay_same = !matches!(item.state, State::RelayDiffers { .. });
    match (name, want) {
        (_, _) if matches!(item.state, State::Unreadable(_)) => false,
        (Name::Lang, Want::Lang(l)) => now.values.lang == *l,
        (Name::RelayMode | Name::RelayTimeLimit | Name::RelayAnswerWait, w) => {
            let wanted = match w {
                Want::Default => name.default_text().map(str::to_owned),
                _ => Some(catalog::want_text(name, w)),
            };
            value == wanted.as_deref() && relay_same
        }
        (Name::RelayAutostart, Want::On(true)) => {
            value == Some("on") && !matches!(item.state, State::TaskDiffers(_))
        }
        (Name::RelayAutostart, Want::On(false)) => value == Some("off"),
        (Name::RelayRunning, Want::On(true)) => value == Some("on"),
        (Name::RelayRunning, Want::On(false)) => {
            value == Some("off") && item.state != State::TaskStarting
        }
        (_, Want::On(on)) => match &item.state {
            State::Skill { state, .. } => {
                if *on {
                    *state == SkillState::Ok
                } else {
                    matches!(state, SkillState::Missing | SkillState::Foreign)
                }
            }
            _ => false,
        },
        _ => false,
    }
}

/// Makes the wanted values so, in the order of design §7.1 (the file, then `relay.autostart`, then `relay.running`,
/// then a restart for the relay's values, then the skill places, whatever the order they were given in), and records
/// what it tried.
pub fn set(ctx: &Ctx, wants: &[(Name, Want)]) -> Vec<Change> {
    let pid_before = ctx.sys.relay_pid();
    let mut out = Vec::new();
    let file_wants: Vec<&(Name, Want)> = wants.iter().filter(|(n, _)| n.is_file()).collect();
    let relay_named = file_wants.iter().any(|(n, _)| *n != Name::Lang);
    let mut relay_changed = false;
    if !file_wants.is_empty() {
        let (changes, changed) = set_file(ctx, &file_wants);
        relay_changed = changed;
        out.extend(changes);
    }
    if let Some(on) = on_of(wants, &Name::RelayAutostart) {
        out.extend(autostart(ctx, on));
    }
    if let Some(on) = on_of(wants, &Name::RelayRunning) {
        out.push(running(ctx, on));
    }
    if relay_named && let Some(c) = restart_for_values(ctx, relay_changed, pid_before) {
        out.push(c);
    }
    for (name, want) in wants {
        if let (Some(place), Want::On(on)) = (place_of(name), want) {
            out.push(skill_set(ctx, name, &place, *on));
        }
    }
    record(ctx, &out);
    out
}

/// The `on`/`off` wanted for `name`.
fn on_of(wants: &[(Name, Want)], name: &Name) -> Option<bool> {
    wants.iter().find_map(|(n, w)| match w {
        Want::On(on) if n == name => Some(*on),
        _ => None,
    })
}

/// Writes the file settings in one write; returns the changes and whether a relay value changed in the file.
fn set_file(ctx: &Ctx, wants: &[&(Name, Want)]) -> (Vec<Change>, bool) {
    let paths = |name: &Name| -> &'static [&'static str] {
        match name {
            Name::Lang => &["lang"],
            Name::RelayMode => &["relay", "mode"],
            Name::RelayTimeLimit => &["relay", "timeLimit"],
            _ => &["relay", "answerWait"],
        }
    };
    let mut before_text: Option<String> = None;
    let mut after_text: Option<String> = None;
    let result = ctx.sys.write_settings(&mut |current| {
        before_text = current.map(str::to_owned);
        let text = file::edit(current, |m| {
            for (name, want) in wants {
                let value = match want {
                    Want::Default | Want::Lang(None) => None,
                    Want::Lang(Some(l)) => Some(Value::from(l.code())),
                    Want::Mode(mode) => Some(Value::from(mode.as_str())),
                    Want::Secs(s) => Some(Value::from(*s)),
                    Want::On(on) => Some(Value::from(*on)),
                };
                file::put(m, paths(name), value);
            }
        });
        after_text = Some(text.clone());
        text
    });
    let before = file::values(before_text.as_deref());
    let mut relay_changed = false;
    let changes = wants
        .iter()
        .map(|(name, want)| {
            let from = file_text(name, &before);
            let to = catalog::want_text(name, want);
            let done = match &result {
                Err(e) => Done::Failed(Why::Failed(e.to_string())),
                Ok(()) => {
                    let path = paths(name);
                    if file::field(before_text.as_deref(), path)
                        == file::field(after_text.as_deref(), path)
                    {
                        Done::Same
                    } else {
                        relay_changed |= *name != Name::Lang;
                        Done::Done
                    }
                }
            };
            Change::new(&name.text(), Some(&from), &to, done)
        })
        .collect();
    (changes, relay_changed)
}

/// A file setting's value as `get` prints it.
fn file_text(name: &Name, v: &Values) -> String {
    match name {
        Name::Lang => v.lang.map_or("default", |l| l.code()).to_owned(),
        Name::RelayMode => v.mode.unwrap_or_default().as_str().to_owned(),
        Name::RelayTimeLimit => {
            values::minutes_text(v.time_limit.unwrap_or(values::DEFAULT_TIME_LIMIT))
        }
        _ => values::seconds_text(v.answer_wait.unwrap_or(values::DEFAULT_ANSWER_WAIT)),
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// Makes op-reason's task from this exe, turned on or off.
fn register(ctx: &Ctx, enabled: bool) -> Result<(), Why> {
    let want = ctx.sys.task_want().map_err(Why::Failed)?;
    ctx.sys
        .register_task(&task::xml(&want, &ctx.sys.now_local(), enabled))
        .map_err(Why::Failed)
}

/// Runs op-reason's task and waits for the relay to answer.
fn start(ctx: &Ctx) -> Result<(), Why> {
    ctx.sys.run_task().map_err(Why::Failed)?;
    ctx.sys.wait_relay().map(|_| ()).ok_or(Why::NoAnswer)
}

fn done(r: Result<(), Why>) -> Done {
    match r {
        Ok(()) => Done::Done,
        Err(w) => Done::Failed(w),
    }
}

/// A held lock.
type Held = Box<dyn std::any::Any>;

/// op-reason's lock, and the relay read again under it (design §9.4): the process of op-reason's task's relay that
/// answers (`None` when none answers now). A relay that op-reason's task did not start is refused. The lock is held
/// while the returned value lives.
fn lock_ours(ctx: &Ctx) -> Result<(Held, Option<u32>), Why> {
    let held = ctx.sys.lock().ok_or(Why::Locked)?;
    let relay = Relay::read(ctx.sys);
    match relay.pid {
        None => Ok((held, None)),
        Some(pid) if relay.ours() => Ok((held, Some(pid))),
        Some(_) => Err(Why::NotOurs(relay.by)),
    }
}

/// With the lock and the relay read again: turns op-reason's task off (so that the trigger of every minute starts
/// none in between) and stops its relay. Returns the lock, for a start that follows.
fn stop_ours(ctx: &Ctx) -> Result<Held, Why> {
    let (held, pid) = lock_ours(ctx)?;
    ctx.sys.enable_task(false).map_err(Why::Failed)?;
    if let Some(pid) = pid {
        ctx.sys.stop_relay(pid).map_err(Why::Failed)?;
    }
    Ok(held)
}

fn autostart(ctx: &Ctx, on: bool) -> Vec<Change> {
    let sys = ctx.sys;
    let relay = Relay::read(sys);
    let had = match &relay.task {
        Err(e) => {
            return vec![Change::new(
                "relay.autostart",
                None,
                on_off(on),
                Done::Failed(Why::Failed(e.clone())),
            )];
        }
        Ok(t) => t.clone(),
    };
    let mut change = Change::new(
        "relay.autostart",
        Some(on_off(had.is_some())),
        on_off(on),
        Done::Same,
    );
    let other = relay.pid.is_some() && !relay.ours();
    if !on {
        if had.is_some() {
            change.done = done(if relay.ours() {
                stop_ours(ctx).and_then(|_held| sys.remove_task().map_err(Why::Failed))
            } else {
                sys.enable_task(false)
                    .and_then(|()| sys.remove_task())
                    .map_err(Why::Failed)
            });
        }
        return vec![change];
    }
    let mut extra = Vec::new();
    match had {
        None => {
            // While another relay answers, the task is made turned off: turned on, it would start op-reason.exe every
            // minute, which exits at once (design §9.2)
            change.done = done(register(ctx, !other));
            if other {
                change.note = Some(Note::TaskMadeOff(relay.by.clone()));
            } else if change.done == Done::Done
                && let Err(w) = start(ctx)
            {
                extra.push(Change::new(
                    "relay.running",
                    Some("off"),
                    "on",
                    Done::Failed(w),
                ));
            }
        }
        Some(t) => {
            let differs = match task_differences(sys, &t) {
                Ok(d) => !d.is_empty(),
                Err(e) => {
                    change.done = Done::Failed(Why::Failed(e));
                    return vec![change];
                }
            };
            if differs {
                change.done = done(if relay.ours() {
                    stop_ours(ctx).and_then(|_held| {
                        register(ctx, true)?;
                        start(ctx)
                    })
                } else {
                    if other {
                        change.note = Some(Note::TaskMadeOff(relay.by.clone()));
                    }
                    register(ctx, t.enabled && !other)
                });
            }
        }
    }
    let mut out = vec![change];
    out.extend(extra);
    out
}

fn running(ctx: &Ctx, on: bool) -> Change {
    let sys = ctx.sys;
    let relay = Relay::read(sys);
    let mut change = Change::new(
        "relay.running",
        Some(on_off(relay.pid.is_some())),
        on_off(on),
        Done::Same,
    );
    let enabled = relay.task().map(|t| t.enabled);
    if on {
        if relay.pid.is_some() {
            return change;
        }
        change.done = match (&relay.task, enabled) {
            (Err(e), _) => Done::Failed(Why::Failed(e.clone())),
            (Ok(None), _) => Done::Failed(Why::NoTask),
            (Ok(Some(_)), enabled) => done(
                if enabled == Some(false) {
                    sys.enable_task(true).map_err(Why::Failed)
                } else {
                    Ok(())
                }
                .and_then(|()| start(ctx)),
            ),
        };
        return change;
    }
    change.done = match relay.pid {
        None if enabled == Some(true) => done(sys.enable_task(false).map_err(Why::Failed)),
        None => Done::Same,
        Some(_) if relay.ours() => done(stop_ours(ctx).map(|_held| ())),
        Some(_) => {
            if enabled == Some(true) {
                let _ = sys.enable_task(false);
            }
            Done::Failed(Why::NotOurs(relay.by.clone()))
        }
    };
    change
}

/// Stops op-reason's task's relay and runs the task again, holding the lock until the new relay answers.
fn restart(ctx: &Ctx) -> Done {
    done(lock_ours(ctx).and_then(|(_held, pid)| {
        if let Some(pid) = pid {
            ctx.sys.stop_relay(pid).map_err(Why::Failed)?;
        }
        start(ctx)
    }))
}

/// After the relay's file values were set: restarts op-reason's task's relay when its values differ from the file's
/// (or are unknown and the file changed); reports another relay whose values differ. A relay that this `set` started
/// read the file as it was written, and is left alone.
fn restart_for_values(ctx: &Ctx, file_changed: bool, pid_before: Option<u32>) -> Option<Change> {
    let sys = ctx.sys;
    let relay = Relay::read(sys);
    let pid = relay.pid?;
    if pid_before != Some(pid) {
        return None;
    }
    let v = file::values(sys.read_settings().ok().flatten().as_deref());
    let differs = relay.info.as_ref().map(|i| {
        i.mode() != Some(v.mode.unwrap_or_default())
            || i.time_limit != v.time_limit.unwrap_or(values::DEFAULT_TIME_LIMIT)
            || i.answer_wait != v.answer_wait.unwrap_or(values::DEFAULT_ANSWER_WAIT)
    });
    let mut change = Change::new("relay.running", Some("on"), "on", Done::Same);
    if relay.ours() {
        if !differs.unwrap_or(file_changed) {
            return None;
        }
        change.done = restart(ctx);
        change.note = Some(Note::Restarted);
    } else {
        if differs != Some(true) {
            return None;
        }
        change.done = Done::Failed(Why::NotOurs(relay.by.clone()));
    }
    Some(change)
}

/// `relay start|stop|restart`.
pub fn relay(ctx: &Ctx, act: RelayAct) -> Vec<Change> {
    let out = match act {
        RelayAct::Start => vec![running(ctx, true)],
        RelayAct::Stop => vec![running(ctx, false)],
        RelayAct::Restart => {
            let relay = Relay::read(ctx.sys);
            let mut change = Change::new(
                "relay.running",
                Some(on_off(relay.pid.is_some())),
                "on",
                Done::Same,
            );
            change.done = if relay.ours() {
                change.note = Some(Note::Restarted);
                restart(ctx)
            } else if relay.pid.is_some() {
                Done::Failed(Why::NotOurs(relay.by.clone()))
            } else {
                match relay.task() {
                    None => Done::Failed(Why::NoTask),
                    Some(t) => done(
                        if t.enabled {
                            Ok(())
                        } else {
                            ctx.sys.enable_task(true).map_err(Why::Failed)
                        }
                        .and_then(|()| start(ctx)),
                    ),
                }
            };
            vec![change]
        }
    };
    record(ctx, &out);
    out
}

fn skill_set(ctx: &Ctx, name: &Name, place: &Place, on: bool) -> Change {
    let sys = ctx.sys;
    let want = skill::marked(ctx.skill, ctx.version);
    let now = match sys.skill(place, SkillAct::Look, &want) {
        Ok(n) => n,
        Err(e) => {
            return Change::new(&name.text(), None, on_off(on), Done::Failed(Why::Failed(e)));
        }
    };
    let mut change = Change::new(
        &name.text(),
        Some(skill_value(&now.state)),
        on_off(on),
        Done::Same,
    );
    let act = if on {
        match &now.state {
            SkillState::Ok => None,
            SkillState::Foreign => {
                change.done = Done::Failed(Why::Foreign(now.path.clone()));
                None
            }
            _ => Some(SkillAct::Place),
        }
    } else {
        match &now.state {
            SkillState::Missing | SkillState::Foreign => None,
            _ => Some(SkillAct::Remove),
        }
    };
    if let Some(act) = act {
        change.done = match &now.link {
            Some(link) => Done::Failed(Why::Link(link.clone())),
            None => match sys.skill(place, act, &want) {
                Ok(after) => skill_result(act, after),
                Err(e) => Done::Failed(Why::Failed(e)),
            },
        };
    }
    if let (Place::Folder(folder), false) = (place, matches!(change.done, Done::Failed(_)))
        && let Err(e) = remember_folder(ctx, folder, on)
    {
        change.done = Done::Failed(Why::Failed(e));
    }
    change
}

/// Whether a place did what was asked, from the state it reports after it (the WSL script refuses by reporting the
/// state it left, not by an error).
fn skill_result(act: SkillAct, after: SkillNow) -> Done {
    match (&after.link, &after.state, act) {
        (Some(link), _, _) => Done::Failed(Why::Link(link.clone())),
        (None, SkillState::Ok, SkillAct::Place) | (None, SkillState::Missing, SkillAct::Remove) => {
            Done::Done
        }
        (None, SkillState::Foreign, _) => Done::Failed(Why::Foreign(after.path)),
        (None, _, _) => Done::Failed(Why::Failed(format!("{} was left as it was", after.path))),
    }
}

/// Keeps `skill.folders` in step: a folder where the skill is placed is listed once (without regard to case).
fn remember_folder(ctx: &Ctx, folder: &str, on: bool) -> Result<(), String> {
    let listed = |text: Option<&str>| {
        file::values(text)
            .folders
            .iter()
            .any(|f| f.eq_ignore_ascii_case(folder))
    };
    if let Ok(text) = ctx.sys.read_settings()
        && listed(text.as_deref()) == on
    {
        return Ok(());
    }
    ctx.sys
        .write_settings(&mut |current| {
            let mut folders = file::values(current).folders;
            if on && !listed(current) {
                folders.push(folder.to_owned());
            } else if !on {
                folders.retain(|f| !f.eq_ignore_ascii_case(folder));
            }
            file::edit(current, |m| {
                let value = (!folders.is_empty()).then(|| {
                    Value::from(
                        folders
                            .iter()
                            .map(|f| Value::from(f.as_str()))
                            .collect::<Vec<_>>(),
                    )
                });
                file::put(m, &["skill", "folders"], value);
            })
        })
        .map_err(|e| format!("settings.json: {e}"))
}

/// `forget-accounts`.
pub fn forget_accounts(ctx: &Ctx) -> Vec<Change> {
    let n = ctx.sys.accounts();
    let done = match ctx.sys.forget_accounts() {
        Ok(()) if n == 0 => Done::Same,
        Ok(()) => Done::Done,
        Err(e) => Done::Failed(Why::Failed(e)),
    };
    let out = vec![Change::new(
        "records.accounts",
        Some(&n.to_string()),
        "0",
        done,
    )];
    record(ctx, &out);
    out
}

/// `open-records` (not recorded: it changes nothing).
pub fn open_records(ctx: &Ctx) -> Vec<Change> {
    let done = match ctx.sys.open_records() {
        Ok(()) => Done::Done,
        Err(e) => Done::Failed(Why::Failed(e)),
    };
    vec![Change::new("records.folder", None, "open", done)]
}

/// Records the changes that were tried (not those that were already so).
fn record(ctx: &Ctx, changes: &[Change]) {
    let tried: Vec<Change> = changes
        .iter()
        .filter(|c| c.done != Done::Same)
        .cloned()
        .collect();
    if !tried.is_empty() {
        ctx.sys.record(ctx.from, &tried);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::settings::catalog::{Name, Want};
    use crate::settings::fake::{Fake, OURS, info};
    use crate::settings::skill::{SkillNow, SkillState};
    use crate::settings::task::{self, TaskNow};
    use crate::settings::values::RelayMode;

    fn set(f: &Fake, wants: &[(Name, Want)]) -> Vec<Change> {
        super::set(&f.ctx(), wants)
    }

    fn on(name: Name) -> (Name, Want) {
        (name, Want::On(true))
    }

    fn off(name: Name) -> (Name, Want) {
        (name, Want::On(false))
    }

    // --- relay.autostart (design §9.2)

    #[test]
    fn autostart_on_without_a_task_makes_it_and_starts_the_relay() {
        let f = Fake::new();
        let c = set(&f, &[on(Name::RelayAutostart)]);
        assert_eq!(f.calls(), ["register", "run"]);
        assert_eq!(c[0].done, Done::Done);
        assert_eq!(f.pid.get(), Some(OURS));
        assert_eq!(f.records.borrow().len(), 1, "one record per set");
    }

    #[test]
    fn autostart_on_with_another_relay_makes_the_task_turned_off() {
        let f = Fake::new().other_running();
        let c = set(&f, &[on(Name::RelayAutostart)]);
        assert_eq!(f.calls(), ["register"]);
        assert_eq!(c[0].done, Done::Done);
        assert_eq!(
            c[0].note,
            Some(Note::TaskMadeOff(Some(r"\pjp\op-reason-ssh-relay".into())))
        );
        assert!(!f.task.borrow().as_ref().unwrap().enabled);
    }

    #[test]
    fn autostart_on_with_another_exe_restarts_our_relay_with_the_lock() {
        let f = Fake::new().ours_running();
        f.task.borrow_mut().as_mut().unwrap().shape.exe = r"D:\old\op-reason.exe".into();
        let c = set(&f, &[on(Name::RelayAutostart)]);
        assert_eq!(
            f.calls(),
            ["lock", "disable", "stop 100", "register", "run"]
        );
        assert_eq!(c[0].done, Done::Done);
        assert_eq!(
            f.task.borrow().as_ref().unwrap().shape.exe,
            r"C:\op\op-reason.exe"
        );
    }

    #[test]
    fn nothing_is_stopped_without_the_lock() {
        let f = Fake::new().ours_running();
        f.locked.set(true);
        f.task.borrow_mut().as_mut().unwrap().shape.exe = r"D:\old\op-reason.exe".into();
        let c = set(&f, &[on(Name::RelayAutostart)]);
        assert_eq!(f.calls(), ["lock"]);
        assert_eq!(c[0].done, Done::Failed(Why::Locked));
        assert_eq!(f.pid.get(), Some(OURS));
    }

    #[test]
    fn autostart_on_with_the_right_task_turned_off_leaves_it() {
        let f = Fake::new();
        *f.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&f.want),
            enabled: false,
            running: false,
        });
        let c = set(&f, &[on(Name::RelayAutostart)]);
        assert!(f.calls().is_empty());
        assert_eq!(c[0].done, Done::Same);
        assert!(f.records.borrow().is_empty(), "nothing to record");
    }

    // --- relay.running

    #[test]
    fn running_on_without_a_task_says_to_turn_autostart_on() {
        let f = Fake::new();
        let c = set(&f, &[on(Name::RelayRunning)]);
        assert_eq!(c[0].done, Done::Failed(Why::NoTask));
        assert!(f.calls().is_empty());
    }

    #[test]
    fn running_on_while_another_relay_answers_is_the_same() {
        let f = Fake::new().other_running();
        let c = set(&f, &[on(Name::RelayRunning)]);
        assert_eq!(c[0].done, Done::Same);
        assert!(f.calls().is_empty());
    }

    #[test]
    fn running_on_turns_our_task_on_and_runs_it() {
        let f = Fake::new();
        *f.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&f.want),
            enabled: false,
            running: false,
        });
        let c = set(&f, &[on(Name::RelayRunning)]);
        assert_eq!(f.calls(), ["enable", "run"]);
        assert_eq!(c[0].done, Done::Done);
    }

    #[test]
    fn running_on_that_does_not_answer_fails() {
        let f = Fake::new();
        f.starts.set(false);
        *f.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&f.want),
            enabled: true,
            running: false,
        });
        let c = set(&f, &[on(Name::RelayRunning)]);
        assert_eq!(c[0].done, Done::Failed(Why::NoAnswer));
    }

    #[test]
    fn running_off_turns_the_task_off_and_stops_our_relay() {
        let f = Fake::new().ours_running();
        let c = set(&f, &[off(Name::RelayRunning)]);
        assert_eq!(f.calls(), ["lock", "disable", "stop 100"]);
        assert_eq!(c[0].done, Done::Done);
        assert!(!f.task.borrow().as_ref().unwrap().enabled);
    }

    #[test]
    fn running_off_never_stops_another_relay_but_turns_our_task_off() {
        let f = Fake::new().other_running();
        *f.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&f.want),
            enabled: true,
            running: false,
        });
        let c = set(&f, &[off(Name::RelayRunning)]);
        assert_eq!(f.calls(), ["disable"]);
        assert_eq!(
            c[0].done,
            Done::Failed(Why::NotOurs(Some(r"\pjp\op-reason-ssh-relay".into())))
        );
        assert_eq!(f.pid.get(), Some(7));
    }

    #[test]
    fn running_off_without_a_relay_turns_the_task_off() {
        let f = Fake::new();
        *f.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&f.want),
            enabled: true,
            running: false,
        });
        let c = set(&f, &[off(Name::RelayRunning)]);
        assert_eq!(f.calls(), ["disable"]);
        assert_eq!(c[0].done, Done::Done);
    }

    #[test]
    fn restart_holds_the_lock_and_runs_the_task_again() {
        let f = Fake::new().ours_running();
        let c = relay(&f.ctx(), RelayAct::Restart);
        assert_eq!(f.calls(), ["lock", "stop 100", "run"]);
        assert_eq!(c[0].done, Done::Done);
        let f = Fake::new().other_running();
        let c = relay(&f.ctx(), RelayAct::Restart);
        assert!(matches!(c[0].done, Done::Failed(Why::NotOurs(_))));
        assert!(f.calls().is_empty());
    }

    // --- what the final review found

    #[test]
    fn a_relay_that_answers_while_our_task_runs_another_instance_is_not_ours() {
        // op-reason's task is on while another relay runs: its instance of every minute starts op-reason.exe, which
        // exits at once; for that moment the task is Running, with another process than the relay's
        let f = Fake::new().other_running();
        *f.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&f.want),
            enabled: true,
            running: true,
        });
        let c = relay(&f.ctx(), RelayAct::Stop);
        assert!(matches!(c[0].done, Done::Failed(Why::NotOurs(_))), "{c:?}");
        assert!(
            !f.calls().iter().any(|c| c.starts_with("stop")),
            "{:?}",
            f.calls()
        );
        let c = relay(&f.ctx(), RelayAct::Restart);
        assert!(matches!(c[0].done, Done::Failed(Why::NotOurs(_))), "{c:?}");
        set(&f, &[off(Name::RelayAutostart)]);
        assert!(
            !f.calls().iter().any(|c| c.starts_with("stop")),
            "{:?}",
            f.calls()
        );
        assert_eq!(f.pid.get(), Some(7));
    }

    #[test]
    fn a_relay_without_the_mark_of_our_task_is_not_ours() {
        // Started by hand with the same pipe, or by another task: relay.json has no task, or another one
        for task in [None, Some("op-reason-test-other".to_owned())] {
            let f = Fake::new().ours_running();
            f.info.borrow_mut().as_mut().unwrap().task = task.clone();
            let c = relay(&f.ctx(), RelayAct::Stop);
            assert!(
                matches!(c[0].done, Done::Failed(Why::NotOurs(_))),
                "{task:?}: {c:?}"
            );
            assert!(
                !f.calls().iter().any(|c| c.starts_with("stop")),
                "{:?}",
                f.calls()
            );
        }
    }

    #[test]
    fn the_relay_is_read_again_under_the_lock() {
        let f = Fake::new().ours_running();
        f.swap_on_lock.set(Some(7));
        let c = relay(&f.ctx(), RelayAct::Stop);
        assert!(matches!(c[0].done, Done::Failed(Why::NotOurs(_))), "{c:?}");
        assert!(
            !f.calls().iter().any(|c| c.starts_with("stop")),
            "{:?}",
            f.calls()
        );
        assert!(
            f.task.borrow().as_ref().unwrap().enabled,
            "the task is left as it was"
        );
    }

    #[test]
    fn set_makes_autostart_before_running_whatever_the_order() {
        let f = Fake::new();
        let c = set(&f, &[on(Name::RelayRunning), on(Name::RelayAutostart)]);
        assert!(
            c.iter().all(|c| !matches!(c.done, Done::Failed(_))),
            "{c:?}"
        );
        assert_eq!(f.calls(), ["register", "run"]);
        assert_eq!(f.pid.get(), Some(OURS));
    }

    #[test]
    fn autostart_off_turns_the_task_off_before_it_stops_the_relay() {
        let f = Fake::new().ours_running();
        set(&f, &[off(Name::RelayAutostart)]);
        assert_eq!(f.calls(), ["lock", "disable", "stop 100", "remove"]);
    }

    #[test]
    fn autostart_off_without_the_lock_touches_nothing() {
        let f = Fake::new().ours_running();
        f.locked.set(true);
        let c = set(&f, &[off(Name::RelayAutostart)]);
        assert_eq!(c[0].done, Done::Failed(Why::Locked));
        assert_eq!(f.calls(), ["lock"]);
        assert!(f.task.borrow().as_ref().unwrap().enabled);
    }

    #[test]
    fn autostart_off_with_another_relay_removes_the_task_only() {
        let f = Fake::new().other_running();
        *f.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&f.want),
            enabled: true,
            running: false,
        });
        let c = set(&f, &[off(Name::RelayAutostart)]);
        assert_eq!(c[0].done, Done::Done);
        assert_eq!(f.calls(), ["disable", "remove"]);
        assert_eq!(f.pid.get(), Some(7));
    }

    #[test]
    fn a_task_made_again_is_turned_off_before_its_relay_stops() {
        let f = Fake::new().ours_running();
        f.task.borrow_mut().as_mut().unwrap().shape.exe = r"D:\old\op-reason.exe".into();
        set(&f, &[on(Name::RelayAutostart)]);
        assert_eq!(
            f.calls(),
            ["lock", "disable", "stop 100", "register", "run"]
        );
        assert!(f.task.borrow().as_ref().unwrap().enabled, "made turned on");
    }

    #[test]
    fn with_another_relay_the_task_is_made_turned_off_in_one_step() {
        let f = Fake::new().other_running();
        set(&f, &[on(Name::RelayAutostart)]);
        assert_eq!(f.calls(), ["register"], "no window in which it is on");
        assert!(f.xml.borrow().contains("<Enabled>false</Enabled>"));
        assert!(!f.task.borrow().as_ref().unwrap().enabled);
    }

    #[test]
    fn a_relay_started_by_the_same_set_is_not_started_again() {
        let f = Fake::new();
        f.info_late.set(true);
        set(
            &f,
            &[
                on(Name::RelayAutostart),
                (Name::RelayMode, Want::Mode(RelayMode::ShowOnly)),
            ],
        );
        assert_eq!(
            f.calls(),
            ["register", "run"],
            "it read the new values when it started"
        );
    }

    #[test]
    fn a_mode_change_without_the_lock_writes_the_file_and_says_so() {
        let f = Fake::new().ours_running();
        f.locked.set(true);
        let c = set(&f, &[(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))]);
        assert_eq!(c[0].done, Done::Done);
        assert_eq!(c[1].done, Done::Failed(Why::Locked));
        assert_eq!(f.calls(), ["lock"]);
        assert_eq!(f.pid.get(), Some(OURS));
    }

    #[test]
    fn a_place_that_refuses_without_an_error_is_not_done() {
        let f = Fake::new();
        f.refuse_quietly.set(true);
        let c = set(&f, &[on(Name::SkillWsl("Ubuntu".into()))]);
        assert!(matches!(c[0].done, Done::Failed(_)), "{c:?}");
        f.skills.borrow_mut().get_mut("wsl:Ubuntu").unwrap().state = SkillState::Ok;
        let c = set(&f, &[off(Name::SkillWsl("Ubuntu".into()))]);
        assert!(matches!(c[0].done, Done::Failed(_)), "{c:?}");
    }

    #[test]
    fn a_folder_that_cannot_be_remembered_is_not_done() {
        let f = Fake::new();
        f.fail_write.set(true);
        let c = set(&f, &[on(Name::SkillFolder(r"D:\agents".into()))]);
        assert!(matches!(c[0].done, Done::Failed(Why::Failed(_))), "{c:?}");
    }

    #[test]
    fn a_broken_file_is_named_on_its_settings() {
        let f = Fake::new();
        *f.settings.borrow_mut() = Some("{broken".into());
        let i = get(&f.ctx(), &Name::RelayMode);
        assert_eq!(i.value.as_deref(), Some("per-call"));
        assert!(matches!(i.state, State::Ignored(_)), "{:?}", i.state);
    }

    // --- the relay's file values

    #[test]
    fn a_mode_change_restarts_our_relay() {
        let f = Fake::new().ours_running();
        let c = set(&f, &[(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))]);
        assert_eq!(f.calls(), ["lock", "stop 100", "run"]);
        assert_eq!(c[0].done, Done::Done);
        assert_eq!(c[1].name, "relay.running");
        assert_eq!(c[1].note, Some(Note::Restarted));
        assert_eq!(f.info.borrow().as_ref().unwrap().mode, "show-only");
    }

    #[test]
    fn set_restarts_when_the_relay_differs_even_if_the_file_does_not_change() {
        let f = Fake::new().ours_running();
        *f.settings.borrow_mut() = Some(r#"{"v":1,"relay":{"mode":"show-only"}}"#.into());
        let c = set(&f, &[(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))]);
        assert_eq!(c[0].done, Done::Same, "the file already says show-only");
        assert_eq!(
            f.calls(),
            ["lock", "stop 100", "run"],
            "the relay ran per-call"
        );
    }

    #[test]
    fn a_relay_that_does_not_tell_its_values_is_left_alone() {
        // A relay of 0.6.0 (no relay.json): never started by op-reason's task, which passes --task to relays that write
        // relay.json
        let f = Fake::new().ours_running();
        *f.info.borrow_mut() = None;
        *f.settings.borrow_mut() = Some(r#"{"relay":{"mode":"show-only"}}"#.into());
        set(&f, &[(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))]);
        assert!(f.calls().is_empty(), "the file did not change");
        let checked = check(
            &f.ctx(),
            &[(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))],
        );
        assert!(
            checked[0].same,
            "a relay of 0.6.0 does not make it different"
        );
        let c = set(&f, &[(Name::RelayMode, Want::Mode(RelayMode::PerCall))]);
        assert!(f.calls().is_empty(), "not op-reason's: never stopped");
        assert_eq!(
            c.len(),
            1,
            "and its values are unknown, so nothing to report: {c:?}"
        );
    }

    #[test]
    fn another_relay_with_other_values_is_reported_not_restarted() {
        let f = Fake::new().other_running();
        *f.info.borrow_mut() = Some(info(7, "per-call"));
        let c = set(&f, &[(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))]);
        assert_eq!(c[0].done, Done::Done, "the file is written");
        assert!(matches!(c[1].done, Done::Failed(Why::NotOurs(_))));
        assert!(!f.calls().iter().any(|c| c.starts_with("stop")));
    }

    #[test]
    fn file_values_go_in_one_write() {
        let f = Fake::new();
        *f.settings.borrow_mut() = Some(r#"{"v":1,"theme":1}"#.into());
        set(
            &f,
            &[
                (Name::Lang, Want::Lang(Some(Lang::Ja))),
                (Name::RelayMode, Want::Mode(RelayMode::ShowOnly)),
                (Name::RelayTimeLimit, Want::Secs(90)),
            ],
        );
        assert_eq!(f.writes.get(), 1);
        assert_eq!(
            f.settings.borrow().as_deref(),
            Some(r#"{"v":1,"lang":"ja","relay":{"mode":"show-only","timeLimit":90},"theme":1}"#)
        );
        set(
            &f,
            &[
                (Name::RelayMode, Want::Default),
                (Name::Lang, Want::Lang(None)),
            ],
        );
        assert_eq!(
            f.settings.borrow().as_deref(),
            Some(r#"{"v":1,"relay":{"timeLimit":90},"theme":1}"#)
        );
    }

    // --- check, get, list

    #[test]
    fn check_compares_with_the_values_now() {
        let f = Fake::new().ours_running();
        let c = check(
            &f.ctx(),
            &[
                on(Name::RelayAutostart),
                on(Name::RelayRunning),
                (Name::RelayMode, Want::Mode(RelayMode::PerCall)),
                (Name::RelayTimeLimit, Want::Default),
            ],
        );
        assert!(c.iter().all(|c| c.same), "{c:?}");
        *f.settings.borrow_mut() = Some(r#"{"relay":{"mode":"show-only"}}"#.into());
        let c = check(
            &f.ctx(),
            &[(Name::RelayMode, Want::Mode(RelayMode::ShowOnly))],
        );
        assert!(!c[0].same);
        assert_eq!(
            c[0].item.state,
            State::RelayDiffers {
                relay: "per-call".into(),
                pid: OURS
            }
        );
        assert_eq!(
            c[0].item.value.as_deref(),
            Some("show-only"),
            "the file's value"
        );
    }

    #[test]
    fn running_off_is_different_while_our_task_would_start_it() {
        let f = Fake::new();
        *f.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&f.want),
            enabled: true,
            running: false,
        });
        let c = check(&f.ctx(), &[off(Name::RelayRunning)]);
        assert!(!c[0].same);
        assert_eq!(c[0].item.state, State::TaskStarting);
    }

    #[test]
    fn autostart_warns_while_another_relay_answers() {
        let f = Fake::new().other_running();
        *f.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&f.want),
            enabled: true,
            running: false,
        });
        let i = get(&f.ctx(), &Name::RelayAutostart);
        assert_eq!(i.value.as_deref(), Some("on"));
        assert_eq!(
            i.state,
            State::OtherRelay {
                pid: 7,
                by: Some(r"\pjp\op-reason-ssh-relay".into())
            }
        );
        let i = get(&f.ctx(), &Name::RelayRunning);
        assert_eq!(i.value.as_deref(), Some("on"));
    }

    #[test]
    fn a_task_of_another_shape_is_named() {
        let f = Fake::new();
        let mut shape = task::wanted(&f.want);
        shape.priority = 7;
        *f.task.borrow_mut() = Some(TaskNow {
            shape,
            enabled: true,
            running: false,
        });
        let c = check(&f.ctx(), &[on(Name::RelayAutostart)]);
        assert!(!c[0].same);
        assert_eq!(c[0].item.state, State::TaskDiffers(vec!["priority"]));
    }

    #[test]
    fn list_never_starts_a_distribution() {
        let f = Fake::new();
        *f.settings.borrow_mut() = Some(r#"{"skill":{"folders":["D:\\a"]}}"#.into());
        let items = list(&f.ctx());
        let names: Vec<String> = items.iter().map(|i| i.name.text()).collect();
        assert_eq!(
            names,
            [
                "lang",
                "relay.mode",
                "relay.time-limit",
                "relay.answer-wait",
                "relay.autostart",
                "relay.running",
                "skill.windows",
                "skill.wsl.Ubuntu",
                "skill.wsl.Debian",
                r"skill.folder.D:\a",
            ]
        );
        let debian = items
            .iter()
            .find(|i| i.name.text() == "skill.wsl.Debian")
            .unwrap();
        assert_eq!(debian.state, State::NotRunning);
        assert!(!f.calls().iter().any(|c| c.contains("Debian")));
        assert!(f.calls().contains(&"skill wsl:Ubuntu Look".to_owned()));
        // named, it is looked at (wsl.exe starts it)
        get(&f.ctx(), &Name::SkillWsl("Debian".into()));
        assert!(f.calls().contains(&"skill wsl:Debian Look".to_owned()));
    }

    #[test]
    fn the_language_alone_reads_no_task() {
        let f = Fake::new().ours_running();
        let i = get(&f.ctx(), &Name::Lang);
        assert_eq!(i.value.as_deref(), Some("default"));
        assert_eq!(
            f.task_reads.get(),
            0,
            "Task Scheduler is not asked for the language"
        );
        get(&f.ctx(), &Name::RelayMode);
        assert_eq!(f.task_reads.get(), 1);
    }

    #[test]
    fn list_names_the_other_tasks_that_start_a_relay() {
        let f = Fake::new();
        f.others.borrow_mut().push(OtherTask {
            path: r"\pjp\op-reason-ssh-relay".into(),
            running: false,
            enabled: true,
        });
        let items = list(&f.ctx());
        let autostart = items
            .iter()
            .find(|i| i.name == Name::RelayAutostart)
            .unwrap();
        assert_eq!(autostart.value.as_deref(), Some("off"));
        assert_eq!(
            autostart.state,
            State::OtherTasks(vec![r"\pjp\op-reason-ssh-relay".into()])
        );
        let i = get(&f.ctx(), &Name::RelayAutostart);
        assert_eq!(i.state, State::Ok, "get does not walk every task folder");
    }

    // --- skills (design §10.1)

    #[test]
    fn a_skill_is_placed_updated_and_removed() {
        let f = Fake::new();
        let c = set(&f, &[on(Name::SkillWindows)]);
        assert_eq!(c[0].done, Done::Done);
        assert_eq!(f.calls(), ["skill windows Look", "skill windows Place"]);
        f.skills.borrow_mut().get_mut("windows").unwrap().state = SkillState::Old("0.6.0".into());
        let c = check(&f.ctx(), &[on(Name::SkillWindows)]);
        assert!(!c[0].same, "an old copy is not the skill of this version");
        assert_eq!(set(&f, &[on(Name::SkillWindows)])[0].done, Done::Done);
        assert_eq!(set(&f, &[off(Name::SkillWindows)])[0].done, Done::Done);
        assert_eq!(f.skills.borrow()["windows"].state, SkillState::Missing);
    }

    #[test]
    fn a_foreign_skill_is_refused_for_on_and_the_same_for_off() {
        let f = Fake::new();
        f.skills.borrow_mut().insert(
            "windows".into(),
            SkillNow {
                state: SkillState::Foreign,
                link: None,
                path: "p".into(),
            },
        );
        let c = set(&f, &[on(Name::SkillWindows)]);
        assert_eq!(c[0].done, Done::Failed(Why::Foreign("p".into())));
        let c = set(&f, &[off(Name::SkillWindows)]);
        assert_eq!(c[0].done, Done::Same);
        assert!(
            !f.calls()
                .iter()
                .any(|c| c.ends_with("Place") || c.ends_with("Remove"))
        );
    }

    #[test]
    fn a_link_is_refused_unless_nothing_is_there() {
        let f = Fake::new();
        f.skills.borrow_mut().insert(
            "wsl:Ubuntu".into(),
            SkillNow {
                state: SkillState::Missing,
                link: Some("/h/.claude/skills".into()),
                path: "p".into(),
            },
        );
        let name = Name::SkillWsl("Ubuntu".into());
        let c = set(&f, &[on(name.clone())]);
        assert_eq!(
            c[0].done,
            Done::Failed(Why::Link("/h/.claude/skills".into()))
        );
        assert_eq!(set(&f, &[off(name.clone())])[0].done, Done::Same);
        f.skills.borrow_mut().get_mut("wsl:Ubuntu").unwrap().state = SkillState::Ok;
        assert_eq!(
            set(&f, &[off(name)])[0].done,
            Done::Failed(Why::Link("/h/.claude/skills".into()))
        );
    }

    #[test]
    fn a_chosen_folder_is_remembered_and_forgotten() {
        let f = Fake::new();
        let name = Name::SkillFolder(r"D:\agents".into());
        set(&f, &[on(name.clone())]);
        assert_eq!(
            crate::settings::values(f.settings.borrow().as_deref()).folders,
            [r"D:\agents"]
        );
        set(&f, &[on(Name::SkillFolder(r"d:\AGENTS".into()))]);
        assert_eq!(
            crate::settings::values(f.settings.borrow().as_deref())
                .folders
                .len(),
            1,
            "the same folder without regard to case"
        );
        set(&f, &[off(name)]);
        assert!(
            crate::settings::values(f.settings.borrow().as_deref())
                .folders
                .is_empty()
        );
    }

    // --- records

    #[test]
    fn accounts_are_forgotten_and_recorded() {
        let f = Fake::new();
        let c = forget_accounts(&f.ctx());
        assert_eq!(c[0].done, Done::Done);
        assert_eq!(f.calls(), ["forget"]);
        assert_eq!(f.records.borrow().len(), 1);
        open_records(&f.ctx());
        assert_eq!(
            f.records.borrow().len(),
            1,
            "opening a folder changes nothing"
        );
    }

    #[test]
    fn a_set_records_what_it_tried() {
        let f = Fake::new();
        set(
            &f,
            &[
                (Name::RelayMode, Want::Mode(RelayMode::ShowOnly)),
                on(Name::RelayRunning),
            ],
        );
        let records = f.records.borrow();
        assert_eq!(records.len(), 1);
        let names: Vec<&str> = records[0].iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["relay.mode", "relay.running"]);
        assert_eq!(records[0][1].done, Done::Failed(Why::NoTask));
    }
}

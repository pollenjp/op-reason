//! A system in memory for the tests of the settings (the engine and the command line).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;

use crate::settings::engine::{Change, Ctx, OtherTask, Place, System};
use crate::settings::relayinfo::RelayInfo;
use crate::settings::skill::{SkillAct, SkillNow, SkillState};
use crate::settings::task::{self, TaskNow, TaskWant};
use crate::settings::wsl::Distro;

pub(crate) const OURS: u32 = 100;

/// A system in memory: the task, the relay, the lock, the skill places and the calls made.
pub(crate) struct Fake {
    pub(crate) settings: RefCell<Option<String>>,
    pub(crate) writes: Cell<usize>,
    pub(crate) task: RefCell<Option<TaskNow>>,
    pub(crate) pid: Cell<Option<u32>>,
    pub(crate) info: RefCell<Option<RelayInfo>>,
    pub(crate) others: RefCell<Vec<OtherTask>>,
    pub(crate) locked: Cell<bool>,
    pub(crate) starts: Cell<bool>,
    pub(crate) calls: RefCell<Vec<String>>,
    pub(crate) skills: RefCell<HashMap<String, SkillNow>>,
    pub(crate) distros: Vec<Distro>,
    pub(crate) records: RefCell<Vec<Vec<Change>>>,
    pub(crate) want: TaskWant,
    pub(crate) task_reads: Cell<usize>,
    /// When set, every skill call fails with this message.
    pub(crate) fail_skill: RefCell<Option<String>>,
}

impl Fake {
    pub(crate) fn new() -> Fake {
        Fake {
            settings: RefCell::new(None),
            writes: Cell::new(0),
            task: RefCell::new(None),
            pid: Cell::new(None),
            info: RefCell::new(None),
            others: RefCell::new(Vec::new()),
            locked: Cell::new(false),
            starts: Cell::new(true),
            calls: RefCell::new(Vec::new()),
            skills: RefCell::new(HashMap::new()),
            distros: vec![
                Distro {
                    name: "Ubuntu".into(),
                    running: true,
                },
                Distro {
                    name: "Debian".into(),
                    running: false,
                },
            ],
            records: RefCell::new(Vec::new()),
            want: TaskWant {
                exe: r"C:\op\op-reason.exe".into(),
                sid: "S-1-5-21-9".into(),
            },
            task_reads: Cell::new(0),
            fail_skill: RefCell::new(None),
        }
    }

    /// op-reason's task, made as wanted, on, and running its relay with the file's values.
    pub(crate) fn ours_running(self) -> Fake {
        *self.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&self.want),
            enabled: true,
            running: true,
        });
        self.pid.set(Some(OURS));
        *self.info.borrow_mut() = Some(info(OURS, "per-call"));
        self
    }

    /// A relay that another task started.
    pub(crate) fn other_running(self) -> Fake {
        self.pid.set(Some(7));
        self.others.borrow_mut().push(OtherTask {
            path: r"\pjp\op-reason-ssh-relay".into(),
            running: true,
            enabled: true,
        });
        self
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn call(&self, c: &str) {
        self.calls.borrow_mut().push(c.to_owned());
    }

    pub(crate) fn ctx(&self) -> Ctx<'_> {
        Ctx {
            sys: self,
            version: "0.7.0",
            skill: "---\nname: op-reason\n---\nbody\n",
            from: "cli",
        }
    }
}

pub(crate) fn info(pid: u32, mode: &str) -> RelayInfo {
    RelayInfo {
        v: 1,
        pid,
        exe: r"C:\op\op-reason.exe".into(),
        pipe: crate::settings::values::RELAY_PIPE.into(),
        mode: mode.into(),
        time_limit: 3600,
        answer_wait: 60,
        args: vec!["--ssh-agent".into()],
    }
}

fn key(place: &Place) -> String {
    match place {
        Place::Windows => "windows".into(),
        Place::Wsl(d) => format!("wsl:{d}"),
        Place::Folder(f) => format!("folder:{f}"),
    }
}

impl System for Fake {
    fn read_settings(&self) -> io::Result<Option<String>> {
        Ok(self.settings.borrow().clone())
    }
    fn write_settings(&self, edit: &mut dyn FnMut(Option<&str>) -> String) -> io::Result<()> {
        let old = self.settings.borrow().clone();
        *self.settings.borrow_mut() = Some(edit(old.as_deref()));
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }
    fn settings_where(&self) -> String {
        "settings.json".into()
    }
    fn task_where(&self) -> String {
        r"\op-reason-ssh-relay-S-1-5-21-9".into()
    }
    fn task(&self) -> Result<Option<TaskNow>, String> {
        self.task_reads.set(self.task_reads.get() + 1);
        Ok(self.task.borrow().clone())
    }
    fn task_want(&self) -> Result<TaskWant, String> {
        Ok(self.want.clone())
    }
    fn same_user(&self, a: &str, b: &str) -> bool {
        a == b
    }
    fn now_local(&self) -> String {
        "2026-11-23T10:00:00".into()
    }
    fn register_task(&self, xml: &str) -> Result<(), String> {
        assert!(xml.contains("<Arguments>--ssh-agent</Arguments>"));
        self.call("register");
        *self.task.borrow_mut() = Some(TaskNow {
            shape: task::wanted(&self.want),
            enabled: true,
            running: false,
        });
        Ok(())
    }
    fn remove_task(&self) -> Result<(), String> {
        self.call("remove");
        *self.task.borrow_mut() = None;
        Ok(())
    }
    fn enable_task(&self, on: bool) -> Result<(), String> {
        self.call(if on { "enable" } else { "disable" });
        if let Some(t) = self.task.borrow_mut().as_mut() {
            t.enabled = on;
        }
        Ok(())
    }
    fn run_task(&self) -> Result<(), String> {
        self.call("run");
        if self.starts.get() && self.pid.get().is_none() {
            self.pid.set(Some(OURS));
            if let Some(t) = self.task.borrow_mut().as_mut() {
                t.running = true;
            }
            let values = crate::settings::values(self.settings.borrow().as_deref());
            let mode = values.mode.unwrap_or_default();
            *self.info.borrow_mut() = Some(info(OURS, mode.as_str()));
        }
        Ok(())
    }
    fn other_tasks(&self, _all: bool) -> Vec<OtherTask> {
        self.others.borrow().clone()
    }
    fn relay_pid(&self) -> Option<u32> {
        self.pid.get()
    }
    fn relay_info(&self) -> Option<RelayInfo> {
        self.info.borrow().clone()
    }
    fn lock(&self) -> Option<Box<dyn std::any::Any>> {
        self.call("lock");
        (!self.locked.get()).then(|| Box::new(()) as Box<dyn std::any::Any>)
    }
    fn stop_relay(&self, pid: u32) -> Result<(), String> {
        self.call(&format!("stop {pid}"));
        self.pid.set(None);
        *self.info.borrow_mut() = None;
        if let Some(t) = self.task.borrow_mut().as_mut() {
            t.running = false;
        }
        Ok(())
    }
    fn wait_relay(&self) -> Option<u32> {
        self.pid.get()
    }
    fn skill(&self, place: &Place, act: SkillAct, _want: &str) -> Result<SkillNow, String> {
        let k = key(place);
        self.call(&format!("skill {k} {act:?}"));
        if let Some(e) = self.fail_skill.borrow().clone() {
            return Err(e);
        }
        let mut skills = self.skills.borrow_mut();
        let now = skills.entry(k).or_insert(SkillNow {
            state: SkillState::Missing,
            link: None,
            path: "p".into(),
        });
        match act {
            SkillAct::Look => {}
            SkillAct::Place => {
                if now.link.is_some() || now.state == SkillState::Foreign {
                    return Err("refused".into());
                }
                now.state = SkillState::Ok;
            }
            SkillAct::Remove => {
                if now.link.is_some() || now.state == SkillState::Foreign {
                    return Err("refused".into());
                }
                now.state = SkillState::Missing;
            }
        }
        Ok(now.clone())
    }
    fn distributions(&self) -> Result<Vec<Distro>, String> {
        Ok(self.distros.clone())
    }
    fn records_where(&self) -> String {
        "records".into()
    }
    fn accounts(&self) -> usize {
        2
    }
    fn forget_accounts(&self) -> Result<(), String> {
        self.call("forget");
        Ok(())
    }
    fn open_records(&self) -> Result<(), String> {
        self.call("open");
        Ok(())
    }
    fn record(&self, from: &str, changes: &[Change]) {
        assert_eq!(from, "cli");
        self.records.borrow_mut().push(changes.to_vec());
    }
}

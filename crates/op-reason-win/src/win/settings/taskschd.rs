//! op-reason's scheduled task through Task Scheduler's COM API (design `2026-11-23-settings-design.md` §9): made from
//! the core's XML, read back into the core's shape, turned on and off, run, removed; and the other tasks that start a
//! relay.

use std::path::Path;

use op_reason_core::settings::engine::OtherTask;
use op_reason_core::settings::task::{TaskNow, TaskShape, Trigger};
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::TaskScheduler::{
    IExecAction, ILogonTrigger, IRegisteredTask, ITaskDefinition, ITaskFolder, ITaskService,
    TASK_ACTION_EXEC, TASK_CREATE_OR_UPDATE, TASK_ENUM_HIDDEN, TASK_INSTANCES_IGNORE_NEW,
    TASK_INSTANCES_PARALLEL, TASK_INSTANCES_QUEUE, TASK_INSTANCES_STOP_EXISTING,
    TASK_LOGON_INTERACTIVE_TOKEN, TASK_LOGON_INTERACTIVE_TOKEN_OR_PASSWORD, TASK_LOGON_PASSWORD,
    TASK_LOGON_S4U, TASK_LOGON_SERVICE_ACCOUNT, TASK_RUNLEVEL_HIGHEST, TASK_STATE_RUNNING,
    TASK_TRIGGER_LOGON, TASK_TRIGGER_TIME, TaskScheduler,
};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::core::{BSTR, Interface};

/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)` and `(ERROR_PATH_NOT_FOUND)`: no such task or folder.
const NOT_FOUND: [i32; 2] = [0x8007_0002_u32 as i32, 0x8007_0003_u32 as i32];

fn err(what: &str, e: &windows::core::Error) -> String {
    format!("Task Scheduler: {what}: {}", e.message())
}

/// A connection to Task Scheduler and its root folder.
pub struct Scheduler {
    service: ITaskService,
    root: ITaskFolder,
}

/// A VARIANT holding an `i32` (the index of a collection, from 1).
fn int(n: i32) -> VARIANT {
    let mut v = VARIANT::default();
    // SAFETY: writes the tag and the matching member of a zeroed VARIANT.
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = n;
    }
    v
}

impl Scheduler {
    /// Connects to the local Task Scheduler as the current user (COM must be initialized).
    pub fn connect() -> Result<Scheduler, String> {
        // SAFETY: COM calls with valid arguments.
        unsafe {
            let service: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| err("start", &e))?;
            let empty = VARIANT::default();
            service
                .Connect(&empty, &empty, &empty, &empty)
                .map_err(|e| err("connect", &e))?;
            let root = service
                .GetFolder(&BSTR::from("\\"))
                .map_err(|e| err("root folder", &e))?;
            Ok(Scheduler { service, root })
        }
    }

    fn registered(&self, name: &str) -> Result<Option<IRegisteredTask>, String> {
        // SAFETY: a COM call with a valid name.
        match unsafe { self.root.GetTask(&BSTR::from(name)) } {
            Ok(t) => Ok(Some(t)),
            Err(e) if NOT_FOUND.contains(&e.code().0) => Ok(None),
            Err(e) => Err(err(name, &e)),
        }
    }

    /// The task `name` of the root folder, as the settings compare it.
    pub fn get(&self, name: &str) -> Result<Option<TaskNow>, String> {
        let Some(task) = self.registered(name)? else {
            return Ok(None);
        };
        // SAFETY: COM calls on a live task.
        unsafe {
            let enabled = task.Enabled().map_err(|e| err(name, &e))?.as_bool();
            let running = task.State().map_err(|e| err(name, &e))? == TASK_STATE_RUNNING;
            let def = task.Definition().map_err(|e| err(name, &e))?;
            // The processes its running instances started: the relay that answers is op-reason's only when it is one
            // of them (design §9.5)
            let mut pids = Vec::new();
            if let Ok(instances) = task.GetInstances(0) {
                let n = instances.Count().unwrap_or(0);
                for i in 1..=n {
                    if let Ok(pid) = instances.get_Item(&int(i)).and_then(|r| r.EnginePID())
                        && pid != 0
                    {
                        pids.push(pid);
                    }
                }
            }
            Ok(Some(TaskNow {
                shape: shape(&def).map_err(|e| err(name, &e))?,
                enabled,
                running,
                pids,
            }))
        }
    }

    /// Makes (or makes again) the task `name` from its XML, with the user's interactive token.
    pub fn register(&self, name: &str, xml: &str) -> Result<(), String> {
        let empty = VARIANT::default();
        // SAFETY: COM calls with valid arguments.
        unsafe {
            self.root
                .RegisterTask(
                    &BSTR::from(name),
                    &BSTR::from(xml),
                    TASK_CREATE_OR_UPDATE.0,
                    &empty,
                    &empty,
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &empty,
                )
                .map(|_| ())
                .map_err(|e| err(name, &e))
        }
    }

    /// Removes the task `name`.
    pub fn delete(&self, name: &str) -> Result<(), String> {
        // SAFETY: a COM call with a valid name.
        unsafe { self.root.DeleteTask(&BSTR::from(name), 0) }.map_err(|e| err(name, &e))
    }

    /// Turns the task `name` on or off.
    pub fn set_enabled(&self, name: &str, on: bool) -> Result<(), String> {
        let task = self
            .registered(name)?
            .ok_or_else(|| format!("Task Scheduler: {name}: not there"))?;
        // SAFETY: a COM call on a live task.
        unsafe { task.SetEnabled(VARIANT_BOOL::from(on)) }.map_err(|e| err(name, &e))
    }

    /// Runs the task `name` now.
    pub fn run(&self, name: &str) -> Result<(), String> {
        let task = self
            .registered(name)?
            .ok_or_else(|| format!("Task Scheduler: {name}: not there"))?;
        // SAFETY: a COM call on a live task.
        unsafe { task.Run(&VARIANT::default()) }
            .map(|_| ())
            .map_err(|e| err(name, &e))
    }

    /// Tasks other than `ours` whose action starts `op-reason.exe --ssh-agent`: the running ones, or every one
    /// (`all`: a walk through every folder the user may read).
    pub fn relay_tasks(&self, ours: &str, all: bool) -> Vec<OtherTask> {
        let mut out = Vec::new();
        if all {
            self.walk(&self.root, ours, &mut out, 0);
            return out;
        }
        // SAFETY: COM calls on live objects; items are taken from 1 to the count.
        unsafe {
            let Ok(running) = self.service.GetRunningTasks(TASK_ENUM_HIDDEN.0) else {
                return out;
            };
            let n = running.Count().unwrap_or(0);
            for i in 1..=n {
                let Ok(r) = running.get_Item(&int(i)) else {
                    continue;
                };
                let Ok(path) = r.Path() else { continue };
                let path = path.to_string();
                if is_ours(&path, ours) {
                    continue;
                }
                if let Ok(task) = self.root.GetTask(&BSTR::from(path.as_str()))
                    && let Some(t) = other(&task, &path)
                {
                    out.push(t);
                }
            }
        }
        out
    }

    fn walk(&self, folder: &ITaskFolder, ours: &str, out: &mut Vec<OtherTask>, depth: usize) {
        if depth > 16 {
            return;
        }
        // SAFETY: COM calls on live objects; items are taken from 1 to the count.
        unsafe {
            if let Ok(tasks) = folder.GetTasks(TASK_ENUM_HIDDEN.0) {
                let n = tasks.Count().unwrap_or(0);
                for i in 1..=n {
                    let Ok(task) = tasks.get_Item(&int(i)) else {
                        continue;
                    };
                    let Ok(path) = task.Path() else { continue };
                    let path = path.to_string();
                    if !is_ours(&path, ours)
                        && let Some(t) = other(&task, &path)
                    {
                        out.push(t);
                    }
                }
            }
            if let Ok(folders) = folder.GetFolders(0) {
                let n = folders.Count().unwrap_or(0);
                for i in 1..=n {
                    if let Ok(f) = folders.get_Item(&int(i)) {
                        self.walk(&f, ours, out, depth + 1);
                    }
                }
            }
        }
    }
}

fn is_ours(path: &str, ours: &str) -> bool {
    path.trim_start_matches('\\').eq_ignore_ascii_case(ours)
}

/// The task as another task that starts a relay, when its action is `op-reason.exe --ssh-agent` (not `--status`).
fn other(task: &IRegisteredTask, path: &str) -> Option<OtherTask> {
    // SAFETY: COM calls on a live task.
    unsafe {
        let def = task.Definition().ok()?;
        let (exe, args) = exec(&def).ok()?;
        let file = Path::new(exe.trim_matches('"'))
            .file_name()?
            .to_string_lossy()
            .to_ascii_lowercase();
        let relay = file == "op-reason.exe"
            && args.split_whitespace().next() == Some("--ssh-agent")
            && !args.split_whitespace().any(|a| a == "--status");
        relay.then(|| OtherTask {
            path: path.to_owned(),
            running: task.State().is_ok_and(|s| s == TASK_STATE_RUNNING),
            enabled: task.Enabled().is_ok_and(|e| e.as_bool()),
        })
    }
}

/// The first action's program and arguments (empty when it is not a program).
///
/// # Safety
/// `def` is a live definition.
unsafe fn exec(def: &ITaskDefinition) -> windows::core::Result<(String, String)> {
    // SAFETY: COM calls on live objects (the caller's promise).
    unsafe {
        let actions = def.Actions()?;
        let mut n = 0;
        actions.Count(&mut n)?;
        for i in 1..=n {
            let action = actions.get_Item(i)?;
            let mut kind = Default::default();
            action.Type(&mut kind)?;
            if kind == TASK_ACTION_EXEC {
                let exec: IExecAction = action.cast()?;
                let mut path = BSTR::new();
                exec.Path(&mut path)?;
                let mut args = BSTR::new();
                exec.Arguments(&mut args)?;
                return Ok((path.to_string(), args.to_string()));
            }
        }
        Ok((String::new(), String::new()))
    }
}

/// A definition read into the core's shape.
///
/// # Safety
/// `def` is a live definition.
unsafe fn shape(def: &ITaskDefinition) -> windows::core::Result<TaskShape> {
    // SAFETY: COM calls on live objects (the caller's promise).
    unsafe {
        let (exe, args) = exec(def)?;
        let mut triggers = Vec::new();
        let list = def.Triggers()?;
        let mut n = 0;
        list.Count(&mut n)?;
        for i in 1..=n {
            let t = list.get_Item(i)?;
            let mut kind = Default::default();
            t.Type(&mut kind)?;
            triggers.push(if kind == TASK_TRIGGER_LOGON {
                let logon: ILogonTrigger = t.cast()?;
                let mut user = BSTR::new();
                logon.UserId(&mut user)?;
                Trigger::Logon {
                    user: user.to_string(),
                }
            } else if kind == TASK_TRIGGER_TIME {
                let rep = t.Repetition()?;
                let mut interval = BSTR::new();
                rep.Interval(&mut interval)?;
                let mut duration = BSTR::new();
                rep.Duration(&mut duration)?;
                Trigger::Repeat {
                    interval: interval.to_string(),
                    duration: duration.to_string(),
                }
            } else {
                Trigger::Other(format!("type {}", kind.0))
            });
        }
        let settings = def.Settings()?;
        let mut policy = Default::default();
        settings.MultipleInstances(&mut policy)?;
        let mut disallow = VARIANT_BOOL::default();
        settings.DisallowStartIfOnBatteries(&mut disallow)?;
        let mut stop = VARIANT_BOOL::default();
        settings.StopIfGoingOnBatteries(&mut stop)?;
        let mut limit = BSTR::new();
        settings.ExecutionTimeLimit(&mut limit)?;
        let mut hidden = VARIANT_BOOL::default();
        settings.Hidden(&mut hidden)?;
        let mut priority = 0;
        settings.Priority(&mut priority)?;
        let mut restart = 0;
        settings.RestartCount(&mut restart)?;
        let principal = def.Principal()?;
        let mut user = BSTR::new();
        principal.UserId(&mut user)?;
        let mut logon = Default::default();
        principal.LogonType(&mut logon)?;
        let mut level = Default::default();
        principal.RunLevel(&mut level)?;
        Ok(TaskShape {
            exe,
            args,
            triggers,
            user: user.to_string(),
            logon_type: match logon {
                TASK_LOGON_INTERACTIVE_TOKEN => "InteractiveToken",
                TASK_LOGON_PASSWORD => "Password",
                TASK_LOGON_S4U => "S4U",
                TASK_LOGON_SERVICE_ACCOUNT => "ServiceAccount",
                TASK_LOGON_INTERACTIVE_TOKEN_OR_PASSWORD => "InteractiveTokenOrPassword",
                _ => "other",
            }
            .to_owned(),
            run_level: if level == TASK_RUNLEVEL_HIGHEST {
                "HighestAvailable"
            } else {
                "LeastPrivilege"
            }
            .to_owned(),
            time_limit: limit.to_string(),
            instances: match policy {
                TASK_INSTANCES_IGNORE_NEW => "IgnoreNew",
                TASK_INSTANCES_PARALLEL => "Parallel",
                TASK_INSTANCES_QUEUE => "Queue",
                TASK_INSTANCES_STOP_EXISTING => "StopExisting",
                _ => "other",
            }
            .to_owned(),
            start_on_batteries: !disallow.as_bool(),
            stop_on_batteries: stop.as_bool(),
            hidden: hidden.as_bool(),
            priority,
            restart_count: restart,
        })
    }
}

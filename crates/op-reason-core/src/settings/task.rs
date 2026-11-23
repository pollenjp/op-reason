//! op-reason's scheduled task for the SSH relay (design `2026-11-23-settings-design.md` §9.1): its name, its XML
//! definition, and the shape that a task read back from Task Scheduler must have.

/// The task's name, in Task Scheduler's root folder: per-user tasks carry the user's SID, as task names are shared by
/// the users of a machine.
pub fn task_name(sid: &str) -> String {
    format!("op-reason-ssh-relay-{sid}")
}

/// What the task is made from: the exe that runs `set relay.autostart on`, the user's SID, and the task's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskWant {
    /// `op-reason.exe`'s full path.
    pub exe: String,
    /// The user's security identifier (`S-1-5-…`).
    pub sid: String,
    /// The task's name (`task_name(sid)`, or a test's).
    pub name: String,
}

/// The relay's arguments in the task: `--ssh-agent --task <name>`, so that the relay tells in `relay.json` that
/// op-reason's task started it (design §9.5).
pub fn task_args(name: &str) -> String {
    format!("--ssh-agent --task {name}")
}

/// The task's definition for `ITaskFolder::RegisterTask`, turned on or off (`enabled`). `start_local` is the start of
/// the trigger that repeats every minute, in local time (`2026-11-23T10:00:00`): the time it was made.
pub fn xml(want: &TaskWant, start_local: &str, enabled: bool) -> String {
    let exe = escape(&want.exe);
    let sid = escape(&want.sid);
    let args = escape(&task_args(&want.name));
    let start = escape(start_local);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Author>op-reason</Author>
    <Description>Starts op-reason's SSH relay (op-reason.exe --ssh-agent) at logon, and again within a minute when it has stopped. Made by op-reason --settings: change it there (op-reason --settings set relay.autostart on|off).</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{sid}</UserId>
    </LogonTrigger>
    <TimeTrigger>
      <Repetition>
        <Interval>PT1M</Interval>
        <StopAtDurationEnd>false</StopAtDurationEnd>
      </Repetition>
      <StartBoundary>{start}</StartBoundary>
      <Enabled>true</Enabled>
    </TimeTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{sid}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>{enabled}</Enabled>
    <Hidden>true</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>{args}</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// A trigger as the shape compares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// At the logon of a user (as Task Scheduler names it: a SID or `DOMAIN\user`).
    Logon {
        /// The user.
        user: String,
    },
    /// A time trigger that repeats (its start is not compared: it is the time the task was made).
    Repeat {
        /// The repetition's interval (`PT1M`).
        interval: String,
        /// How long it repeats; empty: without end.
        duration: String,
    },
    /// Any other trigger, by its kind.
    Other(String),
}

/// A task as the settings compare it (design §9.1): the fields that make the relay run as op-reason wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskShape {
    /// The action's program.
    pub exe: String,
    /// The action's arguments.
    pub args: String,
    /// The triggers in their order.
    pub triggers: Vec<Trigger>,
    /// The principal's user.
    pub user: String,
    /// `InteractiveToken`, `Password`, `S4U`, …
    pub logon_type: String,
    /// `LeastPrivilege` or `HighestAvailable`.
    pub run_level: String,
    /// The execution time limit (`PT0S`: none).
    pub time_limit: String,
    /// `IgnoreNew`, `Parallel`, `Queue` or `StopExisting`.
    pub instances: String,
    /// Starts on batteries (not "disallow start if on batteries").
    pub start_on_batteries: bool,
    /// Stops when the machine goes on batteries.
    pub stop_on_batteries: bool,
    /// Hidden in Task Scheduler's window.
    pub hidden: bool,
    /// 0 (highest) to 10.
    pub priority: i32,
    /// How many times it is restarted on failure.
    pub restart_count: i32,
}

/// The shape of the task that [`xml`] defines.
pub fn wanted(want: &TaskWant) -> TaskShape {
    TaskShape {
        exe: want.exe.clone(),
        args: task_args(&want.name),
        triggers: vec![
            Trigger::Logon {
                user: want.sid.clone(),
            },
            Trigger::Repeat {
                interval: "PT1M".to_owned(),
                duration: String::new(),
            },
        ],
        user: want.sid.clone(),
        logon_type: "InteractiveToken".to_owned(),
        run_level: "LeastPrivilege".to_owned(),
        time_limit: "PT0S".to_owned(),
        instances: "IgnoreNew".to_owned(),
        start_on_batteries: true,
        stop_on_batteries: false,
        hidden: true,
        priority: 5,
        restart_count: 0,
    }
}

/// The fields in which `have` differs from `want`, in the order of [`TaskShape`]: `exe` (without regard to case),
/// `args`, `triggers`, `user`, `logon-type`, `run-level`, `time-limit`, `instances`, `batteries`, `hidden`,
/// `priority`, `restart`. Users are compared by `same_user` (a SID and a name of the same user are the same).
pub fn differences(
    have: &TaskShape,
    want: &TaskShape,
    same_user: &dyn Fn(&str, &str) -> bool,
) -> Vec<&'static str> {
    let same_trigger = |a: &Trigger, b: &Trigger| match (a, b) {
        (Trigger::Logon { user: x }, Trigger::Logon { user: y }) => same_user(x, y),
        _ => a == b,
    };
    let triggers = have.triggers.len() == want.triggers.len()
        && have
            .triggers
            .iter()
            .zip(&want.triggers)
            .all(|(a, b)| same_trigger(a, b));
    let checks = [
        ("exe", have.exe.eq_ignore_ascii_case(&want.exe)),
        ("args", have.args == want.args),
        ("triggers", triggers),
        ("user", same_user(&have.user, &want.user)),
        ("logon-type", have.logon_type == want.logon_type),
        ("run-level", have.run_level == want.run_level),
        ("time-limit", have.time_limit == want.time_limit),
        ("instances", have.instances == want.instances),
        (
            "batteries",
            have.start_on_batteries == want.start_on_batteries
                && have.stop_on_batteries == want.stop_on_batteries,
        ),
        ("hidden", have.hidden == want.hidden),
        ("priority", have.priority == want.priority),
        ("restart", have.restart_count == want.restart_count),
    ];
    checks
        .into_iter()
        .filter(|(_, same)| !same)
        .map(|(name, _)| name)
        .collect()
}

/// op-reason's task as Task Scheduler has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskNow {
    /// Its shape.
    pub shape: TaskShape,
    /// Turned on (a stop turns it off, design §9.3).
    pub enabled: bool,
    /// An instance runs (the relay of op-reason's task).
    pub running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "S-1-5-21-1-2-3-1001";

    fn want() -> TaskWant {
        TaskWant {
            exe: r"C:\Program Files & Co\op-reason\op-reason.exe".into(),
            sid: SID.into(),
            name: task_name(SID),
        }
    }

    fn eq(a: &str, b: &str) -> bool {
        a == b
    }

    #[test]
    fn its_name() {
        assert_eq!(task_name(SID), "op-reason-ssh-relay-S-1-5-21-1-2-3-1001");
    }

    #[test]
    fn the_xml_says_the_shape() {
        let x = xml(&want(), "2026-11-23T10:00:00", true);
        assert!(x.starts_with(r#"<?xml version="1.0" encoding="UTF-16"?>"#));
        for part in [
            r"<Command>C:\Program Files &amp; Co\op-reason\op-reason.exe</Command>",
            "<Arguments>--ssh-agent --task op-reason-ssh-relay-S-1-5-21-1-2-3-1001</Arguments>",
            "<LogonTrigger>",
            "<UserId>S-1-5-21-1-2-3-1001</UserId>",
            "<Interval>PT1M</Interval>",
            "<StopAtDurationEnd>false</StopAtDurationEnd>",
            "<StartBoundary>2026-11-23T10:00:00</StartBoundary>",
            "<LogonType>InteractiveToken</LogonType>",
            "<RunLevel>LeastPrivilege</RunLevel>",
            "<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>",
            "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>",
            "<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>",
            "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
            "<Hidden>true</Hidden>",
            "<Priority>5</Priority>",
            "<Enabled>true</Enabled>",
        ] {
            assert!(x.contains(part), "{part} in {x}");
        }
        assert_eq!(
            x.matches("<UserId>").count(),
            2,
            "the trigger's and the principal's"
        );
        assert!(!x.contains("<RestartOnFailure>"), "no restart on failure");
        assert!(!x.contains("<Duration>"), "the repetition has no end");
        let off = xml(&want(), "2026-11-23T10:00:00", false);
        assert!(
            off.contains(
                "<AllowStartOnDemand>true</AllowStartOnDemand>\n    <Enabled>false</Enabled>"
            ),
            "a task made turned off: {off}"
        );
        assert_eq!(
            off.matches("<Enabled>true</Enabled>").count(),
            2,
            "its triggers stay on"
        );
    }

    #[test]
    fn the_wanted_shape() {
        let s = wanted(&want());
        assert_eq!(
            s.args, "--ssh-agent --task op-reason-ssh-relay-S-1-5-21-1-2-3-1001",
            "the relay tells that op-reason's task started it"
        );
        assert_eq!(
            s.triggers,
            vec![
                Trigger::Logon { user: SID.into() },
                Trigger::Repeat {
                    interval: "PT1M".into(),
                    duration: String::new()
                }
            ]
        );
        assert_eq!(
            (
                s.logon_type.as_str(),
                s.run_level.as_str(),
                s.time_limit.as_str(),
                s.instances.as_str()
            ),
            ("InteractiveToken", "LeastPrivilege", "PT0S", "IgnoreNew")
        );
        assert!(s.start_on_batteries && !s.stop_on_batteries && s.hidden);
        assert_eq!((s.priority, s.restart_count), (5, 0));
    }

    #[test]
    fn differences_name_the_fields() {
        let w = wanted(&want());
        assert!(differences(&w, &w, &eq).is_empty());
        let mut h = w.clone();
        h.exe = h.exe.to_uppercase();
        assert!(
            differences(&h, &w, &eq).is_empty(),
            "the exe without regard to case"
        );
        h.exe = r"D:\old\op-reason.exe".into();
        assert_eq!(differences(&h, &w, &eq), ["exe"]);
        let mut h = w.clone();
        h.triggers.pop();
        h.priority = 7;
        assert_eq!(differences(&h, &w, &eq), ["triggers", "priority"]);
        let mut h = w.clone();
        h.triggers[0] = Trigger::Logon {
            user: r"PC\me".into(),
        };
        assert_eq!(differences(&h, &w, &eq), ["triggers"]);
        assert!(
            differences(&h, &w, &|a, b| a == b || (a == r"PC\me" && b == SID)).is_empty(),
            "users compared by the system"
        );
        let mut h = w.clone();
        h.args = "--ssh-agent".into();
        h.stop_on_batteries = true;
        h.restart_count = 3;
        assert_eq!(differences(&h, &w, &eq), ["args", "batteries", "restart"]);
    }
}

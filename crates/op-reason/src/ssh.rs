//! `op-reason-ssh` (design `2026-11-17-ssh-relay-design.md` §7): runs Windows `ssh.exe` with the arguments and the
//! standard streams unchanged, and with the facts of the call in `OPREASON_SSH_CALL` (added to `WSLENV`) for the SSH
//! relay of `op-reason.exe`, which cannot see Linux processes. It never makes an ssh call fail for want of facts: when
//! it cannot collect them, `ssh.exe` runs without them. The release build aborts on a panic, so nothing here may
//! panic.

use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use op_reason_core::args;
use op_reason_core::contract::Herdr;
use op_reason_core::log::Caller;
use op_reason_core::ssh::call::{self, Proc, WslCall};
use op_reason_core::ssh::cmdline;
use op_reason_core::ssh::roots::{self, Os, Rule};
use op_reason_core::{messages, place};

use crate::procfs::{self, Link};
use crate::{launch, wrapper};

/// How long the herdr label may take: every ssh call waits for it.
const HERDR_LIMIT: Duration = Duration::from_secs(1);

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn say(line: &str) {
    launch::print_stderr(&format!("{}{line}\n", messages::PREFIX));
}

/// Runs `ssh.exe` in place of this process (same PID, same streams). Returns only when it cannot: 127 when there is no
/// `ssh.exe`, 126 when it cannot be started.
pub fn run(args: Vec<OsString>) -> i32 {
    let wanted = env_nonempty(call::ENV_SSH_EXE).unwrap_or_else(|| "ssh.exe".to_owned());
    let Some(ssh) = launch::which(&wanted) else {
        say(&messages::no_ssh_exe(wrapper::wsl_lang(), &wanted));
        return 127;
    };
    let mut cmd = Command::new(&ssh);
    cmd.args(&args);
    if let Some(text) = collect() {
        cmd.env(call::ENV_CALL, text)
            .env("WSLENV", launch::wslenv_plus(&[call::ENV_CALL]));
    }
    let err = cmd.exec();
    say(&messages::cannot_start(
        wrapper::wsl_lang(),
        &ssh.display().to_string(),
        &err.to_string(),
    ));
    126
}

/// The facts of the call as `OPREASON_SSH_CALL`, or `None` when the chain of processes cannot be read.
fn collect() -> Option<String> {
    let proc_root = env_nonempty("OPREASON_PROC_ROOT").unwrap_or_else(|| "/proc".to_owned());
    let start = env_nonempty("OPREASON_PROC_START")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(std::process::id);
    let links = procfs::chain(Path::new(&proc_root), start);
    let nodes: Vec<_> = links.iter().map(|l| l.node.clone()).collect();
    if nodes.is_empty() {
        return None;
    }
    let found = roots::find(Os::Linux, &nodes);
    let root = links.get(found.root)?;
    let claude_pid = match found.rule {
        Rule::Claude => links.get(found.root + 1).map(|l| l.pid),
        _ => None,
    };
    let kind = match found.rule {
        Rule::Claude => "claude",
        Rule::Shell => "human",
        _ => "program",
    };
    let cwd = wrapper::current_dir_text();
    let info = place::detect(Path::new(&cwd));
    let call = WslCall {
        v: 1,
        distro: env_nonempty("WSL_DISTRO_NAME").unwrap_or_default(),
        rule: found.rule.as_str().to_owned(),
        root: proc_of(root),
        shell: found.shell.and_then(|i| links.get(i)).map(|s| Proc {
            cmd: String::new(),
            ..proc_of(s)
        }),
        caller: Caller {
            kind: kind.to_owned(),
            claude_pid,
            herdr: herdr(),
            cwd: place::tilde(&cwd, env_nonempty("HOME").as_deref()),
            repo: info.repo,
            branch: info.branch,
        },
        session: env_nonempty("CLAUDE_CODE_SESSION_ID"),
        reason: args::normalize(
            &std::env::var_os(call::ENV_REASON)
                .map(|r| r.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
    };
    Some(call::encode(&call))
}

/// A process of the chain as the call names it (its command line masked).
fn proc_of(l: &Link) -> Proc {
    Proc {
        pid: l.pid,
        start: l.start,
        name: l.node.name.clone(),
        cmd: cmdline::shown(&l.node.args, call::CMD_MAX),
    }
}

/// herdr's workspace (with its label) and pane, inside herdr.
fn herdr() -> Option<Herdr> {
    if std::env::var("HERDR_ENV").as_deref() != Ok("1") {
        return None;
    }
    let workspace = std::env::var("HERDR_WORKSPACE_ID").unwrap_or_default();
    let label = if !workspace.is_empty() && launch::which("herdr").is_some() {
        wrapper::herdr_workspace_label(&workspace, HERDR_LIMIT)
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect()
    } else {
        String::new()
    };
    Some(Herdr {
        workspace,
        label,
        pane: std::env::var("HERDR_PANE_ID").unwrap_or_default(),
    })
}

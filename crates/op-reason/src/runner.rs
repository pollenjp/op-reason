//! The runner: the Windows process that shows the window and starts `op.exe` for one call (design
//! `2026-11-15-wsl-through-exe-design.md` §4). `op-reason.exe --run` by default; `powershell.exe -File launcher.ps1`
//! when the exe cannot run.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use op_reason_core::contract;

use crate::launch;

/// Chooses the runner: `auto` (default), `exe` or `launcher`.
pub const ENV_RUNNER: &str = "OPREASON_RUNNER";
/// The WSL path of `op-reason.exe` (default: `op-reason.exe` in `PATH`).
pub const ENV_EXE: &str = "OPREASON_EXE";
/// The `powershell.exe` to start the launcher with (default: `powershell.exe` in `PATH`).
pub const ENV_POWERSHELL: &str = "OPREASON_POWERSHELL";

/// The arguments of `powershell.exe` before the path of `launcher.ps1`.
pub const POWERSHELL_ARGS: [&str; 7] = [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-OutputFormat",
    "Text",
    "-File",
];

/// `OPREASON_RUNNER`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// `op-reason.exe`, and the launcher when the exe leaves no status file.
    Auto,
    /// `op-reason.exe` only.
    Exe,
    /// The launcher only.
    Launcher,
}

impl Choice {
    /// From the variable's value: `exe` and `launcher` (any case, spaces trimmed); anything else is `auto`, like the
    /// other `OPREASON_` variables fall back to their default.
    pub fn parse(value: Option<&str>) -> Choice {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("exe") => Choice::Exe,
            Some("launcher") => Choice::Launcher,
            _ => Choice::Auto,
        }
    }
}

/// The launcher: `powershell.exe` and the WSL path of `launcher.ps1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launcher {
    pub powershell: PathBuf,
    pub script: PathBuf,
}

/// A part that is missing, and what was tried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// `op-reason.exe` (the path tried, empty when none).
    Exe(String),
    /// `launcher.ps1` (the path tried, empty when no folder was known).
    Launcher(String),
    /// `powershell.exe` (the name or path tried).
    PowerShell(String),
}

impl Missing {
    /// The `why` of the `refused` record.
    pub fn why(&self) -> &'static str {
        match self {
            Missing::Exe(_) => "no-exe",
            Missing::Launcher(_) => "no-launcher",
            Missing::PowerShell(_) => "no-powershell",
        }
    }
}

/// Whom the call starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerPlan {
    /// `op-reason.exe`; with `fallback`, the launcher of `folder` when the exe leaves no status file.
    Exe {
        exe: PathBuf,
        folder: PathBuf,
        fallback: bool,
    },
    /// The launcher. `not_found` is set when `auto` found no exe where `OPREASON_EXE` points (a fallback).
    Launcher {
        launcher: Launcher,
        not_found: Option<PathBuf>,
    },
}

/// Where the parts are looked for: the environment and `PATH`, as functions so that tests can replace them.
pub struct Lookup<'a> {
    pub env: &'a dyn Fn(&str) -> Option<String>,
    pub which: &'a dyn Fn(&str) -> Option<PathBuf>,
}

impl Lookup<'_> {
    fn var(&self, key: &str) -> Option<String> {
        (self.env)(key).filter(|v| !v.is_empty())
    }

    /// `op-reason.exe`: the path of `OPREASON_EXE` (whether a file is there or not), else the one in `PATH`.
    pub fn exe(&self) -> Option<PathBuf> {
        match self.var(ENV_EXE) {
            Some(p) => Some(PathBuf::from(p)),
            None => (self.which)("op-reason.exe"),
        }
    }

    /// The launcher in `folder` (the folder of `op-reason.exe`).
    pub fn launcher(&self, folder: Option<&Path>) -> Result<Launcher, Missing> {
        let script = folder.map(|f| f.join("launcher.ps1"));
        let script = match script {
            Some(s) if s.is_file() => s,
            other => {
                return Err(Missing::Launcher(
                    other.map(|p| p.display().to_string()).unwrap_or_default(),
                ));
            }
        };
        let name = self
            .var(ENV_POWERSHELL)
            .unwrap_or_else(|| "powershell.exe".to_string());
        match (self.which)(&name) {
            Some(powershell) => Ok(Launcher { powershell, script }),
            None => Err(Missing::PowerShell(name)),
        }
    }

    /// The plan for `choice` (design §4.2).
    pub fn plan(&self, choice: Choice) -> Result<RunnerPlan, Missing> {
        let exe = self.exe();
        let folder = exe.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        let exe_file = exe.clone().filter(|p| p.is_file());
        match (choice, exe_file) {
            (Choice::Launcher, _) => {
                self.launcher(folder.as_deref())
                    .map(|launcher| RunnerPlan::Launcher {
                        launcher,
                        not_found: None,
                    })
            }
            (Choice::Exe | Choice::Auto, Some(exe)) => Ok(RunnerPlan::Exe {
                folder: folder.unwrap_or_default(),
                exe,
                fallback: choice == Choice::Auto,
            }),
            (Choice::Exe, None) => Err(Missing::Exe(shown(exe.as_deref()))),
            (Choice::Auto, None) => match self.launcher(folder.as_deref()) {
                Ok(launcher) => Ok(RunnerPlan::Launcher {
                    launcher,
                    not_found: exe,
                }),
                Err(_) => Err(Missing::Exe(shown(exe.as_deref()))),
            },
        }
    }
}

fn shown(p: Option<&Path>) -> String {
    p.map(|p| p.display().to_string()).unwrap_or_default()
}

/// One runner to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runner {
    Exe(PathBuf),
    Launcher(Launcher),
}

impl Runner {
    /// `exe` or `launcher` (the `end` record's `runner`).
    pub fn name(&self) -> &'static str {
        match self {
            Runner::Exe(_) => "exe",
            Runner::Launcher(_) => "launcher",
        }
    }

    /// The program, for messages.
    pub fn program(&self) -> &Path {
        match self {
            Runner::Exe(p) => p,
            Runner::Launcher(l) => &l.powershell,
        }
    }

    /// The arguments: `--run` for the exe; for the launcher the `powershell.exe` options and the Windows path of
    /// `launcher.ps1` (`windows_path` converts it, `wslpath -w` outside tests).
    pub fn args(&self, windows_path: &dyn Fn(&Path) -> String) -> Vec<OsString> {
        match self {
            Runner::Exe(_) => vec![OsString::from(contract::RUN_ARG)],
            Runner::Launcher(l) => POWERSHELL_ARGS
                .iter()
                .map(OsString::from)
                .chain(std::iter::once(OsString::from(windows_path(&l.script))))
                .collect(),
        }
    }

    /// The command (children start with an empty signal mask, see [`launch::command`]).
    pub fn command(&self) -> Command {
        let mut cmd = launch::command(self.program());
        cmd.args(self.args(&|p| launch::wslpath_w(p)));
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Dir {
            let d = std::env::temp_dir().join(format!(
                "op-reason-runner-{tag}-{}-{}",
                std::process::id(),
                op_reason_core::log::random_u64()
            ));
            fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
        fn file(&self, name: &str) -> PathBuf {
            let p = self.0.join(name);
            fs::write(&p, "").unwrap();
            p
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn plan_with(
        choice: Choice,
        vars: &[(&str, &str)],
        path: &[(&str, &Path)],
    ) -> Result<RunnerPlan, Missing> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let path: HashMap<String, PathBuf> = path
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_path_buf()))
            .collect();
        let env = move |k: &str| vars.get(k).cloned();
        let which = move |k: &str| path.get(k).cloned();
        Lookup {
            env: &env,
            which: &which,
        }
        .plan(choice)
    }

    #[test]
    fn choice_defaults_to_auto() {
        assert_eq!(Choice::parse(None), Choice::Auto);
        assert_eq!(Choice::parse(Some("")), Choice::Auto);
        assert_eq!(Choice::parse(Some("exee")), Choice::Auto);
        assert_eq!(Choice::parse(Some(" Exe ")), Choice::Exe);
        assert_eq!(Choice::parse(Some("LAUNCHER")), Choice::Launcher);
    }

    #[test]
    fn auto_prefers_the_exe_and_keeps_the_fallback() {
        let d = Dir::new("auto");
        let exe = d.file("op-reason.exe");
        let ps = d.file("powershell.exe");
        let p = plan_with(
            Choice::Auto,
            &[],
            &[("op-reason.exe", &exe), ("powershell.exe", &ps)],
        );
        assert_eq!(
            p,
            Ok(RunnerPlan::Exe {
                exe: exe.clone(),
                folder: d.0.clone(),
                fallback: true
            })
        );
        // exe mode: the same exe, no fallback
        let p = plan_with(Choice::Exe, &[(ENV_EXE, exe.to_str().unwrap())], &[]);
        assert!(matches!(
            p,
            Ok(RunnerPlan::Exe {
                fallback: false,
                ..
            })
        ));
    }

    #[test]
    fn auto_without_the_exe_uses_the_launcher_next_to_where_it_should_be() {
        let d = Dir::new("noexe");
        let script = d.file("launcher.ps1");
        let ps = d.file("powershell.exe");
        let missing = d.0.join("op-reason.exe");
        let p = plan_with(
            Choice::Auto,
            &[(ENV_EXE, missing.to_str().unwrap())],
            &[("powershell.exe", &ps)],
        );
        assert_eq!(
            p,
            Ok(RunnerPlan::Launcher {
                launcher: Launcher {
                    powershell: ps.clone(),
                    script: script.clone()
                },
                not_found: Some(missing.clone())
            })
        );
        // nothing at all: no-exe
        let p = plan_with(Choice::Auto, &[], &[("powershell.exe", &ps)]);
        assert_eq!(p, Err(Missing::Exe(String::new())));
        // exe mode never uses the launcher
        let p = plan_with(
            Choice::Exe,
            &[(ENV_EXE, missing.to_str().unwrap())],
            &[("powershell.exe", &ps)],
        );
        assert_eq!(p, Err(Missing::Exe(missing.display().to_string())));
    }

    #[test]
    fn launcher_mode_needs_the_script_and_powershell() {
        let d = Dir::new("launcher");
        let exe = d.file("op-reason.exe");
        let p = plan_with(Choice::Launcher, &[(ENV_EXE, exe.to_str().unwrap())], &[]);
        assert_eq!(
            p,
            Err(Missing::Launcher(
                d.0.join("launcher.ps1").display().to_string()
            ))
        );
        let script = d.file("launcher.ps1");
        let p = plan_with(
            Choice::Launcher,
            &[
                (ENV_EXE, exe.to_str().unwrap()),
                (ENV_POWERSHELL, "/no/ps.exe"),
            ],
            &[],
        );
        assert_eq!(p, Err(Missing::PowerShell("/no/ps.exe".into())));
        let ps = d.file("ps.exe");
        let p = plan_with(
            Choice::Launcher,
            &[(ENV_POWERSHELL, "my-ps.exe")],
            &[("op-reason.exe", &exe), ("my-ps.exe", &ps)],
        );
        assert_eq!(
            p,
            Ok(RunnerPlan::Launcher {
                launcher: Launcher {
                    powershell: ps,
                    script
                },
                not_found: None
            })
        );
    }

    #[test]
    fn command_lines() {
        let exe = Runner::Exe(PathBuf::from("/w/op-reason.exe"));
        assert_eq!(exe.args(&|_| unreachable!()), vec![OsString::from("--run")]);
        let l = Runner::Launcher(Launcher {
            powershell: PathBuf::from("/w/powershell.exe"),
            script: PathBuf::from("/w/launcher.ps1"),
        });
        let args = l.args(&|p| format!("C:{}", p.display()));
        assert_eq!(
            args,
            [
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-OutputFormat",
                "Text",
                "-File",
                "C:/w/launcher.ps1"
            ]
            .map(OsString::from)
        );
        assert_eq!((exe.name(), l.name()), ("exe", "launcher"));
    }
}

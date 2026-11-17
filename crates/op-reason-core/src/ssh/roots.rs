//! The root of a call (design §5.2): a request belongs to the call of its root process, found up the chain of the
//! process that asks. `tests/cases/roots.json` holds Windows chains and `/proc` chains with their roots.
//!
//! 1. A Claude Code ancestor (`claude.exe`, `claude`): the root is its child on the chain (one tool call).
//! 2. Else an interactive shell ancestor: the root is the shell's child (the command a person typed).
//! 3. Else the client's parent (a program that starts ssh itself).
//!
//! On Windows a client whose parent is a WSL interop host is a WSL call: its root is on Linux.

/// Where the chain was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux,
}

/// One process of a chain: its name (the Windows image name, or the Linux `comm`) and its arguments (`args[0]` is the
/// program as started; empty when they cannot be read).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub args: Vec<String>,
}

/// Which rule found the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// A Claude Code ancestor.
    Claude,
    /// An interactive shell ancestor.
    Shell,
    /// The client's parent.
    Parent,
    /// Windows: the client's parent is a WSL interop host.
    Wsl,
}

impl Rule {
    /// The word of the records and of `OPREASON_SSH_CALL`.
    pub fn as_str(self) -> &'static str {
        match self {
            Rule::Claude => "claude",
            Rule::Shell => "shell",
            Rule::Parent => "parent",
            Rule::Wsl => "wsl",
        }
    }
}

/// The root found: the rule, the root's index in the chain and, for rule 2, the shell's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    pub rule: Rule,
    pub root: usize,
    pub shell: Option<usize>,
}

/// The Windows shells: `cmd.exe`, PowerShell, and POSIX shells built for Windows.
pub const WINDOWS_SHELLS: &[&str] = &[
    "cmd.exe",
    "pwsh.exe",
    "powershell.exe",
    "bash.exe",
    "sh.exe",
    "zsh.exe",
    "fish.exe",
    "nu.exe",
];
/// The Linux shells (by `comm`).
pub const LINUX_SHELLS: &[&str] = &[
    "bash", "sh", "dash", "zsh", "fish", "ksh", "mksh", "tcsh", "csh", "nu", "elvish", "xonsh",
];
/// The processes that start Windows programs for WSL.
pub const WSL_HOSTS: &[&str] = &["wsl.exe", "wslhost.exe"];

/// The root of the call of `chain[0]` (the client), by the rules above.
pub fn find(os: Os, chain: &[Node]) -> Found {
    if chain.len() <= 1 {
        return Found {
            rule: Rule::Parent,
            root: 0,
            shell: None,
        };
    }
    if os == Os::Windows && named(os, &chain[1].name, WSL_HOSTS) {
        return Found {
            rule: Rule::Wsl,
            root: 1,
            shell: None,
        };
    }
    let claude: &[&str] = match os {
        Os::Windows => &["claude.exe"],
        Os::Linux => &["claude"],
    };
    if let Some(k) = (1..chain.len()).find(|&k| named(os, &chain[k].name, claude)) {
        return Found {
            rule: Rule::Claude,
            root: k - 1,
            shell: None,
        };
    }
    if let Some(k) = (1..chain.len()).find(|&k| interactive_shell(os, &chain[k])) {
        return Found {
            rule: Rule::Shell,
            root: k - 1,
            shell: Some(k),
        };
    }
    Found {
        rule: Rule::Parent,
        root: 1,
        shell: None,
    }
}

/// Whether `name` is one of `names` (Windows image names without regard to case).
fn named(os: Os, name: &str, names: &[&str]) -> bool {
    match os {
        Os::Windows => names.iter().any(|n| n.eq_ignore_ascii_case(name)),
        Os::Linux => names.contains(&name),
    }
}

/// Whether a process is a shell a person types into: a known shell whose arguments run no command and no script, or
/// keep it open after one (`-NoExit`, `/k`). A shell whose arguments cannot be read counts as interactive.
pub fn interactive_shell(os: Os, node: &Node) -> bool {
    let args = node.args.get(1..).unwrap_or_default();
    match os {
        Os::Windows if node.name.eq_ignore_ascii_case("cmd.exe") => cmd_interactive(args),
        Os::Windows
            if node.name.eq_ignore_ascii_case("pwsh.exe")
                || node.name.eq_ignore_ascii_case("powershell.exe") =>
        {
            powershell_interactive(args)
        }
        Os::Windows if named(os, &node.name, WINDOWS_SHELLS) => posix_interactive(args),
        Os::Linux if named(os, &node.name, LINUX_SHELLS) => posix_interactive(args),
        _ => false,
    }
}

/// `cmd.exe`: `/c` and `/r` run a command and exit; `/k` runs one and stays.
fn cmd_interactive(args: &[String]) -> bool {
    for a in args {
        let a = a.to_ascii_lowercase();
        if a.starts_with("/c") || a.starts_with("/r") {
            return false;
        }
        if a.starts_with("/k") {
            return true;
        }
    }
    true
}

/// Whether `word` is `full` or a prefix of it at least `min` long (PowerShell's parameters may be shortened).
fn shortened(word: &str, full: &str, min: usize) -> bool {
    word.len() >= min && full.starts_with(word)
}

/// PowerShell (`pwsh`, `powershell`): `-NoExit` keeps it open; `-Command`, `-File`, `-EncodedCommand`,
/// `-CommandWithArgs` or a script by position run something and exit (what follows them belongs to it).
fn powershell_interactive(args: &[String]) -> bool {
    const VALUED: &[(&str, usize)] = &[
        ("executionpolicy", 2),
        ("ep", 2),
        ("workingdirectory", 2),
        ("wd", 2),
        ("configurationname", 3),
        ("configurationfile", 15),
        ("inputformat", 3),
        ("if", 2),
        ("outputformat", 1),
        ("of", 2),
        ("windowstyle", 1),
        ("settingsfile", 2),
        ("custompipename", 2),
    ];
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let Some(name) = a.strip_prefix(['-', '/']) else {
            return false;
        };
        let name = name.trim_start_matches('-').to_ascii_lowercase();
        if shortened(&name, "noexit", 3) {
            return true;
        }
        if shortened(&name, "command", 1)
            || shortened(&name, "file", 1)
            || shortened(&name, "encodedcommand", 1)
            || name == "ec"
            || name == "cwa"
            || shortened(&name, "commandwithargs", 8)
        {
            return false;
        }
        i += if VALUED
            .iter()
            .any(|(full, min)| shortened(&name, full, *min))
        {
            2
        } else {
            1
        };
    }
    true
}

/// POSIX shells (and fish, nu, xonsh): `-c` (alone or in a cluster such as `-lc`) or `--command(s)` runs a command,
/// and an operand is a script; both exit. Options that take a value skip it.
fn posix_interactive(args: &[String]) -> bool {
    const VALUED_LONG: &[&str] = &[
        "rcfile",
        "init-file",
        "init-command",
        "profile",
        "config",
        "env-config",
    ];
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" || a == "-" {
            return i + 1 >= args.len();
        }
        if let Some(long) = a.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((n, _)) => (n, true),
                None => (long, false),
            };
            if name == "command" || name == "commands" {
                return false;
            }
            i += if !value && VALUED_LONG.contains(&name) {
                2
            } else {
                1
            };
            continue;
        }
        if let Some(letters) = a.strip_prefix(['-', '+']) {
            if letters.contains('c') {
                return false;
            }
            i += if matches!(letters, "o" | "O" | "C") {
                2
            } else {
                1
            };
            continue;
        }
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(name: &str, args: &[&str]) -> Node {
        Node {
            name: name.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn interactive_or_running_something() {
        let w = Os::Windows;
        assert!(interactive_shell(w, &n("cmd.exe", &["cmd.exe"])));
        assert!(interactive_shell(
            w,
            &n("cmd.exe", &["cmd", "/k", "init.bat"])
        ));
        assert!(!interactive_shell(
            w,
            &n("cmd.exe", &["cmd", "/d", "/s", "/c", "x"])
        ));
        assert!(!interactive_shell(w, &n("CMD.EXE", &["cmd", "/C", "x"])));
        assert!(!interactive_shell(w, &n("cmd.exe", &["cmd", "/R", "x"])));
        assert!(interactive_shell(w, &n("pwsh.exe", &["pwsh", "-NoLogo"])));
        assert!(interactive_shell(
            w,
            &n("pwsh.exe", &["pwsh", "-noe", "-c", "x"])
        ));
        assert!(!interactive_shell(
            w,
            &n("pwsh.exe", &["pwsh", "-c", "x", "-NoExit"])
        ));
        assert!(!interactive_shell(
            w,
            &n("pwsh.exe", &["pwsh", "-f", "a.ps1"])
        ));
        assert!(!interactive_shell(
            w,
            &n("pwsh.exe", &["pwsh", "-ec", "ZQBjAGgAbwA="])
        ));
        assert!(!interactive_shell(
            w,
            &n("pwsh.exe", &["pwsh", "--command", "x"])
        ));
        assert!(interactive_shell(
            w,
            &n("powershell.exe", &["powershell", "-ep", "Bypass"])
        ));
        assert!(
            interactive_shell(w, &n("pwsh.exe", &[])),
            "unreadable arguments"
        );
        assert!(
            !interactive_shell(w, &n("git.exe", &["git"])),
            "not a shell"
        );
        let l = Os::Linux;
        assert!(interactive_shell(l, &n("zsh", &["zsh", "-l"])));
        assert!(interactive_shell(l, &n("bash", &["bash", "--login"])));
        assert!(interactive_shell(l, &n("bash", &["bash", "--"])));
        assert!(!interactive_shell(l, &n("bash", &["bash", "--", "x.sh"])));
        assert!(!interactive_shell(l, &n("sh", &["sh", "-lc", "x"])));
        assert!(!interactive_shell(l, &n("nu", &["nu", "--commands", "x"])));
        assert!(!interactive_shell(l, &n("fish", &["fish", "--command=x"])));
        assert!(
            !interactive_shell(l, &n("fish", &["fish", "--interactive", "-c", "x"])),
            "-c runs a command and exits, -i or not"
        );
        assert!(
            !interactive_shell(l, &n("bash.exe", &["bash"])),
            "Windows names on Linux"
        );
        assert!(interactive_shell(w, &n("bash.exe", &["bash", "-i"])));
    }
}

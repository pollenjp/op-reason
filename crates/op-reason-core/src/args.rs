//! Planning one invocation from its arguments: the wrapper's own options, the reason, the mode, and how the
//! arguments are shown (masked) in the log and the window.
//!
//! The PowerShell entry implements the same rules; `tests/cases/calls.json` keeps the two in step.

use std::ffi::{OsStr, OsString};
use std::path::Path;

/// How the executable was invoked (the file name of `argv[0]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    /// `op-reason-log`: the log viewer.
    LogViewer,
    /// `op-reason-ssh`: runs Windows `ssh.exe` with the facts of the call (WSL; design `2026-11-17-ssh-relay-design.md`
    /// §7).
    Ssh,
    /// The wrapper, called by its own name or another one.
    Wrapper(Name),
}

/// The name the wrapper was called by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Name {
    /// `op-reason`: `--help` and `--version` are the wrapper's own.
    OpReason,
    /// Anything else (`op`): transparent, `--help` and `--version` go to `op`.
    Other,
}

/// Classifies `argv[0]`.
pub fn invocation(argv0: &OsStr) -> Invocation {
    let file = Path::new(argv0)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = file.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    match stem {
        "op-reason-log" => Invocation::LogViewer,
        "op-reason-ssh" => Invocation::Ssh,
        "op-reason" => Invocation::Wrapper(Name::OpReason),
        _ => Invocation::Wrapper(Name::Other),
    }
}

/// The arguments after the reason was taken out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    /// Whether `--reason` was given (it may be empty).
    pub have_reason: bool,
    /// The normalized reason (empty when not given).
    pub reason: String,
    /// Everything else, for `op`, unchanged.
    pub op_args: Vec<OsString>,
}

impl Parsed {
    /// The arguments for `op` as text (lossy for bytes that are not UTF-8).
    pub fn op_args_text(&self) -> Vec<String> {
        self.op_args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }
}

/// Takes the first `--reason X` or `--reason=X` before `--` out of the arguments.
pub fn parse(args: &[OsString]) -> Parsed {
    let mut parsed = Parsed::default();
    let mut seen_dashdash = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if !seen_dashdash && !parsed.have_reason {
            if arg == "--reason" {
                parsed.have_reason = true;
                parsed.reason = args
                    .get(i + 1)
                    .map(|r| r.to_string_lossy().into_owned())
                    .unwrap_or_default();
                i += 2;
                continue;
            }
            let text = arg.to_string_lossy();
            if let Some(rest) = text.strip_prefix("--reason=") {
                parsed.have_reason = true;
                parsed.reason = rest.to_string();
                i += 1;
                continue;
            }
        }
        if arg == "--" {
            seen_dashdash = true;
        }
        parsed.op_args.push(arg.clone());
        i += 1;
    }
    if parsed.have_reason {
        parsed.reason = normalize(&parsed.reason);
    }
    parsed
}

/// Turns control characters (newline and tab too) into spaces, folds runs of spaces into one and trims spaces at both
/// ends.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = true;
    for c in text.chars() {
        let c = if c.is_ascii_control() { ' ' } else { c };
        if c == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Cuts text longer than `n` characters and appends `ellipsis`.
pub fn clip(text: &str, n: usize, ellipsis: &str) -> String {
    match text.char_indices().nth(n) {
        Some((at, _)) => format!("{}{ellipsis}", &text[..at]),
        None => text.to_string(),
    }
}

/// Whether a call without a reason is a help call that needs no approval: the first argument is `--version`, `-v`,
/// `--help`, `-h` or `help`, or `--help`/`-h` appears before `--`.
pub fn is_help(op_args: &[String]) -> bool {
    let Some(first) = op_args.first() else {
        return false;
    };
    if matches!(
        first.as_str(),
        "--version" | "-v" | "--help" | "-h" | "help"
    ) {
        return true;
    }
    for a in op_args {
        match a.as_str() {
            "--" => return false,
            "--help" | "-h" => return true,
            _ => {}
        }
    }
    false
}

/// The arguments as the log, the window and the toast show them: values of `--session` and `--secret-key`, and of
/// `name=value` arguments, become `***`; `op://` references stay; after `--` only the command name and value-less flags
/// stay.
pub fn mask(op_args: &[String]) -> Vec<String> {
    let mut masked = Vec::with_capacity(op_args.len());
    let mut next_secret = false;
    let mut after = false;
    let mut first_after = true;
    for a in op_args {
        if after {
            if first_after || is_bare_flag(a) {
                masked.push(a.clone());
            } else {
                masked.push("***".to_string());
            }
            first_after = false;
            continue;
        }
        if next_secret {
            masked.push("***".to_string());
            next_secret = false;
            continue;
        }
        if a == "--" {
            masked.push(a.clone());
            after = true;
        } else if a == "--session" || a == "--secret-key" {
            masked.push(a.clone());
            next_secret = true;
        } else if a.starts_with("--session=") {
            masked.push("--session=***".to_string());
        } else if a.starts_with("--secret-key=") {
            masked.push("--secret-key=***".to_string());
        } else if a.starts_with('-') || a.starts_with("op://") {
            masked.push(a.clone());
        } else if let Some((name, _)) = a.split_once('=') {
            masked.push(format!("{name}=***"));
        } else {
            masked.push(a.clone());
        }
    }
    masked
}

/// `^-[A-Za-z0-9-]+$`
fn is_bare_flag(a: &str) -> bool {
    a.len() > 1
        && a.starts_with('-')
        && a[1..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The command as shown: `op` and the masked arguments; arguments made only of `[A-Za-z0-9_./:=@%+,*?-]` as they are,
/// others in double quotes with `\` and `"` escaped. Control characters (newline, tab, escape, …) become spaces, so
/// that an argument cannot move the cursor or rewrite a terminal, the window or the toast.
pub fn display(masked: &[String]) -> String {
    let mut out = String::from("op");
    for a in masked {
        out.push(' ');
        out.push_str(&word(a));
    }
    out
}

/// One argument as [`display`] shows it: as it is when it is made only of `[A-Za-z0-9_./:=@%+,*?-]`, otherwise in
/// double quotes with `\` and `"` escaped; control characters become spaces.
pub fn word(arg: &str) -> String {
    let a: String = arg
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if !a.is_empty()
        && a.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'_' | b'.'
                        | b'/'
                        | b':'
                        | b'='
                        | b'@'
                        | b'%'
                        | b'+'
                        | b','
                        | b'*'
                        | b'?'
                        | b'-'
                )
        })
    {
        a
    } else {
        format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// How a call runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The desktop app approves the call: window, toast, sign-out at the end.
    Desktop,
    /// `--version`, `--help` and similar: no reason, no window.
    Help,
    /// A service account or a Connect token: the desktop app is not involved.
    Service,
}

impl Mode {
    /// The word used in the log and the payload.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Desktop => "desktop",
            Mode::Help => "help",
            Mode::Service => "service",
        }
    }
}

/// The finer kind of a call, as the log records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Desktop app.
    Desktop,
    /// Help call.
    Help,
    /// `OP_SERVICE_ACCOUNT_TOKEN`.
    ServiceAccount,
    /// `OP_CONNECT_HOST` and `OP_CONNECT_TOKEN`.
    Connect,
}

impl Kind {
    /// The word used in the log.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Desktop => "desktop",
            Kind::Help => "help",
            Kind::ServiceAccount => "service-account",
            Kind::Connect => "connect",
        }
    }
}

/// Why a call is refused at the argument stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No `--reason`.
    NoReason,
    /// `--reason` that is empty after normalization.
    EmptyReason,
}

impl Refusal {
    /// The `why` of the `refused` record.
    pub fn why(self) -> &'static str {
        "no-reason"
    }
}

/// What to do with one invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Print the wrapper's usage on stdout and exit 0.
    Usage,
    /// Print `op-reason <version>` and exit 0.
    Version,
    /// Print the skill file and exit 0.
    Skill,
    /// Print the completion of `shell` for `command`.
    Completion {
        /// The shell (only `fish`).
        shell: String,
        /// The command name the completion is for.
        command: String,
    },
    /// `--completion` with a shell other than fish (the arguments given): message on stderr, exit 2, nothing logged.
    UsageError(String),
    /// Run the log viewer with these arguments.
    Log(Vec<OsString>),
    /// Refuse the call: log it, print a message, exit 2.
    Refuse(Refusal),
    /// Run `op`.
    Run {
        /// How.
        mode: Mode,
        /// The finer kind.
        kind: Kind,
    },
}

/// The exit code of a refusal at the argument stage.
pub const EXIT_NO_REASON: i32 = 2;

/// Plans one invocation. `env` reads an environment variable (empty values count as unset).
pub fn plan(name: Name, args: &[OsString], env: &dyn Fn(&str) -> Option<String>) -> (Parsed, Plan) {
    let set = |key: &str| env(key).is_some_and(|v| !v.is_empty());
    if let Some(first) = args.first() {
        let rest = &args[1..];
        if first == "--skill" && rest.is_empty() {
            return (Parsed::default(), Plan::Skill);
        }
        if first == "--completion" {
            let shell = rest
                .first()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let plan = if shell == "fish" && rest.len() <= 2 {
                let command = rest
                    .get(1)
                    .map(|c| c.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "op".to_string());
                Plan::Completion { shell, command }
            } else {
                Plan::UsageError(
                    rest.iter()
                        .map(|s| s.to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            };
            return (Parsed::default(), plan);
        }
        if first == "--log" {
            return (Parsed::default(), Plan::Log(rest.to_vec()));
        }
        if name == Name::OpReason && rest.is_empty() {
            if first == "--help" || first == "-h" {
                return (Parsed::default(), Plan::Usage);
            }
            if first == "--version" || first == "-V" {
                return (Parsed::default(), Plan::Version);
            }
        }
    }
    let parsed = parse(args);
    if parsed.op_args.is_empty() {
        return (parsed, Plan::Usage);
    }
    let plan = if parsed.have_reason {
        if parsed.reason.is_empty() {
            Plan::Refuse(Refusal::EmptyReason)
        } else if set("OP_SERVICE_ACCOUNT_TOKEN") {
            Plan::Run {
                mode: Mode::Service,
                kind: Kind::ServiceAccount,
            }
        } else if set("OP_CONNECT_HOST") && set("OP_CONNECT_TOKEN") {
            Plan::Run {
                mode: Mode::Service,
                kind: Kind::Connect,
            }
        } else {
            Plan::Run {
                mode: Mode::Desktop,
                kind: Kind::Desktop,
            }
        }
    } else if is_help(&parsed.op_args_text()) {
        Plan::Run {
            mode: Mode::Help,
            kind: Kind::Help,
        }
    } else {
        Plan::Refuse(Refusal::NoReason)
    };
    (parsed, plan)
}

/// The length of the arguments as they travel in the payload (base64 of each), and whether it is over the limit.
pub fn args_b64_len(op_args: &[OsString]) -> usize {
    op_args
        .iter()
        .map(|a| crate::base64::encoded_len(os_bytes(a).len()))
        .sum()
}

/// Arguments over this many base64 characters are refused (`too-long`).
pub const ARGS_B64_MAX: usize = 28_000;

/// The raw bytes of an argument (on Unix the bytes as given; elsewhere UTF-8, lossy).
pub fn os_bytes(arg: &OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        arg.as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        arg.to_string_lossy().into_owned().into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn clips_by_characters() {
        assert_eq!(clip("あ😀い", 2, "…"), "あ😀…");
        assert_eq!(clip("あ😀い", 3, "…"), "あ😀い");
        assert_eq!(clip("", 0, "…"), "");
        assert_eq!(clip("abc", 0, "..."), "...");
    }

    #[test]
    fn normalizes() {
        assert_eq!(
            normalize("  直す\tため\nに\u{1}読む  "),
            "直す ため に 読む"
        );
        assert_eq!(normalize(" \t "), "");
        assert_eq!(normalize("a\u{7f}b"), "a b");
    }

    #[test]
    fn reason_after_dashdash_belongs_to_op() {
        let p = parse(&os(&["a", "--", "b", "--reason", "X"]));
        assert!(!p.have_reason);
        assert_eq!(p.op_args, os(&["a", "--", "b", "--reason", "X"]));
    }

    #[test]
    fn only_the_first_reason_is_taken() {
        let p = parse(&os(&["--reason", "A", "x", "--reason", "B"]));
        assert_eq!(p.reason, "A");
        assert_eq!(p.op_args, os(&["x", "--reason", "B"]));
    }

    #[cfg(unix)]
    #[test]
    fn keeps_non_utf8_bytes_for_op() {
        use std::os::unix::ffi::OsStringExt;
        let raw = OsString::from_vec(vec![b'a', 0xff, b'b']);
        let p = parse(&[OsString::from("--reason=x"), raw.clone()]);
        assert_eq!(p.op_args, vec![raw.clone()]);
        assert_eq!(os_bytes(&raw), vec![b'a', 0xff, b'b']);
        assert_eq!(p.op_args_text(), vec!["a\u{fffd}b".to_string()]);
    }

    #[test]
    fn too_long_counts_base64() {
        assert_eq!(args_b64_len(&os(&["abc", "a"])), 4 + 4);
    }

    #[test]
    fn invocation_names() {
        assert_eq!(
            invocation(OsStr::new("/usr/bin/op-reason-log")),
            Invocation::LogViewer
        );
        assert_eq!(
            invocation(OsStr::new("op-reason")),
            Invocation::Wrapper(Name::OpReason)
        );
        assert_eq!(
            invocation(OsStr::new("C:\\x\\op-reason.EXE")),
            Invocation::Wrapper(if cfg!(windows) {
                Name::OpReason
            } else {
                Name::Other
            })
        );
        assert_eq!(
            invocation(OsStr::new("op")),
            Invocation::Wrapper(Name::Other)
        );
        assert_eq!(
            invocation(OsStr::new("/home/me/.nix-profile/bin/op-reason-ssh")),
            Invocation::Ssh
        );
    }
}

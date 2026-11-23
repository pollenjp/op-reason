//! The options of the SSH relay, `op-reason.exe --ssh-agent [options]` (design `2026-11-17-ssh-relay-design.md` §8).
//! A mode is changed by starting the relay again with another option; the window has no switch for it.

use op_reason_core::lang::Lang;
use op_reason_core::settings::values;

/// The first argument that chooses the relay's face.
pub const RELAY_ARG: &str = "--ssh-agent";
/// The relay's own pipe.
pub const DEFAULT_PIPE: &str = values::RELAY_PIPE;
/// The 1Password agent's pipe (the one Windows OpenSSH uses by default).
pub const DEFAULT_UPSTREAM: &str = values::RELAY_UPSTREAM;
/// How long an answer covers its call: 60 minutes.
pub const DEFAULT_LIMIT: u64 = values::DEFAULT_TIME_LIMIT;
/// How long a window waits for an answer: 60 seconds.
pub const DEFAULT_ANSWER_WAIT: u64 = values::DEFAULT_ANSWER_WAIT;

/// `per-call` (the relay decides) or `show-only` (1Password decides): the core's, which settings hold too.
pub use values::RelayMode as Mode;

/// What the relay was started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opts {
    pub mode: Mode,
    /// The relay's pipe (`\\.\pipe\…`).
    pub pipe: String,
    /// The 1Password agent's pipe.
    pub upstream: String,
    /// How long an answer covers its call, in seconds.
    pub limit_secs: u64,
    /// How long a window waits for an answer, in seconds.
    pub answer_secs: u64,
    /// `--status`: say whether a relay answers on the pipe, and exit.
    pub status: bool,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts {
            mode: Mode::PerCall,
            pipe: DEFAULT_PIPE.to_owned(),
            upstream: DEFAULT_UPSTREAM.to_owned(),
            limit_secs: DEFAULT_LIMIT,
            answer_secs: DEFAULT_ANSWER_WAIT,
            status: false,
        }
    }
}

/// A mistake in the options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptError {
    /// An argument the relay does not know.
    Unknown(String),
    /// An option without its value.
    Missing(String),
    /// A mode other than `per-call` and `show-only`.
    BadMode(String),
    /// A duration that is not `<n>`, `<n>s`, `<n>m` (and `<n>h` for the time limit), or that is zero.
    BadDuration(String, String),
}

impl OptError {
    /// One line for stderr.
    pub fn message(&self, lang: Lang) -> String {
        match (self, lang) {
            (OptError::Unknown(a), Lang::En) => format!("--ssh-agent does not know {a}"),
            (OptError::Unknown(a), Lang::Ja) => format!("--ssh-agent は {a} を知りません"),
            (OptError::Missing(o), Lang::En) => format!("{o} needs a value"),
            (OptError::Missing(o), Lang::Ja) => format!("{o} には値が要ります"),
            (OptError::BadMode(m), Lang::En) => {
                format!("--mode is per-call or show-only, not {m}")
            }
            (OptError::BadMode(m), Lang::Ja) => {
                format!("--mode は per-call か show-only です ({m} ではなく)")
            }
            (OptError::BadDuration(o, v), Lang::En) => {
                format!("{o} takes a number above 0 with s, m (or h for --time-limit): not {v:?}")
            }
            (OptError::BadDuration(o, v), Lang::Ja) => format!(
                "{o} には 0 より大きい数を s・m (--time-limit は h も) を付けて書きます ({v:?} ではなく)"
            ),
        }
    }
}

/// The options after `--ssh-agent`: `--mode per-call|show-only`, `--pipe <name>`, `--upstream <name>`, `--time-limit
/// <n>[s|m|h]` (a bare number is minutes), `--answer-wait <n>[s|m]` (a bare number is seconds), `--status`; each value
/// also as `--option=value`.
pub fn parse(args: &[String]) -> Result<Opts, OptError> {
    let mut o = Opts::default();
    let mut rest = args.iter();
    while let Some(a) = rest.next() {
        if a == "--status" {
            o.status = true;
            continue;
        }
        let (name, glued) = match a.split_once('=') {
            Some((n, v)) => (n, Some(v.to_owned())),
            None => (a.as_str(), None),
        };
        if !matches!(
            name,
            "--mode" | "--pipe" | "--upstream" | "--time-limit" | "--answer-wait"
        ) {
            return Err(OptError::Unknown(a.clone()));
        }
        let value = match glued {
            Some(v) => v,
            None => rest
                .next()
                .cloned()
                .ok_or_else(|| OptError::Missing(name.to_owned()))?,
        };
        match name {
            "--mode" => o.mode = Mode::parse(&value).ok_or(OptError::BadMode(value))?,
            "--pipe" => o.pipe = pipe_path(&value),
            "--upstream" => o.upstream = pipe_path(&value),
            "--time-limit" => {
                o.limit_secs = values::duration(&value, 60, true)
                    .ok_or_else(|| OptError::BadDuration(name.to_owned(), value.clone()))?
            }
            _ => {
                o.answer_secs = values::duration(&value, 1, false)
                    .ok_or_else(|| OptError::BadDuration(name.to_owned(), value.clone()))?
            }
        }
    }
    Ok(o)
}

/// A pipe's full name: `\\.\pipe\` in front of a bare name.
pub fn pipe_path(name: &str) -> String {
    if name.starts_with(r"\\") || name.starts_with("//") {
        name.to_owned()
    } else {
        format!(r"\\.\pipe\{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Opts, OptError> {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn the_defaults() {
        assert_eq!(p(&[]).unwrap(), Opts::default());
        let o = Opts::default();
        assert_eq!(o.mode, Mode::PerCall);
        assert_eq!(o.pipe, r"\\.\pipe\op-reason-ssh-agent");
        assert_eq!(o.upstream, r"\\.\pipe\openssh-ssh-agent");
        assert_eq!((o.limit_secs, o.answer_secs, o.status), (3600, 60, false));
        assert_eq!(Mode::ShowOnly.as_str(), "show-only");
    }

    #[test]
    fn every_option_in_both_spellings() {
        let o = p(&[
            "--mode",
            "show-only",
            "--pipe",
            "mine",
            r"--upstream=\\.\pipe\up",
            "--time-limit",
            "90s",
            "--answer-wait=2m",
            "--status",
        ])
        .unwrap();
        assert_eq!(
            o,
            Opts {
                mode: Mode::ShowOnly,
                pipe: r"\\.\pipe\mine".into(),
                upstream: r"\\.\pipe\up".into(),
                limit_secs: 90,
                answer_secs: 120,
                status: true,
            }
        );
        assert_eq!(p(&["--mode=per-call"]).unwrap().mode, Mode::PerCall);
    }

    #[test]
    fn bare_numbers_and_units() {
        assert_eq!(
            p(&["--time-limit", "30"]).unwrap().limit_secs,
            1800,
            "minutes"
        );
        assert_eq!(p(&["--time-limit", "2h"]).unwrap().limit_secs, 7200);
        assert_eq!(
            p(&["--answer-wait", "5"]).unwrap().answer_secs,
            5,
            "seconds"
        );
        assert_eq!(p(&["--answer-wait", "1m"]).unwrap().answer_secs, 60);
    }

    #[test]
    fn mistakes_are_named() {
        assert_eq!(p(&["--nope"]), Err(OptError::Unknown("--nope".into())));
        assert_eq!(
            p(&["show-only"]),
            Err(OptError::Unknown("show-only".into()))
        );
        assert_eq!(p(&["--mode"]), Err(OptError::Missing("--mode".into())));
        assert_eq!(
            p(&["--mode", "later"]),
            Err(OptError::BadMode("later".into()))
        );
        for bad in ["0", "5x", "", "-1", "1h"] {
            assert!(
                matches!(p(&["--answer-wait", bad]), Err(OptError::BadDuration(..))),
                "--answer-wait {bad:?}"
            );
        }
        assert!(matches!(
            p(&["--time-limit", "0m"]),
            Err(OptError::BadDuration(..))
        ));
        let e = OptError::BadMode("later".into());
        assert!(e.message(Lang::En).contains("later") && e.message(Lang::En).contains("per-call"));
        assert!(e.message(Lang::Ja).contains("later"));
        assert!(
            OptError::Unknown("--x".into())
                .message(Lang::Ja)
                .contains("--x")
        );
        assert!(
            OptError::Missing("--pipe".into())
                .message(Lang::En)
                .contains("--pipe")
        );
        assert!(
            OptError::BadDuration("--time-limit".into(), "5x".into())
                .message(Lang::En)
                .contains("5x")
        );
    }

    #[test]
    fn pipe_names() {
        assert_eq!(pipe_path("op-reason-ssh-agent"), DEFAULT_PIPE);
        assert_eq!(pipe_path(r"\\.\pipe\x"), r"\\.\pipe\x");
        assert_eq!(pipe_path(r"\\.\PIPE\x"), r"\\.\PIPE\x");
    }
}

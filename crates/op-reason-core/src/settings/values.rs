//! The SSH relay's values that settings can hold: its mode, its time limit and how long its window waits (design
//! `2026-11-23-settings-design.md` §4), with their defaults, their words and how durations are written.

/// The relay's own pipe.
pub const RELAY_PIPE: &str = r"\\.\pipe\op-reason-ssh-agent";
/// The 1Password agent's pipe (the one Windows OpenSSH uses by default).
pub const RELAY_UPSTREAM: &str = r"\\.\pipe\openssh-ssh-agent";
/// How long an answer covers its call: 60 minutes.
pub const DEFAULT_TIME_LIMIT: u64 = 60 * 60;
/// How long a window waits for an answer: 60 seconds.
pub const DEFAULT_ANSWER_WAIT: u64 = 60;

/// `per-call` (the relay decides) or `show-only` (1Password decides).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelayMode {
    /// The relay asks once per call in its window (the default).
    #[default]
    PerCall,
    /// 1Password asks for every signature; the relay shows who and why.
    ShowOnly,
}

impl RelayMode {
    /// The word of the option, the settings, the records and the window.
    pub fn as_str(self) -> &'static str {
        match self {
            RelayMode::PerCall => "per-call",
            RelayMode::ShowOnly => "show-only",
        }
    }

    /// Exactly `per-call` or `show-only`.
    pub fn parse(word: &str) -> Option<RelayMode> {
        match word {
            "per-call" => Some(RelayMode::PerCall),
            "show-only" => Some(RelayMode::ShowOnly),
            _ => None,
        }
    }
}

/// Seconds of `<n>`, `<n>s`, `<n>m` (and `<n>h` when `hours`); a bare number counts `bare` seconds each. Zero is no
/// duration.
pub fn duration(text: &str, bare: u64, hours: bool) -> Option<u64> {
    let (digits, unit) = match text.char_indices().last() {
        Some((at, c)) if c.is_ascii_alphabetic() => (
            &text[..at],
            match c {
                's' => 1,
                'm' => 60,
                'h' if hours => 3600,
                _ => return None,
            },
        ),
        _ => (text, bare),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(unit))
        .filter(|s| *s > 0)
}

/// A time limit as `get` prints it: whole minutes as `<n>m`, else `<n>s`.
pub fn minutes_text(secs: u64) -> String {
    if secs.is_multiple_of(60) {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// An answer wait as `get` prints it: `<n>s`.
pub fn seconds_text(secs: u64) -> String {
    format!("{secs}s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        assert_eq!(RelayMode::parse("per-call"), Some(RelayMode::PerCall));
        assert_eq!(RelayMode::parse("show-only"), Some(RelayMode::ShowOnly));
        assert_eq!(RelayMode::parse("x"), None);
        assert_eq!(RelayMode::parse("Per-Call"), None);
        assert_eq!(RelayMode::ShowOnly.as_str(), "show-only");
        assert_eq!(RelayMode::default(), RelayMode::PerCall);
    }

    #[test]
    fn defaults() {
        assert_eq!(RELAY_PIPE, r"\\.\pipe\op-reason-ssh-agent");
        assert_eq!(RELAY_UPSTREAM, r"\\.\pipe\openssh-ssh-agent");
        assert_eq!((DEFAULT_TIME_LIMIT, DEFAULT_ANSWER_WAIT), (3600, 60));
    }

    #[test]
    fn durations() {
        assert_eq!(
            duration("30", 60, true),
            Some(1800),
            "a bare number counts `bare` seconds"
        );
        assert_eq!(duration("2h", 60, true), Some(7200));
        assert_eq!(duration("90s", 60, true), Some(90));
        assert_eq!(duration("5", 1, false), Some(5));
        assert_eq!(duration("1m", 1, false), Some(60));
        for bad in ["0", "0m", "5x", "", "-1", "m", " 5", "1.5m"] {
            assert_eq!(duration(bad, 60, true), None, "{bad:?}");
        }
        assert_eq!(duration("1h", 1, false), None, "hours only where allowed");
        assert_eq!(
            duration("99999999999999999999h", 60, true),
            None,
            "overflow"
        );
    }

    #[test]
    fn texts_of_durations() {
        assert_eq!(minutes_text(3600), "60m");
        assert_eq!(minutes_text(90), "90s");
        assert_eq!(minutes_text(60), "1m");
        assert_eq!(seconds_text(60), "60s");
        assert_eq!(seconds_text(5), "5s");
        for secs in [1, 59, 60, 61, 3600, 7200] {
            assert_eq!(
                duration(&minutes_text(secs), 60, true),
                Some(secs),
                "{secs}"
            );
            assert_eq!(
                duration(&seconds_text(secs), 1, false),
                Some(secs),
                "{secs}"
            );
        }
    }
}

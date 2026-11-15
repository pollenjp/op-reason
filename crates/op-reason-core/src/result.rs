//! The result of a call, from the launcher's status, `op`'s exit code and `op`'s stderr.

use crate::args::Mode;
use crate::contract::Status;

/// What the stderr lines of `op` say about the approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StderrMatch {
    /// `[ERROR] … authorization prompt dismissed`
    Denied,
    /// `[ERROR] … authoriz… timeout` / `timed out`
    Timeout,
}

/// Looks for `op`'s own `[ERROR]` lines (case-insensitive) that report a denied or timed-out approval. A denial on
/// any line wins over a timeout. A child command's "Connection timed out" does not count.
pub fn scan_stderr(stderr: &[u8]) -> Option<StderrMatch> {
    let mut timeout = false;
    for raw in stderr.split(|&b| b == b'\n') {
        let line = String::from_utf8_lossy(raw).to_ascii_lowercase();
        let Some(rest) = line.strip_prefix("[error] ") else {
            continue;
        };
        if rest.contains("authorization prompt dismissed") {
            return Some(StderrMatch::Denied);
        }
        if let Some(at) = rest.find("authoriz") {
            let after = &rest[at + "authoriz".len()..];
            if after.contains("timeout") || after.contains("timed out") {
                timeout = true;
            }
        }
    }
    timeout.then_some(StderrMatch::Timeout)
}

/// The `result` of the `end` record.
pub fn classify(status: Option<&Status>, op_exit: i32, mode: Mode, stderr: &[u8]) -> &'static str {
    let Some(status) = status else {
        return "launcher-failed";
    };
    if !status.started {
        if status.error.as_deref() == Some("lock-timeout") {
            return "lock-timeout";
        }
        return "launcher-failed";
    }
    if op_exit == 0 {
        return if mode == Mode::Desktop {
            "approved"
        } else {
            "ok"
        };
    }
    let seen = scan_stderr(stderr).or(match status.stderr_match.as_deref() {
        Some("denied") => Some(StderrMatch::Denied),
        Some("timeout") => Some(StderrMatch::Timeout),
        _ => None,
    });
    match seen {
        Some(StderrMatch::Denied) => "denied",
        Some(StderrMatch::Timeout) => "timeout",
        None => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started() -> Status {
        Status {
            started: true,
            ..Default::default()
        }
    }

    #[test]
    fn classifies() {
        let st = started();
        assert_eq!(classify(Some(&st), 0, Mode::Desktop, b""), "approved");
        assert_eq!(classify(Some(&st), 0, Mode::Service, b""), "ok");
        assert_eq!(classify(Some(&st), 0, Mode::Help, b""), "ok");
        assert_eq!(
            classify(
                Some(&st),
                1,
                Mode::Desktop,
                b"[ERROR] 2026/11/08 00:00:00 authorization prompt dismissed, please try again\n"
            ),
            "denied"
        );
        assert_eq!(
            classify(
                Some(&st),
                1,
                Mode::Desktop,
                b"[ERROR] 2026/11/08 00:00:00 authorization timeout\r\n"
            ),
            "timeout"
        );
        assert_eq!(
            classify(
                Some(&st),
                1,
                Mode::Desktop,
                b"[ERROR] 2026/11/08 00:00:00 Authorization Timed Out\n"
            ),
            "timeout"
        );
        assert_eq!(
            classify(
                Some(&st),
                1,
                Mode::Desktop,
                b"[ERROR] 2026/11/08 00:00:00 \"a\" isn't an item\n"
            ),
            "failed"
        );
        assert_eq!(
            classify(
                Some(&st),
                1,
                Mode::Desktop,
                b"curl: (28) Connection timed out after 1000 ms\n"
            ),
            "failed"
        );
        assert_eq!(classify(None, 0, Mode::Desktop, b""), "launcher-failed");
        assert_eq!(
            classify(Some(&Status::default()), 0, Mode::Desktop, b""),
            "launcher-failed"
        );
        let lock = Status {
            error: Some("lock-timeout".into()),
            ..Default::default()
        };
        assert_eq!(
            classify(Some(&lock), 75, Mode::Desktop, b""),
            "lock-timeout"
        );
        let scanned = Status {
            started: true,
            stderr_match: Some("denied".into()),
            ..Default::default()
        };
        assert_eq!(classify(Some(&scanned), 1, Mode::Desktop, b""), "denied");
    }
}

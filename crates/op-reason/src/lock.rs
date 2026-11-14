//! The WSL lock (`<state>/lock`, flock) and the pipe rules while waiting for it (design 5.5 and 6.4).

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use op_reason_core::lang::Lang;
use op_reason_core::log::{lock_with_timeout, open_lock_file};
use op_reason_core::messages;
use serde::{Deserialize, Serialize};

use crate::procfs;

/// `<state>/current.json`: who holds the lock.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Holder {
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub caller: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub pid: Option<u32>,
}

impl Holder {
    /// Reads `current.json`; `None` when it is missing or broken.
    pub fn read(state_dir: &Path) -> Option<Holder> {
        let text = fs::read_to_string(state_dir.join("current.json")).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Writes `current.json` (through a temporary file, so that readers never see half of it).
    pub fn write(&self, state_dir: &Path) -> bool {
        let tmp = state_dir.join(format!("current.json.{}", std::process::id()));
        let ok = fs::write(&tmp, serde_json::to_string(self).unwrap_or_default()).is_ok()
            && fs::rename(&tmp, state_dir.join("current.json")).is_ok();
        if !ok {
            let _ = fs::remove_file(&tmp);
        }
        ok
    }

    /// One line for messages.
    pub fn line(holder: Option<&Holder>, lang: Lang) -> String {
        match holder {
            Some(h) => messages::holder_line(lang, &h.time, &h.caller, &h.reason, &h.command),
            None => messages::unknown(lang).to_string(),
        }
    }
}

/// How the lock was settled.
pub enum Outcome {
    /// We hold it (the file must stay open until the call ends).
    Held(File),
    /// The holder reads our output: run without the lock.
    Skipped {
        /// The holder's reason, for the window's note.
        holder_reason: String,
    },
    /// The wait ran out.
    Timeout {
        /// Who held it, as one line.
        holder: String,
    },
}

/// Reads stdin to the end (a pipe fed by another op-reason).
pub fn drain_stdin() -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = std::io::stdin().lock().read_to_end(&mut buf);
    buf
}

/// Takes the lock, waiting up to `wait`. While waiting it checks every second: if another op-reason feeds our stdin,
/// stdin is read to the end first (`drained`) and the wait starts again; if the holder reads our output, the lock is
/// skipped. `on_wait` is called once when the lock is busy. Returns the outcome and the seconds spent waiting.
pub fn acquire(
    state_dir: &Path,
    wait: Duration,
    lang: Lang,
    drained: &mut Option<Vec<u8>>,
    on_wait: impl FnOnce(),
) -> std::io::Result<(Outcome, u64)> {
    let file = open_lock_file(&state_dir.join("lock"))?;
    let started = Instant::now();
    match file.try_lock() {
        Ok(()) => return Ok((Outcome::Held(file), 0)),
        Err(fs::TryLockError::WouldBlock) => {}
        Err(fs::TryLockError::Error(e)) => return Err(e),
    }
    on_wait();
    let mut deadline = Instant::now() + wait;
    loop {
        if drained.is_none() && procfs::op_upstream() {
            *drained = Some(drain_stdin());
            deadline = Instant::now() + wait;
        }
        if let Some(h) = Holder::read(state_dir)
            && let Some(pid) = h.pid
            && procfs::feeds(pid)
        {
            return Ok((
                Outcome::Skipped {
                    holder_reason: h.reason,
                },
                started.elapsed().as_secs(),
            ));
        }
        if Instant::now() >= deadline {
            let holder = Holder::line(Holder::read(state_dir).as_ref(), lang);
            return Ok((Outcome::Timeout { holder }, started.elapsed().as_secs()));
        }
        if lock_with_timeout(&file, Duration::from_secs(1)) {
            return Ok((Outcome::Held(file), started.elapsed().as_secs()));
        }
    }
}

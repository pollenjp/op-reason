//! `%LOCALAPPDATA%\op-reason\holder.json`: who holds the mutex, written by whoever holds it (the A' launcher or this
//! exe), so that a call that times out can say what it waited for. Same fields as WSL's `current.json`
//! (A' spec §5.5): `{"time":"HH:MM:SS","caller":"<short caller>","reason":"<100 chars>","command":"<100 chars>","pid":<pid>}`.

use std::fs;
use std::path::Path;

use op_reason_core::args::clip;
use op_reason_core::lang::Lang;
use op_reason_core::log::ensure_dir;
use op_reason_core::messages;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Holder {
    time: String,
    caller: String,
    reason: String,
    command: String,
    pid: u32,
}

const FILE: &str = "holder.json";

/// Writes the holder file for this call (failures are ignored).
pub fn write(dir: &Path, time: &str, caller: &str, reason: &str, command: &str) {
    if ensure_dir(dir).is_err() {
        return;
    }
    let h = Holder {
        time: time.to_owned(),
        caller: caller.to_owned(),
        reason: clip(reason, 100, "…"),
        command: clip(command, 100, "…"),
        pid: std::process::id(),
    };
    if let Ok(json) = serde_json::to_string(&h) {
        let _ = fs::write(dir.join(FILE), json);
    }
}

fn read(dir: &Path) -> Option<Holder> {
    serde_json::from_str(&fs::read_to_string(dir.join(FILE)).ok()?).ok()
}

/// Removes the holder file if it is still ours.
pub fn remove(dir: &Path) {
    if read(dir).is_some_and(|h| h.pid == std::process::id()) {
        let _ = fs::remove_file(dir.join(FILE));
    }
}

/// The holder as one line for the time-out message, or "(unknown)".
pub fn describe(dir: Option<&Path>, lang: Lang) -> String {
    match dir.and_then(read) {
        Some(h) => messages::holder_line(lang, &h.time, &h.caller, &h.reason, &h.command),
        None => messages::unknown(lang).to_owned(),
    }
}

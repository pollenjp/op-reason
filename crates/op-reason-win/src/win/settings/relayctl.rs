//! The relay seen from the settings (design `2026-11-23-settings-design.md` §8.2, §9.4): the process that answers on
//! its pipe, what it tells in `relay.json`, ending it through its stop event, and waiting for a new one to answer.

use std::path::Path;
use std::time::{Duration, Instant};

use op_reason_core::settings::relayinfo::{RELAY_INFO_FILE, RelayInfo};
use op_reason_core::settings::values::RELAY_PIPE;
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    EVENT_MODIFY_STATE, OpenEventW, OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, SetEvent,
    TerminateProcess, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, RegisterWindowMessageW};
use windows::core::{HSTRING, PCWSTR};

use crate::win::relay::pipe;

/// How long the relay gets to end by itself after its stop event (it lets requests with 1Password finish for 25 s).
const STOP_WAIT: Duration = Duration::from_secs(30);
/// How long a start waits for the pipe to answer.
const START_WAIT: Duration = Duration::from_secs(15);

/// The process that answers on the relay's pipe.
pub fn pid() -> Option<u32> {
    pipe::probe(RELAY_PIPE)
}

/// `relay.json` in the state folder, as it is.
pub fn info(state_dir: Option<&Path>) -> Option<RelayInfo> {
    let text = std::fs::read_to_string(state_dir?.join(RELAY_INFO_FILE)).ok()?;
    RelayInfo::parse(&text)
}

/// Tells the relay's icon window that `settings.json` changed (its icon follows `relay.tray-icon` at once); nothing
/// when no relay shows one.
pub fn tell_changed() {
    // SAFETY: plain calls with strings that live across them.
    unsafe {
        let class = HSTRING::from(crate::relay_opts::TRAY_CLASS);
        let Ok(hwnd) = FindWindowW(&class, PCWSTR::null()) else {
            return;
        };
        let message = RegisterWindowMessageW(&HSTRING::from(crate::relay_opts::SETTINGS_CHANGED));
        if message != 0 {
            let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
        }
    }
}

/// Waits up to 15 seconds for a relay to answer on the pipe, and then up to two seconds for its `relay.json` (it writes
/// the file just after its pipe), so that the settings read the relay that just started as op-reason's.
pub fn wait_up(state_dir: Option<&Path>) -> Option<u32> {
    let t0 = Instant::now();
    let pid = loop {
        if let Some(pid) = pid() {
            break pid;
        }
        if t0.elapsed() >= START_WAIT {
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    let t1 = Instant::now();
    while t1.elapsed() < Duration::from_secs(2) && info(state_dir).is_none_or(|i| i.pid != pid) {
        std::thread::sleep(Duration::from_millis(50));
    }
    Some(pid)
}

/// Ends the relay `pid`: its stop event (a relay of 0.6.0 has none), then `TerminateProcess` after 30 seconds; returns
/// once it has ended.
pub fn stop(pid: u32) -> Result<(), String> {
    // SAFETY: handles opened here are closed here.
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, false, pid)
            .map_err(|e| format!("the relay (pid {pid}): {}", e.message()))?;
        let ended = |ms: u32| WaitForSingleObject(process, ms) == WAIT_OBJECT_0;
        let name = HSTRING::from(format!(r"Local\op-reason-ssh-relay-stop-{pid}"));
        let mut done = false;
        if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, &name) {
            let set = SetEvent(event).is_ok();
            let _ = CloseHandle(event);
            done = set && ended(STOP_WAIT.as_millis() as u32);
        }
        if !done {
            let _ = TerminateProcess(process, 1);
            done = ended(5_000);
        }
        let _ = CloseHandle(process);
        if done {
            Ok(())
        } else {
            Err(format!("the relay (pid {pid}) did not end"))
        }
    }
}

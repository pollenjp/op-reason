//! What the relay sees of 1Password's prompt after a click that allows (design `2026-11-26-prompt-guide-design.md` §5):
//! the window in front and the visible top-level windows of 1Password's processes, compared with the moment the request
//! went to 1Password. [`crate::relay_prompt`] decides what a look means.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible,
};
use windows::core::BOOL;

use super::procs;
use crate::relay_prompt::{self, Look, Owner};

/// Looks for 1Password's prompt from the moment a request went to 1Password.
pub struct Looker {
    /// The process that serves 1Password's pipe.
    server: Option<u32>,
    /// The window in front when the request went.
    front: isize,
    /// The visible windows of 1Password when the request went.
    known: HashSet<isize>,
    /// Whose each process seen is (its image name is read once).
    owners: HashMap<u32, Owner>,
}

impl Looker {
    /// Notes the window in front and the visible windows of 1Password now, as the request goes.
    pub fn start(server: Option<u32>) -> Looker {
        let mut l = Looker {
            server,
            front: foreground().0 as isize,
            known: HashSet::new(),
            owners: HashMap::new(),
        };
        l.known = l.windows_of_1password().into_iter().collect();
        l
    }

    /// One look: whether a window of 1Password (or of Windows Hello) is in front that was not, and whether a window of
    /// 1Password is there that was not; only windows that a person can see count.
    pub fn look(&mut self) -> Look {
        let fg = foreground();
        let front = !fg.is_invalid()
            && fg.0 as isize != self.front
            && in_view(fg)
            && matches!(self.owner_of(pid_of(fg)), Owner::OnePassword | Owner::Hello);
        let new_window = self
            .windows_of_1password()
            .iter()
            .any(|h| !self.known.contains(h));
        Look { front, new_window }
    }

    fn owner_of(&mut self, pid: u32) -> Owner {
        if pid == 0 {
            return Owner::Other;
        }
        let server = self.server;
        *self.owners.entry(pid).or_insert_with(|| {
            relay_prompt::owner(pid, &procs::image_path(pid).unwrap_or_default(), server)
        })
    }

    /// The top-level windows of 1Password's processes that a person can see.
    fn windows_of_1password(&mut self) -> Vec<isize> {
        let windows = visible_windows();
        windows
            .into_iter()
            .filter(|h| self.owner_of(pid_of(*h)) == Owner::OnePassword && in_view(*h))
            .map(|h| h.0 as isize)
            .collect()
    }
}

fn foreground() -> HWND {
    // SAFETY: a plain query.
    unsafe { GetForegroundWindow() }
}

/// The process of a window (0 when it cannot be read).
fn pid_of(h: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: a plain query with an out parameter on the stack; a stale window gives no process.
    unsafe {
        GetWindowThreadProcessId(h, Some(&mut pid));
    }
    pid
}

/// Whether DWM hides the window although it is visible (a suspended app's window, a window on another desktop).
fn cloaked(h: HWND) -> bool {
    let mut value = 0u32;
    // SAFETY: a DWORD out parameter with its size.
    let read = unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_CLOAKED,
            &mut value as *mut u32 as *mut c_void,
            size_of::<u32>() as u32,
        )
    };
    read.is_ok() && value != 0
}

/// The window class's name (empty when it cannot be read).
fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: a buffer on the stack with its length; a stale window gives nothing.
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Whether a person can see the window: visible, not minimized, not hidden by DWM, and not a menu or a tooltip
/// (1Password's menu in the tray is a window of its process). None of these sends a message to the window's thread.
fn in_view(h: HWND) -> bool {
    // SAFETY: plain queries; a stale window is neither visible nor minimized.
    let shown = unsafe { IsWindowVisible(h).as_bool() && !IsIconic(h).as_bool() };
    shown && !cloaked(h) && !relay_prompt::never_a_prompt(&class_of(h))
}

/// Every visible top-level window (whether a person can see it is [`in_view`], asked of 1Password's windows only).
fn visible_windows() -> Vec<HWND> {
    unsafe extern "system" fn add(h: HWND, lp: LPARAM) -> BOOL {
        // SAFETY: `lp` is the vector of `visible_windows`, alive for the whole enumeration on this thread.
        unsafe {
            if IsWindowVisible(h).as_bool() {
                (*(lp.0 as *mut Vec<HWND>)).push(h);
            }
        }
        BOOL(1)
    }
    let mut out: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes into `out`, which outlives the call.
    unsafe {
        let _ = EnumWindows(Some(add), LPARAM(&mut out as *mut Vec<HWND> as isize));
    }
    out
}

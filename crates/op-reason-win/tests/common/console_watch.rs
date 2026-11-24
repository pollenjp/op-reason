//! Watching for console windows, shared by the Windows tests (`#[path = "common/console_watch.rs"] mod console_watch;`).
//!
//! Windows shows a console window for a console program that starts without a console to inherit (or hands the
//! console to Windows Terminal, whose window comes to the front), unless the program is started with
//! `CREATE_NO_WINDOW` or its manifest asks for no console (design
//! `docs/superpowers/specs/2026-11-24-relay-no-console-window-design.md`). Such a window may last a moment only, so the
//! watcher looks every 2 ms.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextW, IsWindowVisible,
};
use windows::core::BOOL;

unsafe extern "system" fn each_console(hwnd: HWND, found: LPARAM) -> BOOL {
    // SAFETY: `found` points at the vector of `visible_consoles` for the length of EnumWindows; the buffers outlive the
    // calls.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut class);
        let class = String::from_utf16_lossy(&class[..usize::try_from(n).unwrap_or(0)]);
        // conhost's window, or Windows Terminal's when it is the default terminal
        if class == "ConsoleWindowClass" || class == "CASCADIA_HOSTING_WINDOW_CLASS" {
            let mut title = [0u16; 512];
            let n = GetWindowTextW(hwnd, &mut title);
            let title = String::from_utf16_lossy(&title[..usize::try_from(n).unwrap_or(0)]);
            (*(found.0 as *mut Vec<String>)).push(format!("{class}: {title}"));
        }
    }
    true.into()
}

/// The visible console windows, as `<class>: <title>`.
pub fn visible_consoles() -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    // SAFETY: the callback pushes into `found`, which outlives the call.
    unsafe {
        let _ = EnumWindows(
            Some(each_console),
            LPARAM(&mut found as *mut Vec<String> as isize),
        );
    }
    found
}

/// Looks every 2 ms, on a thread of its own, for visible console windows whose title names `needle` (a program's path,
/// or the folder it is in): a console window titled with the program that Windows made it for.
pub struct ConsoleWatch {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Vec<String>>>,
}

impl ConsoleWatch {
    pub fn start(needle: &Path) -> ConsoleWatch {
        let needle = needle.display().to_string().to_lowercase();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut seen: Vec<String> = Vec::new();
            while !flag.load(Ordering::SeqCst) {
                for window in visible_consoles() {
                    if window.to_lowercase().contains(&needle) && !seen.contains(&window) {
                        seen.push(window);
                    }
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            seen
        });
        ConsoleWatch {
            stop,
            thread: Some(thread),
        }
    }

    /// Stops watching. Returns the windows seen, as `<class>: <title>`.
    pub fn stop(mut self) -> Vec<String> {
        self.stop.store(true, Ordering::SeqCst);
        self.thread
            .take()
            .map(|t| t.join().unwrap_or_default())
            .unwrap_or_default()
    }
}

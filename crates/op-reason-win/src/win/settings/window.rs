//! The settings window (design `2026-11-23-settings-design.md` §11): one per logon session, in a process of its own
//! (`op-reason.exe --settings window`) that `op-reason --settings` starts without a console.

use std::path::Path;
use std::time::{Duration, Instant};

use op_reason_core::contract;
use op_reason_core::lang::Lang;
use windows::Win32::Foundation::{
    CloseHandle, HINSTANCE, HWND, LPARAM, LRESULT, WAIT_ABANDONED, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::Graphics::Gdi::{COLOR_BTNFACE, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CreateMutexW, CreateProcessW, DETACHED_PROCESS, PROCESS_INFORMATION,
    STARTUPINFOW, WaitForSingleObject,
};
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
    DefWindowProcW, DispatchMessageW, FindWindowW, GetMessageW, IDC_ARROW, IsDialogMessageW,
    IsIconic, LoadCursorW, MB_ICONERROR, MB_OK, MSG, MessageBoxW, PostQuitMessage,
    RegisterClassExW, SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow, TranslateMessage,
    WM_DESTROY, WNDCLASSEXW, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT, WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

use crate::settings_ui;

/// The window's class: `op-reason --settings` finds an open window by it.
const CLASS_NAME: PCWSTR = w!("op-reason-settings");
/// Held by the window's process while it runs: one window per logon session.
const ONE_WINDOW: &str = r"Local\op-reason-settings-window";
/// The window's first size, in pixels at 96 DPI.
const WIDTH: i32 = 680;
const HEIGHT: i32 = 640;

/// Brings an open settings window to the front, or starts `op-reason.exe --settings window` without a console and lets
/// it take the front. Returns at once: a terminal gets its prompt back.
pub fn launch() -> Result<(), String> {
    if let Some(hwnd) = find() {
        front(hwnd);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| format!("op-reason.exe: {e}"))?;
    let pid = start_detached(&exe)?;
    // SAFETY: a plain call. This process may set the foreground window when the user just started it (a double click,
    // the Start menu); it hands that on to the window.
    unsafe {
        let _ = AllowSetForegroundWindow(pid);
    }
    Ok(())
}

/// Starts `<exe> --settings window` without a console and without handles of this process (a caller that reads this
/// process's output must not wait for the window), out of the caller's job when the job lets it go (a terminal's job
/// would end the window with the terminal). Returns its process ID.
fn start_detached(exe: &Path) -> Result<u32, String> {
    let app = HSTRING::from(exe.as_os_str());
    let dir = exe.parent().map(|d| HSTRING::from(d.as_os_str()));
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut why = String::new();
    for flags in [
        DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB,
        DETACHED_PROCESS,
    ] {
        let mut line: Vec<u16> = format!("\"{}\" --settings window", exe.display())
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut info = PROCESS_INFORMATION::default();
        // SAFETY: the strings and structures outlive the call; no handle is inherited; the handles it returns are
        // closed here.
        let made = unsafe {
            CreateProcessW(
                &app,
                Some(PWSTR(line.as_mut_ptr())),
                None,
                None,
                false,
                flags,
                None,
                dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
                &startup,
                &mut info,
            )
        };
        match made {
            Ok(()) => {
                // SAFETY: handles that CreateProcessW returned to this process.
                unsafe {
                    let _ = CloseHandle(info.hThread);
                    let _ = CloseHandle(info.hProcess);
                }
                return Ok(info.dwProcessId);
            }
            Err(e) => why = e.message(),
        }
    }
    Err(format!("{}: {why}", exe.display()))
}

/// The open settings window, when there is one.
fn find() -> Option<HWND> {
    // SAFETY: a static class name.
    unsafe { FindWindowW(CLASS_NAME, PCWSTR::null()) }
        .ok()
        .filter(|h| !h.is_invalid())
}

/// Brings `hwnd` to the front (Windows flashes its taskbar button instead when this process may not).
fn front(hwnd: HWND) {
    // SAFETY: plain calls on a window handle; a window that went away makes them fail harmlessly.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Whether another window's process holds the mutex of the one window. Otherwise this process holds it from then on,
/// until it ends (the handle stays open). A mutex that cannot be made does not keep the window from opening.
fn another_window_runs() -> bool {
    // SAFETY: a valid name; the handle is closed when another process holds the mutex, and kept open otherwise.
    unsafe {
        let Ok(h) = CreateMutexW(None, false, &HSTRING::from(ONE_WINDOW)) else {
            return false;
        };
        let r = WaitForSingleObject(h, 0);
        if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
            return false;
        }
        let _ = CloseHandle(h);
        true
    }
}

/// `op-reason.exe --settings window`: the window, until it is closed. Returns the exit code.
pub fn run(lang: Lang) -> i32 {
    if another_window_runs() {
        // Two launches at the same moment: the first window comes to the front once it is there
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(5) {
            if let Some(hwnd) = find() {
                front(hwnd);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        return 0;
    }
    match open(lang) {
        Ok(()) => 0,
        Err(e) => {
            let text = HSTRING::from(settings_ui::not_started(lang, &e.message()));
            // SAFETY: a message box without an owner; the strings outlive the call.
            unsafe {
                MessageBoxW(None, &text, w!("op-reason"), MB_OK | MB_ICONERROR);
            }
            contract::EXIT_USAGE
        }
    }
}

fn register_class(instance: HINSTANCE) -> windows::core::Result<()> {
    // SAFETY: a class with a static name and a valid window procedure; the structure outlives the call.
    unsafe {
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as usize as *mut core::ffi::c_void),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
    }
    Ok(())
}

/// Makes the window and runs its messages until it is closed.
fn open(lang: Lang) -> windows::core::Result<()> {
    let texts = settings_ui::texts(lang);
    // SAFETY: the window is made and its messages run on this thread; the strings outlive the calls.
    unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        register_class(instance)?;
        let dpi = GetDpiForSystem();
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            CLASS_NAME,
            &HSTRING::from(texts.title),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(WIDTH, dpi),
            scale(HEIGHT, dpi),
            None,
            None,
            Some(instance),
            None,
        )?;
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = SetForegroundWindow(hwnd);
        let mut msg = MSG::default();
        loop {
            let got = GetMessageW(&mut msg, None, 0, 0).0;
            if got == 0 || got == -1 {
                break;
            }
            if IsDialogMessageW(hwnd, &msg).as_bool() {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// A length at 96 DPI in pixels at `dpi`.
fn scale(value: i32, dpi: u32) -> i32 {
    (i64::from(value) * i64::from(dpi) / 96) as i32
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: the default procedure with the arguments as received.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

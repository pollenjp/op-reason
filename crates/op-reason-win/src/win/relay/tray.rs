//! The relay's icon in the notification area (design `2026-11-23-settings-design.md` §12): a thread of its own with a
//! hidden top-level window of the class `op-reason-ssh-relay-tray`, `Shell_NotifyIconW` with `NOTIFYICON_VERSION_4`,
//! and an icon drawn at run time (the exe has no icon resource). The icon is added again when Explorer starts again
//! (`TaskbarCreated`), and tried every two seconds until it is added (at logon the task may start the relay before
//! Explorer has a taskbar). `relay.tray-icon` applies at once: a writer of `settings.json` posts
//! `op-reason-settings-changed`, and the window reads the file again. The menu's actions run `op-reason.exe --settings
//! …` in a process of its own: the window never opens inside the relay.

use std::cell::RefCell;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::thread;

use op_reason_core::lang::{self, Lang};
use op_reason_core::settings;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcess, PROC_THREAD_ATTRIBUTE_PARENT_PROCESS,
    PROCESS_CREATE_PROCESS, PROCESS_INFORMATION, STARTUPINFOEXW, STARTUPINFOW,
    UpdateProcThreadAttribute,
};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NIN_SELECT, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONDATAW_0, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, AppendMenuW, ChangeWindowMessageFilterEx,
    CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, FindWindowW, GetMessageW, GetSystemMetrics, GetWindowThreadProcessId, HICON,
    ICONINFO, KillTimer, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, MSGFLT_ALLOW, PostMessageW,
    RegisterClassExW, RegisterWindowMessageW, SM_CXSMICON, SM_MENUDROPALIGNMENT,
    SetForegroundWindow, SetMenuDefaultItem, SetTimer, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TrackPopupMenuEx,
    TranslateMessage, WINDOW_EX_STYLE, WM_COMMAND, WM_CONTEXTMENU, WM_NULL, WM_TIMER, WNDCLASSEXW,
    WS_OVERLAPPED,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

use super::app;
use crate::relay_opts::{SETTINGS_CHANGED, SETTINGS_FROM, TRAY_CALLBACK, TRAY_CLASS, TRAY_QUERY};
use crate::tray_ui::{self, Item};

/// `NIN_SELECT | NINF_KEY`: Enter or the space bar on the icon (the windows crate does not name it).
const NIN_KEYSELECT: u32 = NIN_SELECT | 0x1;
/// The icon's ID within its window.
const ICON_ID: u32 = 1;
/// The timer that tries to add the icon again.
const TIMER_RETRY: usize = 1;
const RETRY_MS: u32 = 2_000;

/// The icon's window, for [`remove`] at the relay's exit.
static WINDOW: AtomicIsize = AtomicIsize::new(0);
/// The icon is in the notification area.
static SHOWN: AtomicBool = AtomicBool::new(false);

/// What the icon's window knows, on its thread.
struct Tray {
    mode: String,
    ours: bool,
    lang: Lang,
    icon: HICON,
    taskbar_created: u32,
    changed: u32,
    query: u32,
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// Starts the icon's thread for a relay in `mode`; `ours` when op-reason's task started it (it passes `--task`), which
/// Stop and Restart need. Quiet when it cannot.
pub fn start(mode: &str, ours: bool) {
    let mode = mode.to_owned();
    let _ = thread::Builder::new()
        .name("tray".into())
        .spawn(move || run(mode, ours));
}

/// Takes the icon away as the relay exits (an icon left behind stays until the pointer passes over it).
pub fn remove() {
    let hwnd = WINDOW.load(Ordering::SeqCst);
    if hwnd != 0 && SHOWN.swap(false, Ordering::SeqCst) {
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: HWND(hwnd as *mut c_void),
            uID: ICON_ID,
            ..Default::default()
        };
        // SAFETY: the structure outlives the call.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
    }
}

fn run(mode: String, ours: bool) {
    // SAFETY: the window is made and its messages run on this thread; the strings outlive the calls.
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let instance: HINSTANCE = module.into();
        let class_name = HSTRING::from(TRAY_CLASS);
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(tray_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return;
        }
        // A hidden top-level window: a message-only one would not get TaskbarCreated
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            &class_name,
            w!("op-reason SSH relay"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        ) else {
            return;
        };
        WINDOW.store(hwnd.0 as isize, Ordering::SeqCst);
        let size = GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()).max(16) as u32;
        let tray = Tray {
            mode,
            ours,
            lang: Lang::En,
            icon: make_icon(size).unwrap_or_default(),
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            changed: RegisterWindowMessageW(&HSTRING::from(SETTINGS_CHANGED)),
            query: RegisterWindowMessageW(&HSTRING::from(TRAY_QUERY)),
        };
        // A relay started from an elevated terminal still hears Explorer and the writers of settings.json
        for message in [
            tray.taskbar_created,
            tray.changed,
            tray.query,
            TRAY_CALLBACK,
            WM_COMMAND,
        ] {
            let _ = ChangeWindowMessageFilterEx(hwnd, message, MSGFLT_ALLOW, None);
        }
        TRAY.with(|t| *t.borrow_mut() = Some(tray));
        follow(hwnd);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// The icon drawn at `size` pixels (`tray_ui::icon_pixels`), with its alpha.
fn make_icon(size: u32) -> Option<HICON> {
    let pixels = tray_ui::icon_pixels(size);
    let side = size as i32;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: side,
            // top row first
            biHeight: -side,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: the DIB section has room for size × size pixels of 32 bits; the mask's bits outlive its making; the
    // bitmaps are deleted once the icon holds its own copies.
    unsafe {
        let mut bits: *mut c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(color.into());
            return None;
        }
        // 0xAARRGGBB in memory is B, G, R, A: what a 32-bit DIB holds
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, pixels.len());
        let row = size.div_ceil(16) as usize * 2;
        let zeros = vec![0u8; row * size as usize];
        let mask = CreateBitmap(side, side, 1, 1, Some(zeros.as_ptr() as *const c_void));
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        })
        .ok();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

fn with_tray<R>(f: impl FnOnce(&mut Tray) -> R) -> Option<R> {
    TRAY.with(|t| t.borrow_mut().as_mut().map(f))
}

/// Reads `settings.json` again: the language, and whether the icon is wanted.
fn follow(hwnd: HWND) {
    let text = app::settings_file()
        .and_then(|f| std::fs::read(f).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let values = settings::values(text.as_deref());
    let lang = lang::pick(values.lang);
    with_tray(|t| t.lang = lang);
    if values.tray_icon.unwrap_or(true) {
        show(hwnd);
    } else {
        hide(hwnd);
    }
}

/// The icon's data: its window, ID, icon, tooltip and messages.
fn data(hwnd: HWND) -> Option<NOTIFYICONDATAW> {
    with_tray(|t| {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: TRAY_CALLBACK,
            hIcon: t.icon,
            Anonymous: NOTIFYICONDATAW_0 {
                uVersion: NOTIFYICON_VERSION_4,
            },
            ..Default::default()
        };
        let tip: Vec<u16> = tray_ui::tip(t.lang, &t.mode)
            .encode_utf16()
            .take(data.szTip.len() - 1)
            .collect();
        data.szTip[..tip.len()].copy_from_slice(&tip);
        data
    })
}

/// Adds the icon, or brings its tooltip up to date; tries again in two seconds when the notification area is not there
/// yet.
fn show(hwnd: HWND) {
    let Some(data) = data(hwnd) else {
        return;
    };
    // SAFETY: the structure outlives the calls; the timer is this window's.
    unsafe {
        let shown = SHOWN.load(Ordering::SeqCst) && Shell_NotifyIconW(NIM_MODIFY, &data).as_bool();
        let added = shown
            || Shell_NotifyIconW(NIM_ADD, &data).as_bool()
            || Shell_NotifyIconW(NIM_MODIFY, &data).as_bool();
        if added {
            if !shown {
                let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
            }
            SHOWN.store(true, Ordering::SeqCst);
            let _ = KillTimer(Some(hwnd), TIMER_RETRY);
        } else {
            SHOWN.store(false, Ordering::SeqCst);
            SetTimer(Some(hwnd), TIMER_RETRY, RETRY_MS, None);
        }
    }
}

fn hide(hwnd: HWND) {
    // SAFETY: the timer is this window's.
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_RETRY);
    }
    remove();
}

/// Runs a menu item: `op-reason.exe --settings …` in a process of its own, with a console that is never shown
/// (`CREATE_NO_WINDOW`: the console programs it starts share it), recorded as from the icon. op-reason's task runs its relay in a job of Task Scheduler, which counts the task as running while any process
/// of the job lives ("Do not start a new instance" would then ignore the start of a new relay): the action runs with
/// Explorer as its parent, which takes it out of that job, and otherwise leaves the job when the job lets it go.
fn act(item: Item) {
    let ours = with_tray(|t| t.ours).unwrap_or(false);
    if item.needs_ours() && !ours {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // SAFETY: a plain call. The click on the icon was this process's input: the window or Explorer may come to the
    // front.
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
    if ours && under_explorer(&exe, item.args()).is_ok() {
        return;
    }
    let spawn = |flags: u32| {
        Command::new(&exe)
            .args(item.args())
            .env(SETTINGS_FROM, "tray")
            .creation_flags(flags)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    if spawn((CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB).0).is_err() {
        let _ = spawn(CREATE_NO_WINDOW.0);
    }
}

/// This process's variables with `SETTINGS_FROM=tray`, as an environment block (sorted, UTF-16).
fn env_block() -> Vec<u16> {
    let mut vars: std::collections::BTreeMap<String, (std::ffi::OsString, std::ffi::OsString)> =
        std::env::vars_os()
            .map(|(k, v)| (k.to_string_lossy().to_uppercase(), (k, v)))
            .collect();
    vars.insert(
        SETTINGS_FROM.to_owned(),
        (SETTINGS_FROM.into(), "tray".into()),
    );
    let mut block = Vec::new();
    for (k, v) in vars.values() {
        block.extend(k.encode_wide());
        block.push(u16::from(b'='));
        block.extend(v.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

/// Starts `<exe> <args>` with Explorer (the taskbar's process) as its parent (`PROC_THREAD_ATTRIBUTE_PARENT_PROCESS`):
/// in Explorer's job rather than this process's, with a console that is never shown, with this process's variables and
/// `SETTINGS_FROM=tray`, in the exe's folder.
fn under_explorer(exe: &std::path::Path, args: &[&str]) -> Result<u32, String> {
    // SAFETY: the handles opened here are closed here; the attribute list, the strings and the structures outlive the
    // calls that use them.
    unsafe {
        let taskbar = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null())
            .map_err(|e| format!("Explorer: {}", e.message()))?;
        let mut pid = 0u32;
        GetWindowThreadProcessId(taskbar, Some(&mut pid));
        if pid == 0 {
            return Err("Explorer: no process".into());
        }
        let parent = OpenProcess(PROCESS_CREATE_PROCESS, false, pid)
            .map_err(|e| format!("Explorer: {}", e.message()))?;
        let mut size = 0usize;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        // u64: the list holds pointers
        let mut buffer = vec![0u64; size.div_ceil(8).max(1)];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(buffer.as_mut_ptr() as *mut c_void);
        let made = InitializeProcThreadAttributeList(Some(list), 1, None, &mut size)
            .and_then(|()| {
                UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_PARENT_PROCESS as usize,
                    Some(&parent as *const HANDLE as *const c_void),
                    size_of::<HANDLE>(),
                    None,
                    None,
                )
            })
            .and_then(|()| {
                let mut line: Vec<u16> = std::iter::once(format!("\"{}\"", exe.display()))
                    .chain(args.iter().map(|a| (*a).to_owned()))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let env = env_block();
                let dir = exe.parent().map(|d| HSTRING::from(d.as_os_str()));
                let startup = STARTUPINFOEXW {
                    StartupInfo: STARTUPINFOW {
                        cb: size_of::<STARTUPINFOEXW>() as u32,
                        ..Default::default()
                    },
                    lpAttributeList: list,
                };
                let mut info = PROCESS_INFORMATION::default();
                CreateProcessW(
                    &HSTRING::from(exe.as_os_str()),
                    Some(PWSTR(line.as_mut_ptr())),
                    None,
                    None,
                    false,
                    CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                    Some(env.as_ptr() as *const c_void),
                    dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
                    &startup.StartupInfo,
                    &mut info,
                )
                .map(|()| info)
            });
        DeleteProcThreadAttributeList(list);
        let _ = CloseHandle(parent);
        let info = made.map_err(|e| e.message())?;
        let _ = CloseHandle(info.hThread);
        let _ = CloseHandle(info.hProcess);
        Ok(info.dwProcessId)
    }
}

/// The icon's menu at (`x`, `y`) on the screen.
fn menu(hwnd: HWND, x: i32, y: i32) {
    let Some((lang, mode, ours)) = with_tray(|t| (t.lang, t.mode.clone(), t.ours)) else {
        return;
    };
    // SAFETY: a menu made, shown and destroyed here; the strings outlive their calls.
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let line = HSTRING::from(tray_ui::line(lang, &mode, std::process::id(), ours));
        let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 1, &line);
        for item in Item::ALL {
            if matches!(item, Item::Open | Item::Restart | Item::Hide) {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            }
            let flags = if item.needs_ours() && !ours {
                MF_STRING | MF_GRAYED
            } else {
                MF_STRING
            };
            let _ = AppendMenuW(
                menu,
                flags,
                usize::from(item.id()),
                &HSTRING::from(item.text(lang)),
            );
        }
        let _ = SetMenuDefaultItem(menu, u32::from(Item::Open.id()), 0);
        // the menu closes when the user clicks elsewhere only when its window is in front
        let _ = SetForegroundWindow(hwnd);
        let align = if GetSystemMetrics(SM_MENUDROPALIGNMENT) != 0 {
            TPM_RIGHTALIGN
        } else {
            TPM_LEFTALIGN
        };
        let chosen = TrackPopupMenuEx(
            menu,
            (TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | align).0,
            x,
            y,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        if let Some(item) = u16::try_from(chosen.0).ok().and_then(Item::from_id) {
            act(item);
        }
    }
}

unsafe extern "system" fn tray_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let (taskbar_created, changed, query) =
        with_tray(|t| (t.taskbar_created, t.changed, t.query)).unwrap_or((0, 0, 0));
    match msg {
        TRAY_CALLBACK => {
            // NOTIFYICON_VERSION_4: the event in the low word of lparam, the anchor point in wparam
            let event = (lparam.0 & 0xFFFF) as u32;
            match event {
                NIN_SELECT | NIN_KEYSELECT => act(Item::Open),
                WM_CONTEXTMENU => {
                    let x = (wparam.0 & 0xFFFF) as u16 as i16 as i32;
                    let y = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                    menu(hwnd, x, y);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            if let Some(item) = Item::from_id((wparam.0 & 0xFFFF) as u16) {
                act(item);
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_RETRY => {
            follow(hwnd);
            LRESULT(0)
        }
        m if m != 0 && m == taskbar_created => {
            // Explorer started again: its notification area has no icons
            SHOWN.store(false, Ordering::SeqCst);
            follow(hwnd);
            LRESULT(0)
        }
        m if m != 0 && m == changed => {
            follow(hwnd);
            LRESULT(0)
        }
        m if m != 0 && m == query => LRESULT(isize::from(SHOWN.load(Ordering::SeqCst))),
        // SAFETY: the default procedure with the arguments as received.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

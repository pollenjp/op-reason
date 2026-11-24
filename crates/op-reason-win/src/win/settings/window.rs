//! The settings window (design `2026-11-23-settings-design.md` §11): one per logon session, in a process of its own
//! (`op-reason.exe --settings window`) that `op-reason --settings` starts without a console. Every control calls the
//! core's engine (recorded with `from: window`) on a worker thread of its own, so that the window stays responsive while
//! the relay or a WSL distribution takes its time.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use op_reason_core::contract;
use op_reason_core::lang::{self, Lang};
use op_reason_core::settings::catalog::{Name, Want};
use op_reason_core::settings::cli::RelayAct;
use op_reason_core::settings::engine::{self, Change, Ctx, Item, System};
use op_reason_core::settings::text;
use windows::Win32::Foundation::{
    COLORREF, CloseHandle, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_ABANDONED,
    WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, COLOR_WINDOWTEXT, GetMonitorInfoW, GetSysColor, GetSysColorBrush, HBRUSH, HDC,
    MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, SetBkColor, SetTextColor,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED,
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW, CreateMutexW, CreateProcessW, PROCESS_INFORMATION,
    STARTUPINFOW, WaitForSingleObject,
};
use windows::Win32::UI::Controls::{
    ICC_LISTVIEW_CLASSES, ICC_STANDARD_CLASSES, ICC_UPDOWN_CLASS, INITCOMMONCONTROLSEX,
    InitCommonControlsEx, LVN_ITEMCHANGED, NMHDR,
};
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled, SetFocus};
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
    SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, BN_CLICKED, CBN_SELCHANGE, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EN_CHANGE,
    EN_KILLFOCUS, FindWindowW, GWLP_USERDATA, GetClientRect, GetCursorPos, GetMessageW,
    GetWindowLongPtrW, IDC_ARROW, IDOK, IsDialogMessageW, IsIconic, IsWindow, KillTimer,
    LoadCursorW, MB_ICONERROR, MB_ICONQUESTION, MB_OK, MB_OKCANCEL, MINMAXINFO, MSG, MessageBoxW,
    PostMessageW, PostQuitMessage, RegisterClassExW, SW_RESTORE, SW_SHOWNORMAL, SWP_NOACTIVATE,
    SWP_NOZORDER, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLOSE, WM_COMMAND,
    WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_GETMINMAXINFO, WM_NCDESTROY, WM_NOTIFY,
    WM_SIZE, WM_TIMER, WNDCLASSEXW, WS_EX_CONTROLPARENT, WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

use super::controls::{self, Controls, Fonts, Metrics, list};
use super::{WinSystem, app};
use crate::settings_ui::{self, SkillRow, ids};

/// The window's class: `op-reason --settings` finds an open window by it.
const CLASS_NAME: PCWSTR = w!("op-reason-settings");
/// Held by the window's process while it runs: one window per logon session.
const ONE_WINDOW: &str = r"Local\op-reason-settings-window";
/// The window's style. Without `WS_CLIPCHILDREN`, as a dialog: the window paints its background inside the group boxes,
/// which paint only their frames.
const STYLE: WINDOW_STYLE = WS_OVERLAPPEDWINDOW;
const EX_STYLE: WINDOW_EX_STYLE = WS_EX_CONTROLPARENT;
/// The window's width, in pixels at 96 DPI (its height follows from the layout).
const WIDTH: i32 = 700;
/// The narrowest the window may be made.
const MIN_WIDTH: i32 = 480;
/// The worker finished a job.
const WM_APP_DONE: u32 = WM_APP + 1;
/// The time fields are written this long after their last change.
const TIMER_TIMES: usize = 1;
const TIMES_DELAY_MS: u32 = 800;

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

/// Starts `<exe> --settings window` with a console of its own that is never shown (`CREATE_NO_WINDOW`; the console
/// programs it starts, `wsl.exe` for every read, are started with `CREATE_NO_WINDOW` themselves) and without handles of
/// this process (a caller that reads this process's output must not wait for the window), out of the caller's job when
/// the job lets it go (a terminal's job would end the window with the terminal). Returns its process ID.
fn start_detached(exe: &Path) -> Result<u32, String> {
    let app = HSTRING::from(exe.as_os_str());
    let dir = exe.parent().map(|d| HSTRING::from(d.as_os_str()));
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut why = String::new();
    for flags in [
        CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB,
        CREATE_NO_WINDOW,
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

/// Takes the mutex of the one window: `true` when this process holds it from now on, until it ends (the handle stays
/// open), or when the mutex cannot be made (which does not keep the window from opening).
fn take_one_window() -> bool {
    // SAFETY: a valid name; the handle is closed when another process holds the mutex, and kept open otherwise.
    unsafe {
        let Ok(h) = CreateMutexW(None, false, &HSTRING::from(ONE_WINDOW)) else {
            return true;
        };
        let r = WaitForSingleObject(h, 0);
        if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
            return true;
        }
        let _ = CloseHandle(h);
        false
    }
}

/// `op-reason.exe --settings window`: the window, until it is closed. Returns the exit code.
pub fn run(lang: Lang) -> i32 {
    // Another window's process holds the mutex: its window comes to the front. Without a window it is either making
    // one (two launches at the same moment) or ending (its window was just closed): wait for either.
    let t0 = Instant::now();
    while !take_one_window() {
        if let Some(hwnd) = find() {
            front(hwnd);
            return 0;
        }
        if t0.elapsed() >= Duration::from_secs(10) {
            return 0;
        }
        std::thread::sleep(Duration::from_millis(100));
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
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as usize as *mut c_void),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
    }
    Ok(())
}

/// Makes the window and runs its messages until it is closed; then waits for the job in progress.
fn open(lang: Lang) -> windows::core::Result<()> {
    // SAFETY: COM for this thread (the folder picker), apartment-threaded as Microsoft documents for the shell's
    // dialogs; the window is made and its messages run on this thread; the strings outlive the calls.
    unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let classes = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES | ICC_UPDOWN_CLASS | ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&classes);
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        register_class(instance)?;
        let dpi = GetDpiForSystem();
        let hwnd = CreateWindowExW(
            EX_STYLE,
            CLASS_NAME,
            &HSTRING::from(settings_ui::texts(lang).title),
            STYLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            controls::scale(WIDTH, dpi),
            controls::scale(700, dpi),
            None,
            None,
            Some(instance),
            None,
        )?;
        let app = Box::new(App::new(hwnd, instance, lang)?);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*app as *const App as isize);
        app.start();
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
        drop(app);
        if com.is_ok() {
            CoUninitialize();
        }
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: GWLP_USERDATA holds the App of `open` from its making until WM_NCDESTROY, and the App outlives the window.
    unsafe {
        let app = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const App;
        if msg == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        } else if let Some(app) = app.as_ref()
            && let Some(result) = app.handle(msg, wparam, lparam)
        {
            return result;
        }
        if msg == WM_DESTROY {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// What the worker does.
enum Job {
    /// Reads everything again.
    Load,
    /// Writes file settings (the relay is not started again).
    Write(Vec<(Name, Want)>),
    /// Sets settings that are not in the file (the task, the skill places, the Start menu).
    Set(Vec<(Name, Want)>),
    /// Starts, stops or restarts the relay.
    Relay(RelayAct),
    /// Writes the relay's values and restarts op-reason's relay.
    Apply(Vec<(Name, Want)>),
    /// Looks at one place (a WSL distribution that does not run is started).
    Check(Name),
    /// Forgets the accounts remembered for links.
    Forget,
    /// Opens the records folder.
    OpenRecords,
}

/// What a job did, and everything read again after it.
struct Outcome {
    changes: Vec<Change>,
    checked: Option<Item>,
    items: Vec<Item>,
    records: String,
    accounts: usize,
}

/// The worker: one job at a time with the engine, each followed by a read of every setting.
fn work(jobs: Receiver<Job>, done: Sender<Outcome>, hwnd: usize) {
    // SAFETY: COM for this thread (Task Scheduler, the shortcut), uninitialized after.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    {
        let sys = WinSystem::new();
        let ctx = Ctx {
            sys: &sys,
            version: app::VERSION,
            skill: app::SKILL,
            from: "window",
        };
        for job in jobs {
            let mut checked = None;
            let changes = match job {
                Job::Load => Vec::new(),
                Job::Write(wants) => engine::write(&ctx, &wants),
                Job::Set(wants) => engine::set(&ctx, &wants),
                Job::Relay(act) => engine::relay(&ctx, act),
                Job::Apply(wants) => {
                    let mut changes = engine::write(&ctx, &wants);
                    changes.extend(engine::relay(&ctx, RelayAct::Restart));
                    changes
                }
                Job::Check(name) => {
                    checked = Some(engine::get(&ctx, &name));
                    Vec::new()
                }
                Job::Forget => engine::forget_accounts(&ctx),
                Job::OpenRecords => engine::open_records(&ctx),
            };
            // A window that is gone (closed while jobs waited) needs no read: the process ends sooner
            // SAFETY: a plain question about a window handle.
            if !unsafe { IsWindow(Some(HWND(hwnd as *mut c_void))) }.as_bool() {
                continue;
            }
            let outcome = Outcome {
                changes,
                checked,
                items: engine::list(&ctx),
                records: sys.records_where(),
                accounts: sys.accounts(),
            };
            if done.send(outcome).is_err() {
                break;
            }
            // SAFETY: a message to the window; one that is gone makes it fail harmlessly.
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    WM_APP_DONE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }
    if com.is_ok() {
        // SAFETY: pairs the initialization above.
        unsafe { CoUninitialize() };
    }
}

/// The window's state, on its thread.
struct App {
    hwnd: HWND,
    ui: Controls,
    fonts: RefCell<Fonts>,
    dpi: Cell<u32>,
    lang: Cell<Lang>,
    items: RefCell<Vec<Item>>,
    rows: RefCell<Vec<SkillRow>>,
    accounts: Cell<usize>,
    /// Jobs handed to the worker and not done yet: the controls that act are greyed out until none is left. A job
    /// asked for meanwhile (by a question's answer, the folder picker, a field's timer) waits for its turn.
    pending: Cell<usize>,
    /// What the jobs since the window was last idle did, shown when the last one is done.
    batch: RefCell<Vec<Change>>,
    batch_checked: RefCell<Option<Item>>,
    /// A time field was changed and is not written yet.
    times_dirty: Cell<bool>,
    /// A line to show with the next result (a time field out of its bounds).
    note: RefCell<Option<String>>,
    /// The window fills its controls: their notifications are not the user's.
    updating: Cell<bool>,
    /// The control that had the focus when the job started (greyed out, it lost it).
    focus: Cell<Option<HWND>>,
    jobs: RefCell<Option<Sender<Job>>>,
    done: Receiver<Outcome>,
    worker: RefCell<Option<JoinHandle<()>>>,
}

impl Drop for App {
    fn drop(&mut self) {
        // The job in progress (a relay that stops, a distribution that starts) ends before the process does
        self.jobs.borrow_mut().take();
        if let Some(worker) = self.worker.borrow_mut().take() {
            let _ = worker.join();
        }
    }
}

fn loword(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

fn hiword(value: usize) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}

impl App {
    fn new(hwnd: HWND, instance: HINSTANCE, lang: Lang) -> windows::core::Result<App> {
        // SAFETY: reads the window's DPI.
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let ui = Controls::create(hwnd, instance)?;
        let fonts = Fonts::new(dpi);
        ui.set_font(fonts.normal);
        ui.set_texts(&settings_ui::texts(lang));
        let (jobs, job_rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let target = hwnd.0 as usize;
        let worker = std::thread::Builder::new()
            .name("settings-worker".into())
            .spawn(move || work(job_rx, done_tx, target))
            .map_err(|_| windows::core::Error::from_thread())?;
        Ok(App {
            hwnd,
            ui,
            fonts: RefCell::new(fonts),
            dpi: Cell::new(dpi),
            lang: Cell::new(lang),
            items: RefCell::new(Vec::new()),
            rows: RefCell::new(Vec::new()),
            accounts: Cell::new(0),
            pending: Cell::new(0),
            batch: RefCell::new(Vec::new()),
            batch_checked: RefCell::new(None),
            times_dirty: Cell::new(false),
            note: RefCell::new(None),
            updating: Cell::new(false),
            focus: Cell::new(None),
            jobs: RefCell::new(Some(jobs)),
            done,
            worker: RefCell::new(Some(worker)),
        })
    }

    /// Reads everything (the controls are greyed before the window shows), and shows the window in front, in the middle
    /// of the work area where the pointer is, as high as its layout wants and the work area allows.
    fn start(&self) {
        self.run(Job::Load);
        let dpi = self.dpi.get();
        let width = controls::scale(WIDTH, dpi);
        let (w, h) = self.outer(width, self.ui.preferred_height(width, &self.metrics()));
        let area = work_area();
        let h = h.min(area.bottom - area.top);
        let x = area.left + ((area.right - area.left) - w).max(0) / 2;
        let y = area.top + ((area.bottom - area.top) - h).max(0) / 2;
        // SAFETY: places and shows this window.
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
            let _ = ShowWindow(self.hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.hwnd);
        }
        self.arrange();
    }

    fn metrics(&self) -> Metrics {
        let fonts = self.fonts.borrow();
        Metrics {
            dpi: self.dpi.get(),
            font: fonts.normal,
            char_height: fonts.char_height,
        }
    }

    /// The window's size for a client area `w` × `h`.
    fn outer(&self, w: i32, h: i32) -> (i32, i32) {
        let mut r = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        // SAFETY: the rectangle outlives the call.
        let _ = unsafe { AdjustWindowRectExForDpi(&mut r, STYLE, false, EX_STYLE, self.dpi.get()) };
        (r.right - r.left, r.bottom - r.top)
    }

    fn arrange(&self) {
        let mut client = RECT::default();
        // SAFETY: the rectangle outlives the call.
        let _ = unsafe { GetClientRect(self.hwnd, &mut client) };
        self.ui.arrange(
            client.right - client.left,
            client.bottom - client.top,
            &self.metrics(),
        );
    }

    fn handle(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_SIZE => {
                self.arrange();
                Some(LRESULT(0))
            }
            WM_GETMINMAXINFO => {
                // at its narrowest the mode and the times take two rows: the smallest height is measured there
                let dpi = self.dpi.get();
                let narrowest = controls::scale(MIN_WIDTH, dpi);
                let need = self.ui.arrange_need(narrowest, &self.metrics());
                let (w, h) = self.outer(narrowest, need);
                // SAFETY: lparam points at the MINMAXINFO of this message.
                let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                info.ptMinTrackSize = POINT { x: w, y: h };
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                self.on_dpi_changed(u32::from(hiword(wparam.0)), lparam);
                Some(LRESULT(0))
            }
            WM_CTLCOLORSTATIC => {
                // labels, check boxes and group boxes on the window's own background
                let dc = HDC(wparam.0 as *mut c_void);
                // SAFETY: the DC of this message; system colors and brushes.
                unsafe {
                    SetBkColor(dc, COLORREF(GetSysColor(COLOR_BTNFACE)));
                    SetTextColor(dc, COLORREF(GetSysColor(COLOR_WINDOWTEXT)));
                    Some(LRESULT(GetSysColorBrush(COLOR_BTNFACE).0 as isize))
                }
            }
            WM_COMMAND => {
                self.on_command(loword(wparam.0), u32::from(hiword(wparam.0)));
                Some(LRESULT(0))
            }
            WM_NOTIFY => {
                // SAFETY: lparam points at the NMHDR of this notification.
                let hdr = unsafe { &*(lparam.0 as *const NMHDR) };
                if hdr.idFrom == usize::from(ids::SKILLS)
                    && hdr.code == LVN_ITEMCHANGED
                    && !self.updating.get()
                {
                    self.enable_skill_buttons();
                }
                None
            }
            WM_TIMER if wparam.0 == TIMER_TIMES => {
                self.on_times();
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                // a value typed a moment ago is written before the window goes (the worker ends after its jobs)
                self.flush_times();
                None
            }
            WM_APP_DONE => {
                self.on_done();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn on_dpi_changed(&self, dpi: u32, lparam: LPARAM) {
        self.dpi.set(dpi);
        let fonts = Fonts::new(dpi);
        self.ui.set_font(fonts.normal);
        // the old font goes once the controls have the new one
        *self.fonts.borrow_mut() = fonts;
        // SAFETY: lparam points at the suggested rectangle of this message.
        let r = unsafe { &*(lparam.0 as *const RECT) };
        // SAFETY: moves this window to the rectangle Windows suggests.
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        self.arrange();
    }

    fn texts(&self) -> settings_ui::Texts {
        settings_ui::texts(self.lang.get())
    }

    fn busy(&self) -> bool {
        self.pending.get() > 0
    }

    /// Hands a job to the worker, after the time fields' edits that are not written yet (Apply carries them itself).
    fn run(&self, job: Job) {
        if !matches!(job, Job::Apply(_)) {
            self.flush_times();
        }
        self.send(job);
    }

    /// Hands a job to the worker; the controls that act are greyed out until every job is done.
    fn send(&self, job: Job) {
        let sent = self
            .jobs
            .borrow()
            .as_ref()
            .is_some_and(|jobs| jobs.send(job).is_ok());
        if !sent {
            return;
        }
        let first = !self.busy();
        self.pending.set(self.pending.get() + 1);
        if first {
            // SAFETY: reads this thread's focus.
            let focus = unsafe { GetFocus() };
            // nothing has it before the first read: the language then
            self.focus.set(Some(if focus.is_invalid() {
                self.ui.lang
            } else {
                focus
            }));
            for hwnd in self.ui.inputs() {
                controls::enable(hwnd, false);
            }
            controls::set_text(self.ui.status, self.texts().working);
        }
    }

    /// Writes the time fields' edits that are not written yet: before another job, a question or the folder picker, and
    /// before the window closes.
    fn flush_times(&self) {
        if !self.times_dirty.replace(false) {
            return;
        }
        // SAFETY: the timer of this window.
        let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_TIMES) };
        match self.time_wants() {
            Ok(wants) if wants.is_empty() => {}
            Ok(wants) => self.send(Job::Write(wants)),
            Err(line) => {
                controls::set_text(self.ui.status, &line);
                *self.note.borrow_mut() = Some(line);
            }
        }
    }

    fn on_command(&self, id: u16, code: u32) {
        if self.updating.get() {
            return;
        }
        if matches!(id, ids::TIME_LIMIT | ids::ANSWER_WAIT) {
            match code {
                EN_CHANGE => {
                    self.times_dirty.set(true);
                    // SAFETY: a timer of this window.
                    unsafe { SetTimer(Some(self.hwnd), TIMER_TIMES, TIMES_DELAY_MS, None) };
                }
                EN_KILLFOCUS => self.flush_times(),
                _ => {}
            }
            return;
        }
        // A click or a choice (and Enter, Esc) writes what was typed before it acts
        if code == BN_CLICKED || code == CBN_SELCHANGE {
            self.flush_times();
        }
        let checked = |hwnd| Want::On(controls::is_checked(hwnd));
        match (id, code) {
            (ids::LANG, CBN_SELCHANGE) => self.run(Job::Write(vec![(
                Name::Lang,
                settings_ui::lang_want(controls::chosen(self.ui.lang)),
            )])),
            (ids::START, BN_CLICKED) => self.run(Job::Relay(RelayAct::Start)),
            (ids::STOP, BN_CLICKED) => self.run(Job::Relay(RelayAct::Stop)),
            (ids::RESTART, BN_CLICKED) => self.run(Job::Relay(RelayAct::Restart)),
            (ids::AUTOSTART, BN_CLICKED) => self.run(Job::Set(vec![(
                Name::RelayAutostart,
                checked(self.ui.autostart),
            )])),
            (ids::TRAY_ICON, BN_CLICKED) => self.run(Job::Write(vec![(
                Name::RelayTrayIcon,
                checked(self.ui.tray_icon),
            )])),
            (ids::MODE, CBN_SELCHANGE) => self.run(Job::Write(vec![(
                Name::RelayMode,
                settings_ui::mode_want(controls::chosen(self.ui.mode)),
            )])),
            (ids::APPLY, BN_CLICKED) => self.apply(),
            (ids::PLACE, BN_CLICKED) => self.skill(true),
            (ids::REMOVE, BN_CLICKED) => self.skill(false),
            (ids::CHECK, BN_CLICKED) => {
                if let Some(name) = self.selected() {
                    self.run(Job::Check(name));
                }
            }
            (ids::ADD_FOLDER, BN_CLICKED) => self.add_folder(),
            (ids::START_MENU, BN_CLICKED) => self.run(Job::Set(vec![(
                Name::StartMenu,
                checked(self.ui.start_menu),
            )])),
            (ids::OPEN_RECORDS, BN_CLICKED) => {
                // SAFETY: a plain call. The click is this process's input: Explorer may come to the front.
                unsafe {
                    let _ = AllowSetForegroundWindow(ASFW_ANY);
                }
                self.run(Job::OpenRecords);
            }
            (ids::FORGET, BN_CLICKED) => {
                if self.confirm(self.texts().confirm_forget) {
                    self.run(Job::Forget);
                }
            }
            (ids::REFRESH, BN_CLICKED) => self.run(Job::Load),
            (ids::CLOSE, BN_CLICKED) => {
                // SAFETY: closes this window (Esc comes here too).
                let _ = unsafe { DestroyWindow(self.hwnd) };
            }
            _ => {}
        }
    }

    /// The time fields' values that differ from the file, or the line that says a field is out of its bounds.
    fn time_wants(&self) -> Result<Vec<(Name, Want)>, String> {
        let items = self.items.borrow();
        let mut wants = Vec::new();
        for (name, field, now, want) in [
            (
                Name::RelayTimeLimit,
                self.ui.time_limit,
                settings_ui::time_limit_text(&items),
                settings_ui::time_limit_want as fn(&str) -> Option<Want>,
            ),
            (
                Name::RelayAnswerWait,
                self.ui.answer_wait,
                settings_ui::answer_wait_text(&items),
                settings_ui::answer_wait_want,
            ),
        ] {
            let text = controls::text_of(field);
            if text.trim() == now {
                continue;
            }
            match want(&text) {
                Some(w) => wants.push((name, w)),
                None => return Err(settings_ui::out_of_range(self.lang.get(), &name)),
            }
        }
        Ok(wants)
    }

    /// The time fields' last change was a moment ago (the up-down's arrows): writes them, unless the user is still typing
    /// in one; that one is written when it loses the focus, at Enter, or before the next action.
    fn on_times(&self) {
        // SAFETY: the timer of this window; reads this thread's focus.
        let focus = unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_TIMES);
            GetFocus()
        };
        if focus == self.ui.time_limit || focus == self.ui.answer_wait {
            return;
        }
        self.flush_times();
    }

    /// Apply: the fields as they are, then op-reason's relay starts again, after the question. Without the answer OK the
    /// fields are still written (the relay uses them when it starts again).
    fn apply(&self) {
        // SAFETY: the timer of this window: Apply writes the fields itself.
        let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_TIMES) };
        let wants = match self.time_wants() {
            Ok(w) => w,
            Err(line) => {
                controls::set_text(self.ui.status, &line);
                return;
            }
        };
        self.times_dirty.set(false);
        if self.confirm(self.texts().confirm_apply) {
            self.send(Job::Apply(wants));
        } else if !wants.is_empty() {
            self.send(Job::Write(wants));
        }
    }

    fn confirm(&self, question: &str) -> bool {
        let text = HSTRING::from(question);
        let title = HSTRING::from(self.texts().title);
        // SAFETY: a message box owned by this window; the strings outlive the call.
        unsafe {
            MessageBoxW(
                Some(self.hwnd),
                &text,
                &title,
                MB_OKCANCEL | MB_ICONQUESTION,
            ) == IDOK
        }
    }

    fn selected(&self) -> Option<Name> {
        let index = list::selected(self.ui.skills)?;
        self.rows.borrow().get(index).map(|r| r.name.clone())
    }

    fn skill(&self, on: bool) {
        if let Some(name) = self.selected() {
            self.run(Job::Set(vec![(name, Want::On(on))]));
        }
    }

    /// Add a folder…: the folder picker, then `skill.folder.<folder> on`.
    fn add_folder(&self) {
        let Some(folder) = pick_folder(self.hwnd) else {
            return;
        };
        match Name::parse(&format!("skill.folder.{folder}")) {
            Ok(name) => self.run(Job::Set(vec![(name, Want::On(true))])),
            Err(usage) => {
                controls::set_text(self.ui.status, &text::usage_error(self.lang.get(), &usage))
            }
        }
    }

    /// A job is done: when it was the last one handed over, the window shows what was read and what the jobs did.
    fn on_done(&self) {
        while let Ok(outcome) = self.done.try_recv() {
            self.pending.set(self.pending.get().saturating_sub(1));
            self.batch
                .borrow_mut()
                .extend(outcome.changes.iter().cloned());
            if outcome.checked.is_some() {
                *self.batch_checked.borrow_mut() = outcome.checked.clone();
            }
            if !self.busy() {
                let changes = self.batch.take();
                let checked = self.batch_checked.take();
                self.show(Outcome {
                    changes,
                    checked,
                    ..outcome
                });
            }
        }
    }

    /// Shows what was read, and what the job did.
    fn show(&self, outcome: Outcome) {
        let Outcome {
            changes,
            checked,
            items,
            records,
            accounts,
        } = outcome;
        // The window speaks the language of `lang` as it is now
        let chosen = items
            .iter()
            .find(|i| i.name == Name::Lang)
            .and_then(|i| i.value.as_deref())
            .and_then(Lang::parse);
        let lang = lang::pick(chosen);
        if lang != self.lang.get() {
            self.lang.set(lang);
            let t = settings_ui::texts(lang);
            self.ui.set_texts(&t);
            // SAFETY: the title of this window; the string outlives the call.
            let _ = unsafe { SetWindowTextW(self.hwnd, &HSTRING::from(t.title)) };
            self.arrange();
        }
        let version = app::VERSION;
        let rows = settings_ui::skill_rows(lang, &items, version);
        let before = self.selected();
        self.updating.set(true);
        controls::choose(self.ui.lang, settings_ui::lang_index(&items));
        controls::set_text(self.ui.relay_line, &settings_ui::relay_line(lang, &items));
        controls::set_checked(
            self.ui.autostart,
            settings_ui::is_on(&items, &Name::RelayAutostart),
        );
        controls::set_checked(
            self.ui.tray_icon,
            settings_ui::is_on(&items, &Name::RelayTrayIcon),
        );
        controls::choose(self.ui.mode, settings_ui::mode_index(&items));
        controls::set_text(self.ui.time_limit, &settings_ui::time_limit_text(&items));
        controls::set_text(self.ui.answer_wait, &settings_ui::answer_wait_text(&items));
        controls::set_text(self.ui.apply_note, settings_ui::apply_note(lang, &items));
        list::clear(self.ui.skills);
        for (index, row) in rows.iter().enumerate() {
            list::insert(self.ui.skills, index, [&row.place, &row.state, &row.where_]);
        }
        let keep = before
            .and_then(|name| rows.iter().position(|r| r.name == name))
            .unwrap_or(0);
        if !rows.is_empty() {
            list::select(self.ui.skills, keep);
        }
        controls::set_checked(
            self.ui.start_menu,
            settings_ui::is_on(&items, &Name::StartMenu),
        );
        controls::set_text(self.ui.records, &records);
        controls::set_text(self.ui.accounts, &settings_ui::accounts(lang, accounts));
        let status = match &checked {
            Some(item) => format!(
                "{}: {}",
                settings_ui::place_label(lang, &item.name),
                settings_ui::skill_state(lang, item, version)
            ),
            None => settings_ui::result_line(lang, &changes),
        };
        let status = match self.note.take() {
            Some(note) if status.is_empty() => note,
            Some(note) => format!("{note} / {status}"),
            None => status,
        };
        controls::set_text(self.ui.status, &status);
        self.updating.set(false);
        *self.rows.borrow_mut() = rows;
        self.accounts.set(accounts);
        *self.items.borrow_mut() = items;
        self.enable_all();
    }

    /// The controls that act, as the settings now allow.
    fn enable_all(&self) {
        for hwnd in self.ui.inputs() {
            controls::enable(hwnd, true);
        }
        let (start, stop, restart) = settings_ui::relay_buttons(&self.items.borrow());
        controls::enable(self.ui.start, start);
        controls::enable(self.ui.stop, stop);
        controls::enable(self.ui.restart, restart);
        controls::enable(self.ui.apply, restart);
        controls::enable(self.ui.forget, self.accounts.get() > 0);
        self.enable_skill_buttons();
        if let Some(focus) = self.focus.take() {
            // SAFETY: the focus goes back where it was, or to Refresh when that control may not act now.
            unsafe {
                let to = if IsWindowEnabled(focus).as_bool() {
                    focus
                } else {
                    self.ui.refresh
                };
                let _ = SetFocus(Some(to));
            }
        }
    }

    fn enable_skill_buttons(&self) {
        if self.busy() {
            return;
        }
        let items = self.items.borrow();
        let item = self
            .selected()
            .and_then(|name| items.iter().find(|i| i.name == name));
        let (place, remove, check) = settings_ui::skill_buttons(item);
        controls::enable(self.ui.place, place);
        controls::enable(self.ui.remove, remove);
        controls::enable(self.ui.check, check);
    }
}

/// The folder picker (`IFileOpenDialog` for folders); the chosen folder, or nothing when it was cancelled.
fn pick_folder(owner: HWND) -> Option<String> {
    // SAFETY: COM calls on this thread (initialized apartment-threaded by `open`); the path the dialog returns is freed
    // here.
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .ok()?;
        dialog.Show(Some(owner)).ok()?;
        let item = dialog.GetResult().ok()?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const c_void));
        text
    }
}

/// The work area of the screen where the pointer is (the user just clicked the Start menu, the icon or a terminal there).
fn work_area() -> RECT {
    let mut point = POINT::default();
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: the structures outlive the calls.
    unsafe {
        let _ = GetCursorPos(&mut point);
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTOPRIMARY);
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            return info.rcWork;
        }
    }
    RECT {
        left: 0,
        top: 0,
        right: 1024,
        bottom: 768,
    }
}

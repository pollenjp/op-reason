//! Ctrl+C, Ctrl+Break and closing the console. op.exe shares the console and gets the same event; the exe only
//! notes it, so that it can still sign out and write the record.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    CONSOLE_MODE, CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT,
    CTRL_SHUTDOWN_EVENT, GetConsoleMode, GetConsoleProcessList, GetStdHandle, STD_ERROR_HANDLE,
    STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleCtrlHandler,
};
use windows::core::BOOL;

const NONE: u8 = 0;
const INTERRUPT: u8 = 1;
const CLOSE: u8 = 2;

static STATE: AtomicU8 = AtomicU8::new(NONE);

/// Run once on console close (the system ends the process a few seconds after the handler returns).
type CloseHook = Box<dyn FnOnce() + Send>;
/// The handler runs the hook while holding this lock, and the main flow writes its own `end` while holding it with the
/// hook removed ([`without_close_hook`]): exactly one of the two writes the record, and the process cannot end in the
/// middle of the main flow's write.
static CLOSE_HOOK: Mutex<Option<CloseHook>> = Mutex::new(None);

unsafe extern "system" fn handler(ctrl: u32) -> BOOL {
    match ctrl {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => {
            // The first one is noted (the call signs out and records it); a second one is left to the default
            // handler, which ends the process, so that a stuck call can always be stopped.
            let first = STATE
                .compare_exchange(NONE, INTERRUPT, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok();
            BOOL::from(first)
        }
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
            STATE.store(CLOSE, Ordering::SeqCst);
            let mut guard = CLOSE_HOOK.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(hook) = guard.take() {
                hook();
            }
            drop(guard);
            BOOL::from(false)
        }
        _ => BOOL::from(false),
    }
}

/// Installs the handler. Failing to install is not fatal (the default handler then ends the process).
pub fn install() {
    // SAFETY: `handler` is a valid `extern "system"` function for the whole process lifetime.
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(handler), true);
    }
}

/// Sets what to do when the console closes (write the `end` record).
pub fn on_close(hook: CloseHook) {
    *CLOSE_HOOK.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
}

/// Removes the close hook and runs `f` while the console handler cannot run it (the main flow writes `end` itself).
pub fn without_close_hook<R>(f: impl FnOnce() -> R) -> R {
    let mut guard = CLOSE_HOOK.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;
    let r = f();
    drop(guard);
    r
}

/// Ctrl+C or Ctrl+Break was pressed.
pub fn interrupted() -> bool {
    STATE.load(Ordering::SeqCst) != NONE
}

/// Our stderr is a console (not a pipe or a file): only then is the "waiting" note printed.
pub fn stderr_is_console() -> bool {
    is_console(STD_ERROR_HANDLE)
}

/// Our stdin is something to read: not a console (where reading would wait for typing) and not missing. An
/// `op inject` template is read from it only then (design `2026-11-17-refs-in-window-design.md` §2).
pub fn stdin_is_readable() -> bool {
    has_handle(STD_INPUT_HANDLE) && !is_console(STD_INPUT_HANDLE)
}

/// How many processes share this process's console; 0 when it has none.
fn processes() -> u32 {
    let mut list = [0u32; 4];
    // SAFETY: a valid buffer.
    unsafe { GetConsoleProcessList(&mut list) }
}

/// This process has no console: none was inherited and Windows made none (the manifest's console allocation policy, on
/// Windows 11 24H2 and later: a start without flags from a scheduled task or a program without a console, Explorer's
/// `CREATE_NEW_CONSOLE`), it was started with `DETACHED_PROCESS`, or it let go of one. A console program that it starts
/// would get a console window of its own (design `2026-11-24-relay-no-console-window-design.md` §5.2).
pub fn none() -> bool {
    processes() == 0
}

/// Nobody reads this process's output, as after a double click in Explorer: a console that Windows made for this
/// process alone with the output on it (Windows before 11 24H2), or no console and no standard output at all (the
/// manifest's console allocation policy). `op-reason.exe` without arguments then opens the settings window instead of
/// printing its usage into a console that closes at once, or into nothing (designs `2026-11-23-settings-design.md`
/// §11.1 and `2026-11-24-relay-no-console-window-design.md` §5.3).
pub fn started_alone() -> bool {
    match processes() {
        0 => !has_handle(STD_OUTPUT_HANDLE),
        1 => is_console(STD_OUTPUT_HANDLE),
        _ => false,
    }
}

/// The standard handle is there (not missing, not invalid).
fn has_handle(which: STD_HANDLE) -> bool {
    // SAFETY: GetStdHandle only reads process state.
    unsafe { GetStdHandle(which) }.is_ok_and(|h| h != HANDLE::default() && !h.is_invalid())
}

fn is_console(which: STD_HANDLE) -> bool {
    // SAFETY: GetStdHandle and GetConsoleMode only read process state.
    unsafe {
        let Ok(h) = GetStdHandle(which) else {
            return false;
        };
        if h == HANDLE::default() || h.is_invalid() {
            return false;
        }
        let mut mode = CONSOLE_MODE::default();
        GetConsoleMode(h, &mut mode).is_ok()
    }
}

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
    // SAFETY: GetStdHandle only reads process state.
    let present = unsafe { GetStdHandle(STD_INPUT_HANDLE) }
        .is_ok_and(|h| h != HANDLE::default() && !h.is_invalid());
    present && !is_console(STD_INPUT_HANDLE)
}

/// This process has a console of its own that shows its output: Windows made the console for it (a double click in
/// Explorer), not a terminal that runs other programs too. `op-reason.exe` without arguments then opens the settings
/// window instead of printing its usage into a console that closes at once (design `2026-11-23-settings-design.md`
/// §11.1).
pub fn of_its_own() -> bool {
    let mut list = [0u32; 4];
    // SAFETY: a valid buffer.
    let n = unsafe { GetConsoleProcessList(&mut list) };
    n == 1 && is_console(STD_OUTPUT_HANDLE)
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

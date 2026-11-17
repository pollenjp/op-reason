//! Opening an item in the 1Password app from a link in the window (design `2026-11-17-refs-in-window-design.md` §5.4).

use std::thread::{self, JoinHandle};

use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow, SW_SHOWNORMAL};
use windows::core::{HSTRING, w};

/// The only URLs the window opens (built by `crate::account::item_url`).
pub const ITEM_URL_PREFIX: &str = "onepassword://view-item?";

/// Opens `url` with the handler Windows has for it, on a thread of its own so that the window never waits for the
/// shell. Called while the window handles the click: this process received the last input, so it may let the app
/// that the shell starts or wakes come to the front. Anything but an item URL is refused.
pub fn open_item(url: &str) -> Option<JoinHandle<()>> {
    if !url.starts_with(ITEM_URL_PREFIX) || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    // SAFETY: a plain Win32 call without pointers.
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
    let url = HSTRING::from(url);
    Some(thread::spawn(move || {
        // SAFETY: COM is initialized single-threaded for this thread only, as Microsoft documents for ShellExecute, and
        // uninitialized after; the strings outlive the call.
        unsafe {
            let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let _ = ShellExecuteW(None, w!("open"), &url, None, None, SW_SHOWNORMAL);
            if com.is_ok() {
                CoUninitialize();
            }
        }
    }))
}

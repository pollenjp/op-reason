//! Windows' app mode, and the frame of a window in the look (design `2026-11-26-modern-window-design.md` §4.1, §4.14):
//! the caption keeps its close button and the window's text, but draws neither its text nor its icon, takes the
//! window's background color (Windows 11) and is dark in the dark look; the corners are round. Every request may fail
//! on an older Windows, which then shows its usual caption.

use std::ffi::c_void;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::Controls::{
    SetWindowThemeAttribute, WTA_NONCLIENT, WTA_OPTIONS, WTNCA_NODRAWCAPTION, WTNCA_NODRAWICON,
};
use windows::core::{BOOL, PCWSTR, w};

use crate::theme::{self, Rgb, Theme};

/// Windows' app mode now (`AppsUseLightTheme`; light when it cannot be read).
pub fn current() -> Theme {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: a DWORD out parameter with its size; the names are NUL-terminated constants.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    Theme::from_apps_use_light_theme(read.is_ok().then_some(value))
}

/// A color as Windows' COLORREF (`0x00BBGGRR`).
pub fn colorref(c: Rgb) -> COLORREF {
    COLORREF(u32::from(c.0) | (u32::from(c.1) << 8) | (u32::from(c.2) << 16))
}

/// Puts the frame of `hwnd` in the look of `theme`. Failures are ignored.
pub fn frame(hwnd: HWND, theme: Theme) {
    let dark = BOOL::from(theme == Theme::Dark);
    let caption = colorref(theme::palette(theme).background);
    let corners = DWMWCP_ROUND;
    let options = WTA_OPTIONS {
        dwFlags: WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON,
        dwMask: WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON,
    };
    // SAFETY: each value lives across its call, with its size.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const BOOL as *const c_void,
            size_of::<BOOL>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption as *const COLORREF as *const c_void,
            size_of::<COLORREF>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corners as *const _ as *const c_void,
            size_of_val(&corners) as u32,
        );
        let _ = SetWindowThemeAttribute(
            hwnd,
            WTA_NONCLIENT,
            &options as *const WTA_OPTIONS as *const c_void,
            size_of::<WTA_OPTIONS>() as u32,
        );
    }
}

/// Whether a `WM_SETTINGCHANGE` says that the app mode may have changed: its `lParam` names `ImmersiveColorSet`.
pub fn changed(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    let name: Vec<u16> = "ImmersiveColorSet".encode_utf16().collect();
    // SAFETY: Windows passes a NUL-terminated string in lParam of this message; it is read up to the NUL, never past
    // the length of the name and its NUL.
    unsafe {
        let s = PCWSTR(lparam.0 as *const u16);
        for (i, want) in name.iter().enumerate() {
            if *s.0.add(i) != *want {
                return false;
            }
        }
        *s.0.add(name.len()) == 0
    }
}

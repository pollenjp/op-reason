//! The screen that op-reason's windows and toasts open on (design `2026-11-18-pass-toasts-design.md` §5): the screen of
//! the foreground window when they are made, so that they come where the person works; the primary screen when there is
//! no foreground window or it cannot be read. A window never moves after it opened.

use std::ffi::c_void;

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
    MonitorFromRect, MonitorFromWindow,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SystemParametersInfoW,
};

/// A screen: its work area (pixels, without the taskbar) and its DPI.
#[derive(Debug, Clone, Copy)]
pub struct Screen {
    pub work: RECT,
    pub dpi: u32,
}

/// The screen of the foreground window, else the primary screen.
pub fn of_foreground() -> Screen {
    // SAFETY: plain queries; the window handle may be stale, which MonitorFromWindow answers with the primary monitor.
    let foreground = unsafe {
        let fg = GetForegroundWindow();
        (!fg.is_invalid()).then(|| MonitorFromWindow(fg, MONITOR_DEFAULTTOPRIMARY))
    };
    if let Some(screen) = foreground.and_then(of_monitor) {
        return screen;
    }
    // SAFETY: a plain query.
    let primary = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    of_monitor(primary).unwrap_or_else(|| Screen {
        work: primary_work_area(),
        dpi: dpi_of(primary),
    })
}

/// A monitor's work area and DPI, when it can be read.
fn of_monitor(monitor: HMONITOR) -> Option<Screen> {
    if monitor.is_invalid() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: a valid out parameter with its size set.
    let read = unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() };
    read.then(|| Screen {
        work: info.rcWork,
        dpi: dpi_of(monitor),
    })
}

/// A monitor's effective DPI; 96 when it cannot be read.
fn dpi_of(monitor: HMONITOR) -> u32 {
    let (mut x, mut y) = (96u32, 96u32);
    // SAFETY: valid out parameters.
    let ok = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok() };
    if ok && x > 0 { x } else { 96 }
}

/// The primary screen's work area, as op-reason's windows took it before (`SPI_GETWORKAREA`).
fn primary_work_area() -> RECT {
    let mut area = RECT::default();
    // SAFETY: the out parameter is a RECT, as SPI_GETWORKAREA writes.
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut area as *mut RECT as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    area
}

/// The whole screen (pixels, with the taskbar) that holds most of `r`, and its DPI; the primary screen when none does.
pub fn bounds_of(r: RECT) -> Option<(RECT, u32)> {
    // SAFETY: a plain query with a rectangle on the stack.
    let monitor = unsafe { MonitorFromRect(&r, MONITOR_DEFAULTTOPRIMARY) };
    if monitor.is_invalid() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: a valid out parameter with its size set.
    let read = unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() };
    read.then(|| (info.rcMonitor, dpi_of(monitor)))
}

/// The primary screen's work area and DPI (where the pointer to the tray goes when there is no taskbar to find).
pub fn primary() -> Screen {
    // SAFETY: a plain query.
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    of_monitor(monitor).unwrap_or_else(|| Screen {
        work: primary_work_area(),
        dpi: dpi_of(monitor),
    })
}

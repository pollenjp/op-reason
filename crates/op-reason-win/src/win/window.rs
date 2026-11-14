//! The approval window, drawn with Direct2D and DirectWrite. Same look and behavior as the A' launcher's WPF window:
//! top right of the primary work area, always on top, never takes the focus, not in the taskbar, light yellow,
//! the title in bold, one "label: value" line per fact, an orange line while other op.exe run, and the footer in
//! red. It asks nothing; the decision is made in the 1Password prompt.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows::Win32::Foundation::{
    HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_SIZE_U, D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_DRAW_TEXT_OPTIONS_NONE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1CreateFactory,
    ID2D1Factory, ID2D1HwndRenderTarget,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_METRICS,
    DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory,
    IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
    MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, HWND_TOPMOST, IDC_ARROW,
    LoadCursorW, MA_NOACTIVATE, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW, SPI_GETWORKAREA,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    SystemParametersInfoW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SIZE,
    WNDCLASSEXW, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WS_SYSMENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EndMenu, SendMessageW, SetTimer, WM_CANCELMODE, WM_TIMER,
};
use windows::core::{HSTRING, PCWSTR, Result, w};
use windows_numerics::Vector2;

use crate::ui::Texts;

// Layout in DIPs, as the launcher's WPF window.
const OUTER_WIDTH: f32 = 520.0;
const PAD_LEFT: f32 = 14.0;
const PAD_TOP: f32 = 10.0;
const PAD_RIGHT: f32 = 14.0;
const PAD_BOTTOM: f32 = 12.0;
const LINE_GAP: f32 = 2.0;
const SCREEN_MARGIN: f32 = 16.0;
const TITLE_SIZE: f32 = 16.0;
const BODY_SIZE: f32 = 12.0;

const CLASS_NAME: PCWSTR = w!("op-reason-approval");
/// The timer of [`Window::watch`].
const WATCH_TIMER: usize = 1;
const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0);
const EX_STYLE: WINDOW_EX_STYLE =
    WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0 | WS_EX_NOACTIVATE.0);

const fn rgb(r: u8, g: u8, b: u8) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}
const LIGHT_YELLOW: D2D1_COLOR_F = rgb(0xFF, 0xFF, 0xE0);
const BLACK: D2D1_COLOR_F = rgb(0, 0, 0);
const DARK_ORANGE: D2D1_COLOR_F = rgb(0xFF, 0x8C, 0x00);
const FIREBRICK: D2D1_COLOR_F = rgb(0xB2, 0x22, 0x22);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Title,
    Line,
    Others,
    Footer,
}

struct Para {
    layout: IDWriteTextLayout,
    color: D2D1_COLOR_F,
    height: f32,
    role: Role,
}

struct State {
    d2d: ID2D1Factory,
    target: Option<ID2D1HwndRenderTarget>,
    paras: Vec<Para>,
    show_others: bool,
    dpi: u32,
    destroyed: bool,
    /// Draw emoji in color (turned off if drawing fails with it).
    color_fonts: bool,
    /// A frame was drawn and presented (EndDraw succeeded at least once).
    painted: bool,
    /// The op.exe whose end must break a modal loop (moving the window, its system menu).
    watched: Option<HANDLE>,
}

impl State {
    fn client_height_dip(&self) -> f32 {
        PAD_TOP
            + self
                .paras
                .iter()
                .filter(|p| self.show_others || p.role != Role::Others)
                .map(|p| p.height + 2.0 * LINE_GAP)
                .sum::<f32>()
            + PAD_BOTTOM
    }

    fn render(&mut self, hwnd: HWND) -> Result<()> {
        if self.target.is_none() {
            let mut rc = RECT::default();
            // SAFETY: valid window handle and out parameter.
            unsafe { GetClientRect(hwnd, &mut rc)? };
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_UNKNOWN,
                    alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                },
                dpiX: self.dpi as f32,
                dpiY: self.dpi as f32,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hprops = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U {
                    width: (rc.right - rc.left).max(1) as u32,
                    height: (rc.bottom - rc.top).max(1) as u32,
                },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            // SAFETY: properties live across the call.
            self.target = Some(unsafe { self.d2d.CreateHwndRenderTarget(&props, &hprops)? });
        }
        let Some(t) = self.target.as_ref() else {
            return Ok(());
        };
        // Brushes first, so that nothing can return between BeginDraw and EndDraw.
        let mut brushes = Vec::with_capacity(self.paras.len());
        for p in &self.paras {
            // SAFETY: the color lives across the call.
            brushes.push(unsafe { t.CreateSolidColorBrush(&p.color, None)? });
        }
        let options = if self.color_fonts {
            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT
        } else {
            D2D1_DRAW_TEXT_OPTIONS_NONE
        };
        // SAFETY: Direct2D drawing between BeginDraw and EndDraw on the target we own.
        let r = unsafe {
            t.BeginDraw();
            t.Clear(Some(&LIGHT_YELLOW));
            let mut y = PAD_TOP;
            for (p, brush) in self.paras.iter().zip(&brushes) {
                if p.role == Role::Others && !self.show_others {
                    continue;
                }
                y += LINE_GAP;
                t.DrawTextLayout(Vector2 { X: PAD_LEFT, Y: y }, &p.layout, brush, options);
                y += p.height + LINE_GAP;
            }
            t.EndDraw(None, None)
        };
        if r.is_ok() {
            self.painted = true;
        } else {
            // D2DERR_RECREATE_TARGET and friends: rebuild the target. The first failure also turns color fonts off
            // and paints again; after that, wait for the next paint request rather than loop.
            self.target = None;
            if self.color_fonts {
                self.color_fonts = false;
                // SAFETY: valid window handle.
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
        }
        Ok(())
    }
}

/// Makes the process per-monitor DPI aware (V2). The manifest says the same; this covers a build without it.
pub fn set_dpi_awareness() {
    // SAFETY: process-wide setting; failure (already set by the manifest) is fine.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn register_class() -> Result<()> {
    static DONE: OnceLock<bool> = OnceLock::new();
    if DONE.get().is_some() {
        return Ok(());
    }
    // SAFETY: registering a window class with a static name and a valid window procedure.
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return Err(windows::core::Error::from_thread());
        }
    }
    let _ = DONE.set(true);
    Ok(())
}

fn locale() -> Vec<u16> {
    let mut buf = [0u16; 85];
    // SAFETY: the buffer is LOCALE_NAME_MAX_LENGTH long.
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n <= 0 {
        return "en-US\0".encode_utf16().collect();
    }
    buf[..n as usize].to_vec()
}

fn format(
    dw: &IDWriteFactory,
    weight: DWRITE_FONT_WEIGHT,
    size: f32,
    locale: &[u16],
) -> Result<IDWriteTextFormat> {
    // SAFETY: the strings are NUL-terminated and outlive the call.
    unsafe {
        let f = dw.CreateTextFormat(
            w!("Segoe UI"),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            PCWSTR(locale.as_ptr()),
        )?;
        f.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
        Ok(f)
    }
}

fn para(
    dw: &IDWriteFactory,
    fmt: &IDWriteTextFormat,
    text: &str,
    bold_prefix: usize,
    width: f32,
    color: D2D1_COLOR_F,
    role: Role,
) -> Result<Para> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: DirectWrite copies the text; the out parameter lives on the stack.
    unsafe {
        let layout = dw.CreateTextLayout(&wide, fmt, width, f32::MAX)?;
        if bold_prefix > 0 {
            layout.SetFontWeight(
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_TEXT_RANGE {
                    startPosition: 0,
                    length: bold_prefix as u32,
                },
            )?;
        }
        let mut m = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut m)?;
        Ok(Para {
            layout,
            color,
            height: m.height,
            role,
        })
    }
}

fn scale(dip: f32, dpi: u32) -> i32 {
    (dip * dpi as f32 / 96.0).round() as i32
}

/// The outer size in pixels for a client height in DIPs.
fn outer_size(client_h_dip: f32, dpi: u32) -> Result<(i32, i32)> {
    let mut frame = RECT::default();
    // SAFETY: valid out parameter.
    unsafe { AdjustWindowRectExForDpi(&mut frame, STYLE, false, EX_STYLE, dpi)? };
    let frame_h = frame.bottom - frame.top;
    Ok((scale(OUTER_WIDTH, dpi), scale(client_h_dip, dpi) + frame_h))
}

/// The approval window. Lives on the thread that created it.
pub struct Window {
    hwnd: HWND,
    state: *mut State,
}

impl Window {
    /// Creates the window (hidden). `others` shows the orange line from the start.
    pub fn create(text: &Texts, others: bool) -> Result<Window> {
        register_class()?;
        // SAFETY: factory creation and layout calls with valid arguments; the state pointer is handed to the
        // window and freed in Drop after the window is destroyed.
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let loc = locale();
            let title_fmt = format(&dw, DWRITE_FONT_WEIGHT_BOLD, TITLE_SIZE, &loc)?;
            let body_fmt = format(&dw, DWRITE_FONT_WEIGHT_NORMAL, BODY_SIZE, &loc)?;
            let bold_fmt = format(&dw, DWRITE_FONT_WEIGHT_BOLD, BODY_SIZE, &loc)?;

            let hmon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            let (mut dx, mut dy) = (96u32, 96u32);
            if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_err() {
                dx = 96;
            }
            let dpi = dx;

            // The text width: the outer width minus the side frames and the padding.
            let mut frame = RECT::default();
            AdjustWindowRectExForDpi(&mut frame, STYLE, false, EX_STYLE, dpi)?;
            let frame_w_dip = (frame.right - frame.left) as f32 * 96.0 / dpi as f32;
            let width = OUTER_WIDTH - frame_w_dip - PAD_LEFT - PAD_RIGHT;

            let mut paras = vec![para(
                &dw,
                &title_fmt,
                &text.title,
                0,
                width,
                BLACK,
                Role::Title,
            )?];
            for (label, value) in &text.lines {
                let head = format!("{label}: ");
                let line = format!("{head}{value}");
                paras.push(para(
                    &dw,
                    &body_fmt,
                    &line,
                    head.encode_utf16().count(),
                    width,
                    BLACK,
                    Role::Line,
                )?);
            }
            paras.push(para(
                &dw,
                &bold_fmt,
                &text.others,
                0,
                width,
                DARK_ORANGE,
                Role::Others,
            )?);
            paras.push(para(
                &dw,
                &bold_fmt,
                &text.footer,
                0,
                width,
                FIREBRICK,
                Role::Footer,
            )?);

            let hinstance = GetModuleHandleW(None)?;
            let state = Box::into_raw(Box::new(State {
                d2d,
                target: None,
                paras,
                show_others: others,
                dpi,
                destroyed: false,
                color_fonts: true,
                painted: false,
                watched: None,
            }));
            let (w, h) = match outer_size((*state).client_height_dip(), dpi) {
                Ok(s) => s,
                Err(e) => {
                    drop(Box::from_raw(state));
                    return Err(e);
                }
            };
            let mut area = RECT::default();
            let _ = SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut area as *mut RECT as *mut c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
            let margin = scale(SCREEN_MARGIN, dpi);
            let x = area.right - w - margin;
            let y = area.top + margin;
            let hwnd = match CreateWindowExW(
                EX_STYLE,
                CLASS_NAME,
                &HSTRING::from(text.title.as_str()),
                STYLE,
                x,
                y,
                w,
                h,
                None,
                None,
                Some(hinstance.into()),
                Some(state as *const c_void),
            ) {
                Ok(h) => h,
                Err(e) => {
                    drop(Box::from_raw(state));
                    return Err(e);
                }
            };
            Ok(Window { hwnd, state })
        }
    }

    /// Shows the window without activating it, then puts it on top (topmost has to be applied after showing).
    pub fn show(&self) {
        // SAFETY: valid window handle on its own thread.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    /// A frame was drawn (what the record calls `shown`).
    pub fn painted(&self) -> bool {
        // SAFETY: `state` stays valid until Drop.
        unsafe { (*self.state).painted }
    }

    /// Watches op.exe from a timer: moving the window or opening its system menu runs a modal loop inside
    /// `DispatchMessageW`, where the caller's wait cannot see op.exe end or Ctrl+C. The timer still runs there and
    /// breaks the loop when either happens. `process` must stay valid while the window lives.
    pub fn watch(&self, process: HANDLE) {
        // SAFETY: `state` stays valid until Drop; the reference ends before SetTimer.
        unsafe {
            (*self.state).watched = Some(process);
            let _ = SetTimer(Some(self.hwnd), WATCH_TIMER, 250, None);
        }
    }

    /// The user closed the window (the call goes on).
    pub fn is_open(&self) -> bool {
        // SAFETY: `state` stays valid until Drop.
        unsafe { !(*self.state).destroyed }
    }

    /// Shows or hides the "other op.exe" line and fits the height.
    pub fn set_others(&self, show: bool) {
        if !self.is_open() {
            return;
        }
        // Read what we need, then call Win32 without holding a reference into the state: SetWindowPos runs the
        // window procedure, which takes its own reference.
        // SAFETY: `state` stays valid until Drop; the reference ends before the calls below.
        let (changed, client_h, dpi) = unsafe {
            let st = &mut *self.state;
            let changed = st.show_others != show;
            st.show_others = show;
            (changed, st.client_height_dip(), st.dpi)
        };
        if !changed {
            return;
        }
        if let Ok((w, h)) = outer_size(client_h, dpi) {
            // SAFETY: valid window handle on its own thread.
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    0,
                    0,
                    w,
                    h,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: destroy the window first (its procedure reads the state), then free the state.
        unsafe {
            if !(*self.state).destroyed {
                let _ = DestroyWindow(self.hwnd);
            }
            drop(Box::from_raw(self.state));
        }
    }
}

/// Handles every queued message of this thread.
pub fn pump() {
    let mut msg = MSG::default();
    // SAFETY: standard message loop on the calling thread.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: GWLP_USERDATA holds the State pointer from WM_NCCREATE until WM_NCDESTROY; each handler takes a
    // short-lived reference that does not span calls which may re-enter this procedure.
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                let _ = (*ptr).render(hwnd);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_SIZE => {
                if let Some(t) = (*ptr).target.as_ref() {
                    let mut rc = RECT::default();
                    if GetClientRect(hwnd, &mut rc).is_ok() {
                        let _ = t.Resize(&D2D_SIZE_U {
                            width: (rc.right - rc.left).max(1) as u32,
                            height: (rc.bottom - rc.top).max(1) as u32,
                        });
                    }
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let dpi = u32::from((wparam.0 & 0xFFFF) as u16);
                {
                    let st = &mut *ptr;
                    st.dpi = dpi;
                    if let Some(t) = st.target.as_ref() {
                        t.SetDpi(dpi as f32, dpi as f32);
                    }
                }
                let r = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_TIMER if wparam.0 == WATCH_TIMER => {
                let ended = (*ptr)
                    .watched
                    .is_some_and(|h| WaitForSingleObject(h, 0) == WAIT_OBJECT_0);
                if ended || super::console::interrupted() {
                    // end a menu loop and a move/size loop, so that the caller's wait runs again
                    let _ = EndMenu();
                    let _ = SendMessageW(hwnd, WM_CANCELMODE, None, None);
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                (*ptr).destroyed = true;
                (*ptr).target = None;
                LRESULT(0)
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

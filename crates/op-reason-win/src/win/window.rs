//! The approval window, drawn with Direct2D and DirectWrite. Same look and behavior as the A' launcher's WPF window:
//! top right of the primary work area, always on top, never takes the focus, not in the taskbar, light yellow,
//! the title in bold, one "label: value" line per fact, an orange line while other op.exe run, and the footer in
//! red. It asks nothing; the decision is made in the 1Password prompt.
//!
//! Its one control is the language switch `English | 日本語` at the right of the footer (design
//! `2026-11-15-language-toggle-design.md` §4): a click on the other language lays the window out again in that
//! language and hands the choice to the caller, without activating the window.

use std::ffi::c_void;
use std::sync::OnceLock;

use op_reason_core::lang::Lang;
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
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_LINE_METRICS,
    DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory,
    IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint, PAINTSTRUCT,
    ScreenToClient,
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
    EndMenu, GetCursorPos, HTCLIENT, IDC_HAND, SendMessageW, SetCursor, SetTimer, SetWindowTextW,
    WM_CANCELMODE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_SETCURSOR, WM_TIMER,
};
use windows::core::{HSTRING, PCWSTR, Result, w};
use windows_numerics::Vector2;

use crate::ui::{self, Both, Texts};

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
// The language switch at the right of the footer: its size, the gap before it, and the room around the clickable word.
const SWITCH_SIZE: f32 = 11.0;
const SWITCH_GAP: f32 = 12.0;
const SWITCH_HIT_X: f32 = 4.0;
const SWITCH_HIT_Y: f32 = 3.0;
/// The width of a switch part's layout: wide enough never to wrap.
const NO_WRAP: f32 = 10_000.0;

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
const LINK_BLUE: D2D1_COLOR_F = rgb(0x00, 0x66, 0xCC);
const GRAY: D2D1_COLOR_F = rgb(0x80, 0x80, 0x80);

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
    /// The baseline of its last line, from its top (the switch sits on the footer's).
    last_baseline: f32,
}

/// One part of the language switch: a language's name or the bar.
struct Piece {
    layout: IDWriteTextLayout,
    color: D2D1_COLOR_F,
    part: ui::SwitchPart,
}

/// The text formats, kept to lay the window out again in the other language.
struct Fonts {
    dw: IDWriteFactory,
    title: IDWriteTextFormat,
    body: IDWriteTextFormat,
    bold: IDWriteTextFormat,
    switch: IDWriteTextFormat,
    switch_bold: IDWriteTextFormat,
}

struct State {
    d2d: ID2D1Factory,
    target: Option<ID2D1HwndRenderTarget>,
    fonts: Fonts,
    /// The width of the text area (DIPs).
    width: f32,
    texts: Both,
    /// The language shown.
    lang: Lang,
    paras: Vec<Para>,
    switch: Vec<Piece>,
    show_others: bool,
    dpi: u32,
    destroyed: bool,
    /// Draw emoji in color (turned off if drawing fails with it).
    color_fonts: bool,
    /// A frame was drawn and presented (EndDraw succeeded at least once).
    painted: bool,
    /// The op.exe whose end must break a modal loop (moving the window, its system menu).
    watched: Option<HANDLE>,
    /// The language whose word the left button went down on (a click is a press and a release on the same word).
    pressed: Option<Lang>,
    /// Told the language chosen with the switch (taken out while it runs).
    on_choose: Option<Box<dyn FnMut(Lang)>>,
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

    /// The paragraphs shown, each with its top (DIPs).
    fn tops(&self) -> Vec<(usize, f32)> {
        let mut y = PAD_TOP;
        let mut out = Vec::with_capacity(self.paras.len());
        for (i, p) in self.paras.iter().enumerate() {
            if p.role == Role::Others && !self.show_others {
                continue;
            }
            y += LINE_GAP;
            out.push((i, y));
            y += p.height + LINE_GAP;
        }
        out
    }

    /// Where the switch ends and the baseline it sits on: the right of the text area, the footer's last line.
    fn switch_anchor(&self) -> Option<(f32, f32)> {
        let (i, top) = self
            .tops()
            .into_iter()
            .find(|(i, _)| self.paras[*i].role == Role::Footer)?;
        Some((PAD_LEFT + self.width, top + self.paras[i].last_baseline))
    }

    fn switch_parts(&self) -> Vec<ui::SwitchPart> {
        self.switch.iter().map(|p| p.part).collect()
    }

    /// The language a click at the point (DIPs of the client area) chooses.
    fn hit(&self, x: f32, y: f32) -> Option<Lang> {
        let (right, baseline) = self.switch_anchor()?;
        ui::switch_hit(
            &self.switch_parts(),
            right,
            baseline,
            self.lang,
            (SWITCH_HIT_X, SWITCH_HIT_Y),
            (x, y),
        )
    }

    /// Lays the window out in `lang`. Everything is built before anything is replaced, so a failure keeps the old
    /// layout.
    fn relayout(&mut self, lang: Lang) -> Result<()> {
        let switch = build_switch(&self.fonts, lang)?;
        let paras = build_paras(
            &self.fonts,
            self.texts.get(lang),
            self.width,
            footer_width(self.width, &switch),
        )?;
        self.switch = switch;
        self.paras = paras;
        self.lang = lang;
        Ok(())
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
        // Brushes and positions first, so that nothing can return between BeginDraw and EndDraw.
        let mut brushes = Vec::with_capacity(self.paras.len());
        for p in &self.paras {
            // SAFETY: the color lives across the call.
            brushes.push(unsafe { t.CreateSolidColorBrush(&p.color, None)? });
        }
        let mut switch_brushes = Vec::with_capacity(self.switch.len());
        for p in &self.switch {
            // SAFETY: the color lives across the call.
            switch_brushes.push(unsafe { t.CreateSolidColorBrush(&p.color, None)? });
        }
        let tops = self.tops();
        let origins = self
            .switch_anchor()
            .map(|(right, baseline)| ui::switch_origins(&self.switch_parts(), right, baseline))
            .unwrap_or_default();
        let options = if self.color_fonts {
            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT
        } else {
            D2D1_DRAW_TEXT_OPTIONS_NONE
        };
        // SAFETY: Direct2D drawing between BeginDraw and EndDraw on the target we own.
        let r = unsafe {
            t.BeginDraw();
            t.Clear(Some(&LIGHT_YELLOW));
            for (i, top) in &tops {
                t.DrawTextLayout(
                    Vector2 {
                        X: PAD_LEFT,
                        Y: *top,
                    },
                    &self.paras[*i].layout,
                    &brushes[*i],
                    options,
                );
            }
            for ((p, brush), (x, y)) in self.switch.iter().zip(&switch_brushes).zip(&origins) {
                t.DrawTextLayout(Vector2 { X: *x, Y: *y }, &p.layout, brush, options);
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
        let last_baseline = last_baseline(&layout)?;
        Ok(Para {
            layout,
            color,
            height: m.height,
            role,
            last_baseline,
        })
    }
}

/// The baseline of a layout's last line, from the layout's top.
fn last_baseline(layout: &IDWriteTextLayout) -> Result<f32> {
    let mut count = 0u32;
    // SAFETY: valid layout and out parameters; the first call only counts the lines (it fails for want of room).
    let lines = unsafe {
        let _ = layout.GetLineMetrics(None, &mut count);
        let mut lines = vec![DWRITE_LINE_METRICS::default(); count.max(1) as usize];
        layout.GetLineMetrics(Some(&mut lines), &mut count)?;
        lines.truncate(count as usize);
        lines
    };
    Ok(match lines.split_last() {
        Some((last, before)) => before.iter().map(|l| l.height).sum::<f32>() + last.baseline,
        None => 0.0,
    })
}

/// The title, the lines, the orange line and the footer (narrower, to leave room for the switch).
fn build_paras(fonts: &Fonts, text: &Texts, width: f32, footer_width: f32) -> Result<Vec<Para>> {
    let dw = &fonts.dw;
    let mut paras = vec![para(
        dw,
        &fonts.title,
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
            dw,
            &fonts.body,
            &line,
            head.encode_utf16().count(),
            width,
            BLACK,
            Role::Line,
        )?);
    }
    paras.push(para(
        dw,
        &fonts.bold,
        &text.others,
        0,
        width,
        DARK_ORANGE,
        Role::Others,
    )?);
    paras.push(para(
        dw,
        &fonts.bold,
        &text.footer,
        0,
        footer_width,
        FIREBRICK,
        Role::Footer,
    )?);
    Ok(paras)
}

/// The parts of the switch with `shown` in bold black; the other language blue and underlined (the link), the bar gray.
fn build_switch(fonts: &Fonts, shown: Lang) -> Result<Vec<Piece>> {
    ui::SWITCH_LABELS
        .iter()
        .map(|(text, lang)| {
            let link = lang.is_some_and(|l| l != shown);
            let is_shown = *lang == Some(shown);
            let fmt = if is_shown {
                &fonts.switch_bold
            } else {
                &fonts.switch
            };
            let color = if link {
                LINK_BLUE
            } else if is_shown {
                BLACK
            } else {
                GRAY
            };
            let wide: Vec<u16> = text.encode_utf16().collect();
            // SAFETY: DirectWrite copies the text; the out parameter lives on the stack.
            unsafe {
                let layout = fonts.dw.CreateTextLayout(&wide, fmt, NO_WRAP, f32::MAX)?;
                if link {
                    layout.SetUnderline(
                        true,
                        DWRITE_TEXT_RANGE {
                            startPosition: 0,
                            length: wide.len() as u32,
                        },
                    )?;
                }
                let mut m = DWRITE_TEXT_METRICS::default();
                layout.GetMetrics(&mut m)?;
                let baseline = last_baseline(&layout)?;
                Ok(Piece {
                    layout,
                    color,
                    part: ui::SwitchPart {
                        width: m.widthIncludingTrailingWhitespace,
                        height: m.height,
                        baseline,
                        lang: *lang,
                    },
                })
            }
        })
        .collect()
}

/// The footer's width: the text area less the switch and the gap before it (never less than half of it).
fn footer_width(width: f32, switch: &[Piece]) -> f32 {
    let taken: f32 = switch.iter().map(|p| p.part.width).sum();
    (width - taken - SWITCH_GAP).max(width / 2.0)
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
    /// Creates the window (hidden) in `lang`. `others` shows the orange line from the start. `on_choose` is told the
    /// language chosen with the switch, after the window has switched.
    pub fn create(
        texts: &Both,
        lang: Lang,
        others: bool,
        on_choose: Box<dyn FnMut(Lang)>,
    ) -> Result<Window> {
        register_class()?;
        // SAFETY: factory creation and layout calls with valid arguments; the state pointer is handed to the
        // window and freed in Drop after the window is destroyed.
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let loc = locale();
            let fonts = Fonts {
                title: format(&dw, DWRITE_FONT_WEIGHT_BOLD, TITLE_SIZE, &loc)?,
                body: format(&dw, DWRITE_FONT_WEIGHT_NORMAL, BODY_SIZE, &loc)?,
                bold: format(&dw, DWRITE_FONT_WEIGHT_BOLD, BODY_SIZE, &loc)?,
                switch: format(&dw, DWRITE_FONT_WEIGHT_NORMAL, SWITCH_SIZE, &loc)?,
                switch_bold: format(&dw, DWRITE_FONT_WEIGHT_BOLD, SWITCH_SIZE, &loc)?,
                dw,
            };

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

            let switch = build_switch(&fonts, lang)?;
            let paras = build_paras(&fonts, texts.get(lang), width, footer_width(width, &switch))?;
            let title = HSTRING::from(texts.get(lang).title.as_str());

            let hinstance = GetModuleHandleW(None)?;
            let state = Box::into_raw(Box::new(State {
                d2d,
                target: None,
                fonts,
                width,
                texts: texts.clone(),
                lang,
                paras,
                switch,
                show_others: others,
                dpi,
                destroyed: false,
                color_fonts: true,
                painted: false,
                watched: None,
                pressed: None,
                on_choose: Some(on_choose),
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
                &title,
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

/// The point of a mouse message in DIPs of the client area.
fn dip_point(lparam: LPARAM, dpi: u32) -> (f32, f32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i16;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16;
    let k = 96.0 / dpi.max(1) as f32;
    (f32::from(x) * k, f32::from(y) * k)
}

/// Whether the mouse pointer is on the switch's clickable word.
///
/// # Safety
///
/// `ptr` is the window's state.
unsafe fn pointer_on_switch(hwnd: HWND, ptr: *mut State) -> bool {
    let mut pt = POINT::default();
    // SAFETY: valid window handle and out parameter.
    let found = unsafe { GetCursorPos(&mut pt).is_ok() && ScreenToClient(hwnd, &mut pt).as_bool() };
    if !found {
        return false;
    }
    // SAFETY: (caller) a valid state; the reference ends in this function.
    let st = unsafe { &*ptr };
    let k = 96.0 / st.dpi.max(1) as f32;
    st.hit(pt.x as f32 * k, pt.y as f32 * k).is_some()
}

/// Switches the window to `lang` (texts, caption, height) and then tells the owner. Nothing here activates the
/// window: the caption and the size change with SWP_NOACTIVATE, and the click never activated it (MA_NOACTIVATE).
///
/// # Safety
///
/// `ptr` is the window's state, and no reference into it is held by the caller: the calls below re-enter the window
/// procedure.
unsafe fn choose(hwnd: HWND, ptr: *mut State, lang: Lang) {
    // SAFETY: (caller) a valid state; the reference ends before the Win32 calls.
    let (title, client_h, dpi) = unsafe {
        let st = &mut *ptr;
        if st.relayout(lang).is_err() {
            return;
        }
        (
            HSTRING::from(st.texts.get(lang).title.as_str()),
            st.client_height_dip(),
            st.dpi,
        )
    };
    // SAFETY: valid window handle on its own thread.
    unsafe {
        let _ = SetWindowTextW(hwnd, &title);
        if let Ok((w, h)) = outer_size(client_h, dpi) {
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                w,
                h,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    // The callback is taken out of the state while it runs, so that no reference into the state is held then.
    // SAFETY: (caller) the state stays valid until the window is dropped, which cannot happen during this message.
    let callback = unsafe { (*ptr).on_choose.take() };
    if let Some(mut tell) = callback {
        tell(lang);
        // SAFETY: as above.
        unsafe { (*ptr).on_choose = Some(tell) };
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
            // The hand over the switch's clickable word; elsewhere the class's arrow.
            WM_SETCURSOR
                if (lparam.0 & 0xFFFF) as u32 == HTCLIENT && pointer_on_switch(hwnd, ptr) =>
            {
                match LoadCursorW(None, IDC_HAND) {
                    Ok(hand) => {
                        SetCursor(Some(hand));
                        LRESULT(1)
                    }
                    Err(_) => DefWindowProcW(hwnd, msg, wparam, lparam),
                }
            }
            WM_LBUTTONDOWN => {
                let st = &mut *ptr;
                let (x, y) = dip_point(lparam, st.dpi);
                st.pressed = st.hit(x, y);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let chosen = {
                    let st = &mut *ptr;
                    let (x, y) = dip_point(lparam, st.dpi);
                    let pressed = st.pressed.take();
                    st.hit(x, y).filter(|l| pressed == Some(*l))
                };
                if let Some(lang) = chosen {
                    choose(hwnd, ptr, lang);
                }
                LRESULT(0)
            }
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

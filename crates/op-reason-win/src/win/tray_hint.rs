//! The pointer to 1Password's icon in the tray (design `2026-11-26-prompt-guide-design.md` §4, §6): a small layered
//! window next to the notification area of the taskbar, with an arrow at it, that says where 1Password keeps the prompt
//! it did not show and about how long it keeps it. It never takes the focus or a click (`WS_EX_TRANSPARENT`), stays out
//! of the taskbar and Alt+Tab, and is drawn with Direct2D's software renderer into a bitmap with an alpha channel
//! (`UpdateLayeredWindow`), in the look of Windows' app mode. Its picture is a sketch drawn with shapes and op-reason's
//! own words, not a copy of 1Password's menu.
//!
//! The relay's window thread owns it: it shows it when the prompt has not come, redraws it once a second
//! ([`Pointer::tick`]) and drops it when the prompt comes or the request ends.

use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::Instant;

use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
    D2D1_FIGURE_END_CLOSED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
    IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC,
    HGDIOBJ, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETTASKBARPOS, APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowExW, FindWindowW, GetWindowRect,
    HTTRANSPARENT, HWND_TOPMOST, MA_NOACTIVATE, RegisterClassExW, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, SetWindowTextW, ShowWindow, ULW_ALPHA,
    UpdateLayeredWindow, WINDOW_EX_STYLE, WM_DPICHANGED, WM_MOUSEACTIVATE, WM_NCHITTEST,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR, Result, w};
use windows_numerics::Vector2;

use super::screen;
use super::window::{self, color};
use crate::relay_prompt::{self, Px, Side, Spot};
use crate::relay_ui::PointerTexts;
use crate::theme::{self, Palette, Rgb, Theme};

const CLASS_NAME: PCWSTR = w!("op-reason-tray-pointer");
const EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(
    WS_EX_LAYERED.0
        | WS_EX_TRANSPARENT.0
        | WS_EX_NOACTIVATE.0
        | WS_EX_TOOLWINDOW.0
        | WS_EX_TOPMOST.0,
);

// The card (DIPs): its width, the room inside it, its corners and border; the picture at its left; the arrow.
const CARD_W: f32 = 340.0;
const PAD: f32 = 14.0;
const RADIUS: f32 = 10.0;
const BORDER: f32 = 2.0;
const PIC: (f32, f32) = (108.0, 90.0);
const TEXT_X: f32 = PAD + PIC.0 + 12.0;
const TEXT_W: f32 = CARD_W - TEXT_X - PAD;
const ARROW: f32 = 14.0;
const ARROW_HALF: f32 = 11.0;
/// Room around the card in the bitmap, for its shadow.
const MARGIN: f32 = 6.0;
/// How far from the taskbar the pointer stays, and how wide the notification area is taken to be when it cannot be
/// found (DIPs).
const GAP: f32 = 2.0;
const TRAY_GUESS: f32 = 160.0;

fn register_class() -> Result<()> {
    static DONE: OnceLock<bool> = OnceLock::new();
    if DONE.get().is_some() {
        return Ok(());
    }
    // SAFETY: a class with a static name and a valid window procedure.
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            let e = windows::core::Error::from_thread();
            if e.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
                return Err(e);
            }
        }
    }
    let _ = DONE.set(true);
    Ok(())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        // the pointer and clicks go to the window below
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        // placed for its screen's DPI when it is shown
        WM_DPICHANGED => LRESULT(0),
        // SAFETY: the default procedure with the arguments as received.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn px(r: RECT) -> Px {
    Px {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

/// Where the notification area is: the screen of the taskbar, the edge the taskbar is on, the notification area, the
/// screen's DPI.
struct Tray {
    screen: Px,
    side: Side,
    target: Px,
    dpi: u32,
}

/// The notification area of the primary taskbar (`TrayNotifyWnd` in `Shell_TrayWnd`), when it has a size.
fn notify_area() -> Option<RECT> {
    // SAFETY: plain queries with constant class names and an out parameter on the stack.
    unsafe {
        let tray = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
        let notify = FindWindowExW(Some(tray), None, w!("TrayNotifyWnd"), PCWSTR::null()).ok()?;
        let mut r = RECT::default();
        GetWindowRect(notify, &mut r).ok()?;
        (r.right > r.left && r.bottom > r.top).then_some(r)
    }
}

/// Where the pointer points: the notification area of the taskbar; else the end of the taskbar where it sits; else the
/// bottom right of the primary screen's work area.
fn find_tray() -> Tray {
    let mut bar = APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: HWND::default(),
        uCallbackMessage: 0,
        uEdge: 0,
        rc: RECT::default(),
        lParam: LPARAM(0),
    };
    // SAFETY: a valid structure with its size set.
    let found = unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut bar) } != 0;
    if found && bar.rc.right > bar.rc.left && bar.rc.bottom > bar.rc.top {
        let rc = bar.rc;
        let (bounds, dpi) = screen::bounds_of(rc).unwrap_or((rc, 96));
        let side = match bar.uEdge {
            ABE_LEFT => Side::Left,
            ABE_TOP => Side::Top,
            ABE_RIGHT => Side::Right,
            _ => Side::Bottom,
        };
        let guess = (TRAY_GUESS * dpi as f32 / 96.0) as i32;
        let end = match side {
            Side::Bottom | Side::Top => RECT {
                left: rc.right - guess,
                ..rc
            },
            Side::Left | Side::Right => RECT {
                top: rc.bottom - guess,
                ..rc
            },
        };
        let target = notify_area()
            .filter(|n| {
                n.left >= rc.left && n.right <= rc.right && n.top >= rc.top && n.bottom <= rc.bottom
            })
            .unwrap_or(end);
        return Tray {
            screen: px(bounds),
            side,
            target: px(target),
            dpi,
        };
    }
    let s = screen::primary();
    let w = s.work;
    let guess = (TRAY_GUESS * s.dpi as f32 / 96.0) as i32;
    Tray {
        screen: px(w),
        side: Side::Bottom,
        target: Px {
            left: w.right - guess,
            top: w.bottom - 1,
            right: w.right,
            bottom: w.bottom,
        },
        dpi: s.dpi,
    }
}

/// A memory device context with a 32-bit bitmap, and a Direct2D target on the software renderer bound to it.
struct Canvas {
    /// Dropped before the device context it is bound to.
    rt: ID2D1DCRenderTarget,
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    size: (i32, i32),
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: we made both; the old object goes back before the bitmap is deleted.
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

impl Canvas {
    fn new(d2d: &ID2D1Factory, (w, h): (i32, i32), dpi: u32) -> Result<Canvas> {
        // SAFETY: GDI and Direct2D calls with valid arguments; what was made is freed on any error.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err(windows::core::Error::from_thread());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(b) => b,
                Err(e) => {
                    let _ = DeleteDC(dc);
                    return Err(e);
                }
            };
            let old = SelectObject(dc, bitmap.into());
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi as f32,
                dpiY: dpi as f32,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = match d2d.CreateDCRenderTarget(&props) {
                Ok(rt) => rt,
                Err(e) => {
                    SelectObject(dc, old);
                    let _ = DeleteObject(bitmap.into());
                    let _ = DeleteDC(dc);
                    return Err(e);
                }
            };
            // ClearType needs an opaque background
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let canvas = Canvas {
                rt,
                dc,
                bitmap,
                old,
                size: (w, h),
            };
            canvas.rt.BindDC(
                canvas.dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                },
            )?;
            Ok(canvas)
        }
    }
}

/// The text formats of the card.
struct Fonts {
    dw: IDWriteFactory,
    title: IDWriteTextFormat,
    body: IDWriteTextFormat,
    bold: IDWriteTextFormat,
    item: IDWriteTextFormat,
}

impl Fonts {
    fn text(
        &self,
        fmt: &IDWriteTextFormat,
        s: &str,
        max_w: f32,
    ) -> Result<(IDWriteTextLayout, f32)> {
        let wide: Vec<u16> = s.encode_utf16().collect();
        // SAFETY: DirectWrite copies the text; the out parameter lives on the stack.
        unsafe {
            let layout = self
                .dw
                .CreateTextLayout(&wide, fmt, max_w.max(1.0), f32::MAX)?;
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            Ok((layout, m.height))
        }
    }
}

/// Where the card and its arrow are in the bitmap (DIPs), and where the window goes (pixels).
struct Geometry {
    card: (f32, f32),
    card_h: f32,
    /// The arrow's tip and the two ends of its base.
    arrow: [(f32, f32); 3],
    at: POINT,
    size: (i32, i32),
    dpi: u32,
}

/// Lays the pointer out for a card `card_h` high next to the notification area.
fn geometry(tray: &Tray, card_h: f32) -> Geometry {
    let k = tray.dpi as f32 / 96.0;
    let side = tray.side;
    let (w, h) = match side {
        Side::Bottom | Side::Top => (CARD_W + 2.0 * MARGIN, card_h + ARROW + 2.0 * MARGIN),
        Side::Left | Side::Right => (CARD_W + ARROW + 2.0 * MARGIN, card_h + 2.0 * MARGIN),
    };
    let size = ((w * k).ceil() as i32, (h * k).ceil() as i32);
    let inset = ((MARGIN + RADIUS + ARROW_HALF + 2.0) * k) as i32;
    let Spot { left, top, tip } = relay_prompt::spot(
        tray.screen,
        side,
        tray.target,
        size,
        (GAP * k) as i32,
        inset,
    );
    let tip = tip as f32 / k;
    let card = match side {
        Side::Bottom | Side::Right => (MARGIN, MARGIN),
        Side::Top => (MARGIN, MARGIN + ARROW),
        Side::Left => (MARGIN + ARROW, MARGIN),
    };
    let arrow = match side {
        Side::Bottom => {
            let base = card.1 + card_h - BORDER;
            [
                (tip, base + ARROW),
                (tip - ARROW_HALF, base),
                (tip + ARROW_HALF, base),
            ]
        }
        Side::Top => {
            let base = card.1 + BORDER;
            [
                (tip, base - ARROW),
                (tip - ARROW_HALF, base),
                (tip + ARROW_HALF, base),
            ]
        }
        Side::Right => {
            let base = card.0 + CARD_W - BORDER;
            [
                (base + ARROW, tip),
                (base, tip - ARROW_HALF),
                (base, tip + ARROW_HALF),
            ]
        }
        Side::Left => {
            let base = card.0 + BORDER;
            [
                (base - ARROW, tip),
                (base, tip - ARROW_HALF),
                (base, tip + ARROW_HALF),
            ]
        }
    };
    Geometry {
        card,
        card_h,
        arrow,
        at: POINT { x: left, y: top },
        size,
        dpi: tray.dpi,
    }
}

/// The pointer to the tray, shown until it is dropped.
pub struct Pointer {
    hwnd: HWND,
    d2d: ID2D1Factory,
    fonts: Fonts,
    canvas: Canvas,
    geometry: Geometry,
    palette: &'static Palette,
    texts: PointerTexts,
    title: IDWriteTextLayout,
    title_h: f32,
    line: IDWriteTextLayout,
    line_h: f32,
    item: IDWriteTextLayout,
    /// When the request went to 1Password (the seconds left count from it).
    sent: Instant,
    /// The seconds drawn, and their line.
    closes: Option<(u64, IDWriteTextLayout)>,
}

impl Pointer {
    /// Shows the pointer next to the notification area of the taskbar, in the look of `theme`; `sent`: when the request
    /// went to 1Password. `None` when it cannot be made (the relay goes on without it).
    pub fn show(texts: PointerTexts, theme: Theme, sent: Instant) -> Option<Pointer> {
        Pointer::make(texts, theme, sent).ok()
    }

    fn make(texts: PointerTexts, theme: Theme, sent: Instant) -> Result<Pointer> {
        register_class()?;
        // SAFETY: factory creation, text formats and a hidden window of a registered class.
        let (d2d, fonts, hwnd) = unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let loc = window::locale();
            let item = window::format(&dw, DWRITE_FONT_WEIGHT_NORMAL, 10.0, &loc)?;
            item.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let sign = dw.CreateEllipsisTrimmingSign(&item)?;
            item.SetTrimming(
                &DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                },
                &sign,
            )?;
            let fonts = Fonts {
                title: window::format(&dw, DWRITE_FONT_WEIGHT_BOLD, 12.0, &loc)?,
                body: window::format(&dw, DWRITE_FONT_WEIGHT_NORMAL, 11.0, &loc)?,
                bold: window::format(&dw, DWRITE_FONT_WEIGHT_BOLD, 11.0, &loc)?,
                item,
                dw,
            };
            let hinstance = GetModuleHandleW(None)?;
            let hwnd = CreateWindowExW(
                EX_STYLE,
                CLASS_NAME,
                &HSTRING::from(texts.title.as_str()),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?;
            (d2d, fonts, hwnd)
        };
        let made = (|| {
            let (title, title_h) = fonts.text(&fonts.title, &texts.title, TEXT_W)?;
            let (line, line_h) = fonts.text(&fonts.body, &texts.line, TEXT_W)?;
            let (item, _) = fonts.text(&fonts.item, &texts.item, PIC.0 - 14.0)?;
            let (_, closes_h) =
                fonts.text(&fonts.bold, &texts.closes.replace("{0}", "60"), TEXT_W)?;
            let card_h =
                (PIC.1 + 2.0 * PAD).max(PAD + title_h + 4.0 + line_h + 6.0 + closes_h + PAD);
            let tray = find_tray();
            let geometry = geometry(&tray, card_h);
            let canvas = Canvas::new(&d2d, geometry.size, geometry.dpi)?;
            Ok::<_, windows::core::Error>((title, title_h, line, line_h, item, geometry, canvas))
        })();
        let (title, title_h, line, line_h, item, geometry, canvas) = match made {
            Ok(m) => m,
            Err(e) => {
                // SAFETY: our window, on this thread.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(e);
            }
        };
        let mut p = Pointer {
            hwnd,
            d2d,
            fonts,
            canvas,
            geometry,
            palette: theme::palette(theme),
            texts,
            title,
            title_h,
            line,
            line_h,
            item,
            sent,
            closes: None,
        };
        p.draw()?;
        // SAFETY: our window, on its own thread; nothing here activates it.
        unsafe {
            let _ = ShowWindow(p.hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                p.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        Ok(p)
    }

    /// Draws it again when the seconds left changed.
    pub fn tick(&mut self) {
        let secs = self.secs();
        if self.closes.as_ref().is_some_and(|(s, _)| *s == secs) {
            return;
        }
        if self.draw().is_err() {
            // rebuilt at the next tick
            if let Ok(c) = Canvas::new(&self.d2d, self.geometry.size, self.geometry.dpi) {
                self.canvas = c;
            }
            self.closes = None;
        }
    }

    fn secs(&self) -> u64 {
        relay_prompt::left_secs(u64::try_from(self.sent.elapsed().as_millis()).unwrap_or(u64::MAX))
    }

    /// Draws the card into the bitmap and puts it on the screen.
    fn draw(&mut self) -> Result<()> {
        let secs = self.secs();
        let text = self.texts.closes.replace("{0}", &secs.to_string());
        let (closes, _) = self.fonts.text(&self.fonts.bold, &text, TEXT_W)?;
        let p = self.palette;
        let rt = &self.canvas.rt;
        let g = &self.geometry;
        let brush = |c: Rgb, a: f32| {
            // SAFETY: the color lives across the call.
            unsafe { rt.CreateSolidColorBrush(&color(c, a), None) }
        };
        let surface = brush(p.surface, 1.0)?;
        let caution = brush(p.caution, 1.0)?;
        let text_b = brush(p.text, 1.0)?;
        let secondary = brush(p.secondary, 1.0)?;
        let line = brush(p.line, 1.0)?;
        let accent = brush(p.accent, 1.0)?;
        let highlight = brush(p.accent, 0.22)?;
        let on_accent = brush(p.on_accent, 1.0)?;
        let taskbar = brush(p.pic_taskbar, 1.0)?;
        let faint = brush(p.secondary, 0.6)?;
        let shadow = brush(Rgb(0, 0, 0), 0.25)?;
        // SAFETY: a geometry of our factory, built and closed before it is drawn.
        let arrow = unsafe {
            let geometry = self.d2d.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            let [tip, a, b] = g.arrow;
            sink.BeginFigure(Vector2 { X: a.0, Y: a.1 }, D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLine(Vector2 { X: tip.0, Y: tip.1 });
            sink.AddLine(Vector2 { X: b.0, Y: b.1 });
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            geometry
        };
        let (cx, cy) = g.card;
        let at = |x: f32, y: f32| Vector2 {
            X: cx + x,
            Y: cy + y,
        };
        let (px_, py) = (cx + PAD, cy + PAD);
        // SAFETY: Direct2D drawing between BeginDraw and EndDraw on the target we own.
        unsafe {
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));
            // the card: a shadow, the face, the border, the arrow
            rt.FillRoundedRectangle(
                &rounded(cx + 1.0, cy + 3.0, CARD_W, g.card_h, RADIUS),
                &shadow,
            );
            rt.FillRoundedRectangle(&rounded(cx, cy, CARD_W, g.card_h, RADIUS), &surface);
            rt.DrawRoundedRectangle(
                &rounded(
                    cx + BORDER / 2.0,
                    cy + BORDER / 2.0,
                    CARD_W - BORDER,
                    g.card_h - BORDER,
                    RADIUS,
                ),
                &caution,
                BORDER,
                None,
            );
            rt.FillGeometry(&arrow, &caution, None);
            // the picture: 1Password's menu with its item, a line to its icon in the tray, the tray with ^
            rt.FillRoundedRectangle(&rounded(px_, py, PIC.0, 40.0, 4.0), &surface);
            rt.DrawRoundedRectangle(
                &rounded(px_ + 0.5, py + 0.5, PIC.0 - 1.0, 39.0, 4.0),
                &line,
                1.0,
                None,
            );
            rt.FillRoundedRectangle(
                &rounded(px_ + 3.0, py + 3.0, PIC.0 - 6.0, 18.0, 3.0),
                &highlight,
            );
            rt.DrawTextLayout(
                Vector2 {
                    X: px_ + 7.0,
                    Y: py + 5.0,
                },
                &self.item,
                &text_b,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            rt.FillRectangle(
                &D2D_RECT_F {
                    left: px_ + 8.0,
                    top: py + 28.0,
                    right: px_ + 68.0,
                    bottom: py + 31.0,
                },
                &line,
            );
            let (ix, iy) = (px_ + 48.0, py + PIC.1 - 14.0);
            rt.DrawLine(
                Vector2 {
                    X: ix,
                    Y: py + 40.0,
                },
                Vector2 { X: ix, Y: iy - 9.0 },
                &caution,
                1.5,
                None,
            );
            rt.FillRoundedRectangle(&rounded(px_, py + PIC.1 - 28.0, PIC.0, 28.0, 4.0), &taskbar);
            rt.DrawLine(
                Vector2 {
                    X: px_ + 12.0,
                    Y: iy + 3.0,
                },
                Vector2 {
                    X: px_ + 17.0,
                    Y: iy - 3.0,
                },
                &text_b,
                1.6,
                None,
            );
            rt.DrawLine(
                Vector2 {
                    X: px_ + 17.0,
                    Y: iy - 3.0,
                },
                Vector2 {
                    X: px_ + 22.0,
                    Y: iy + 3.0,
                },
                &text_b,
                1.6,
                None,
            );
            rt.FillEllipse(&ellipse(ix, iy, 8.0), &accent);
            rt.FillEllipse(&ellipse(ix, iy - 1.5, 2.2), &on_accent);
            rt.DrawLine(
                Vector2 { X: ix, Y: iy - 1.0 },
                Vector2 { X: ix, Y: iy + 4.0 },
                &on_accent,
                2.0,
                None,
            );
            rt.FillEllipse(&ellipse(ix + 7.0, iy - 7.0, 3.5), &caution);
            rt.DrawEllipse(&ellipse(ix + 7.0, iy - 7.0, 6.0), &caution, 1.2, None);
            rt.FillRoundedRectangle(&rounded(px_ + 66.0, iy - 6.0, 12.0, 12.0, 2.0), &faint);
            rt.FillRectangle(
                &D2D_RECT_F {
                    left: px_ + 86.0,
                    top: iy - 5.0,
                    right: px_ + 100.0,
                    bottom: iy - 2.0,
                },
                &faint,
            );
            rt.FillRectangle(
                &D2D_RECT_F {
                    left: px_ + 86.0,
                    top: iy + 1.0,
                    right: px_ + 96.0,
                    bottom: iy + 4.0,
                },
                &faint,
            );
            // the words: where it waits, how to open it, the seconds left
            rt.DrawTextLayout(
                at(TEXT_X, PAD),
                &self.title,
                &text_b,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            rt.DrawTextLayout(
                at(TEXT_X, PAD + self.title_h + 4.0),
                &self.line,
                &secondary,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            rt.DrawTextLayout(
                at(TEXT_X, PAD + self.title_h + 4.0 + self.line_h + 6.0),
                &closes,
                &caution,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            rt.EndDraw(None, None)?;
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&g.at),
                Some(&SIZE {
                    cx: self.canvas.size.0,
                    cy: self.canvas.size.1,
                }),
                Some(self.canvas.dc),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                }),
                ULW_ALPHA,
            )?;
            let _ = SetWindowTextW(
                self.hwnd,
                &HSTRING::from(format!("{} · {text}", self.texts.title).as_str()),
            );
        }
        self.closes = Some((secs, closes));
        Ok(())
    }
}

impl Drop for Pointer {
    fn drop(&mut self) {
        // SAFETY: our window, on the thread that made it.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        },
        radiusX: r,
        radiusY: r,
    }
}

fn ellipse(x: f32, y: f32, r: f32) -> D2D1_ELLIPSE {
    D2D1_ELLIPSE {
        point: Vector2 { X: x, Y: y },
        radiusX: r,
        radiusY: r,
    }
}

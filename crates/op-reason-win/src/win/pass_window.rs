//! The toasts of the SSH relay (design `2026-11-18-pass-toasts-design.md` §4): signatures that pass without a window
//! show as cards at the top left of the screen of the foreground window. One layered window holds every card
//! ([`crate::pass_toast::Stack`] decides them) and is drawn with Direct2D into a bitmap with an alpha channel
//! (`UpdateLayeredWindow`). It never takes the focus, lets clicks and the pointer through (`WS_EX_TRANSPARENT`), is
//! always on top, stays out of the taskbar and Alt+Tab, and is hidden while no card is shown. Its window text lists the
//! cards' first lines, newest first.
//!
//! A UI thread of its own owns the window; the signing threads hand it their toasts through a channel and wake it with
//! an event, so that none of them ever waits for it. A window that cannot be made only ends that thread: the relay goes
//! on without toasts.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use op_reason_core::ssh::decide::Mark;
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_TEXT_METRICS, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory,
    IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC,
    HGDIOBJ, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HTTRANSPARENT, HWND_TOPMOST, MA_NOACTIVATE,
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, QS_ALLINPUT, RegisterClassExW,
    SPI_GETCLIENTAREAANIMATION, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetWindowPos, SetWindowTextW, ShowWindow,
    SystemParametersInfoW, ULW_ALPHA, UpdateLayeredWindow, WINDOW_EX_STYLE, WM_DPICHANGED,
    WM_MOUSEACTIVATE, WM_NCHITTEST, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{BOOL, HSTRING, PCWSTR, Result, w};
use windows_numerics::Vector2;

use super::screen::{self, Screen};
use super::window;
use crate::pass_toast::{CARD_H, CARD_W, CardView, EDGE, MAX_CARDS, Stack, slot_y};
use crate::relay_ui::PassTexts;

const CLASS_NAME: PCWSTR = w!("op-reason-pass-toast");
const EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(
    WS_EX_LAYERED.0
        | WS_EX_TRANSPARENT.0
        | WS_EX_NOACTIVATE.0
        | WS_EX_TOOLWINDOW.0
        | WS_EX_TOPMOST.0,
);

// The bitmap: every place of the stack, with room for the shadow and the glow (DIPs).
const STACK_W: f32 = EDGE + CARD_W + EDGE;
const STACK_H: f32 = slot_y(MAX_CARDS - 1) + CARD_H + EDGE;

// A card (DIPs from its top left): a check at the left, three lines of uniform height, the line of the time left.
const RADIUS: f32 = 6.0;
const TEXT_X: f32 = 36.0;
const TEXT_W: f32 = CARD_W - TEXT_X - 12.0;
const LINE1_TOP: f32 = 9.0;
const LINE2_TOP: f32 = 26.0;
const LINE3_TOP: f32 = 43.0;
const BODY_SIZE: f32 = 12.0;
const SMALL_SIZE: f32 = 11.0;
const BODY_LINE: (f32, f32) = (17.0, 13.0); // line height, baseline
const SMALL_LINE: (f32, f32) = (16.0, 12.0);
const CHECK: (f32, f32, f32) = (18.0, 17.5, 8.0); // center and radius
const BAR_TOP: f32 = CARD_H - 5.0;
/// The widest share of line 2 that the destination may take; the command gets the rest.
const DEST_SHARE: f32 = 0.6;

const fn rgb(r: u8, g: u8, b: u8) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}
/// The window's colors: light yellow, black, gray, dark gray (`window.rs`), and a green for "passed".
const LIGHT_YELLOW: D2D1_COLOR_F = rgb(0xFF, 0xFF, 0xE0);
const BLACK: D2D1_COLOR_F = rgb(0, 0, 0);
const GRAY: D2D1_COLOR_F = rgb(0x80, 0x80, 0x80);
const DIM: D2D1_COLOR_F = rgb(0x55, 0x55, 0x55);
const GREEN: D2D1_COLOR_F = rgb(0x15, 0x80, 0x3D);
const PALE_GREEN: D2D1_COLOR_F = rgb(0xDC, 0xFC, 0xE7);
const WHITE: D2D1_COLOR_F = rgb(0xFF, 0xFF, 0xFF);

/// A toast for the UI thread.
struct Pass {
    call: Mark,
    answer: String,
    texts: PassTexts,
}

/// The event that wakes the UI thread.
#[derive(Clone, Copy)]
struct Wake(HANDLE);

// SAFETY: an event handle may be set and waited on from any thread; it is never closed.
unsafe impl Send for Wake {}
// SAFETY: as above.
unsafe impl Sync for Wake {}

/// The toasts' UI thread, for the signing threads.
pub struct Toasts {
    tx: mpsc::Sender<Pass>,
    wake: Wake,
}

impl Toasts {
    /// Starts the UI thread. `forget`: how long a call's count is kept without a signature (the relay's time limit).
    pub fn start(forget: Duration) -> Option<Toasts> {
        // SAFETY: an unnamed auto-reset event, kept for the life of the relay.
        let event = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.ok()?;
        let wake = Wake(event);
        let (tx, rx) = mpsc::channel();
        let forget_ms = u64::try_from(forget.as_millis()).unwrap_or(u64::MAX);
        thread::Builder::new()
            .name("op-reason-toasts".to_owned())
            .spawn(move || run(&rx, wake, forget_ms))
            .ok()?;
        Some(Toasts { tx, wake })
    }

    /// A signature of `call` passed without a window, under the answer recorded as `answer`.
    pub fn pass(&self, call: Mark, answer: &str, texts: PassTexts) {
        let p = Pass {
            call,
            answer: answer.to_owned(),
            texts,
        };
        if self.tx.send(p).is_ok() {
            // SAFETY: a valid event handle.
            unsafe {
                let _ = SetEvent(self.wake.0);
            }
        }
    }
}

/// The UI thread: waits for a toast or the next frame, serves the window's messages, draws.
fn run(rx: &Receiver<Pass>, wake: Wake, forget_ms: u64) {
    let Ok(mut ui) = Ui::new(forget_ms) else {
        return;
    };
    loop {
        let now = ui.now();
        let timeout = ui.stack.next_wake(now).map_or(INFINITE, |t| {
            u32::try_from(t.saturating_sub(now)).unwrap_or(INFINITE - 1)
        });
        // SAFETY: a valid event handle; waits for it, a message or the time-out.
        unsafe {
            let _ = MsgWaitForMultipleObjectsEx(
                Some(&[wake.0]),
                timeout,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            );
        }
        window::pump();
        let now = ui.now();
        loop {
            match rx.try_recv() {
                Ok(p) => ui.pass(now, p),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        ui.show(now);
    }
}

/// Whether Windows' animation effects are on (on when it cannot be read).
fn animation_on() -> bool {
    let mut on = BOOL(1);
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes a BOOL.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut on as *mut BOOL as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
    };
    !read || on.as_bool()
}

fn px(dip: f32, dpi: u32) -> i32 {
    (dip * dpi as f32 / 96.0).ceil() as i32
}

/// The text formats of a card: one line each, cut with an ellipsis, every line as high as the others.
struct Fonts {
    dw: IDWriteFactory,
    bold: IDWriteTextFormat,
    body: IDWriteTextFormat,
    small: IDWriteTextFormat,
    chip: IDWriteTextFormat,
}

fn one_line(
    dw: &IDWriteFactory,
    bold: bool,
    size: f32,
    (height, baseline): (f32, f32),
    locale: &[u16],
) -> Result<IDWriteTextFormat> {
    let weight = if bold {
        DWRITE_FONT_WEIGHT_BOLD
    } else {
        DWRITE_FONT_WEIGHT_NORMAL
    };
    let f = window::format(dw, weight, size, locale)?;
    // SAFETY: a valid format and a trimming sign made for it.
    unsafe {
        f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        f.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, height, baseline)?;
        let sign = dw.CreateEllipsisTrimmingSign(&f)?;
        f.SetTrimming(
            &DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            },
            &sign,
        )?;
    }
    Ok(f)
}

/// A line of text laid out, and its width.
struct Text {
    layout: IDWriteTextLayout,
    width: f32,
}

fn text(fonts: &Fonts, format: &IDWriteTextFormat, s: &str, max: f32) -> Result<Text> {
    let wide: Vec<u16> = s.encode_utf16().collect();
    // SAFETY: DirectWrite copies the text; the out parameter lives on the stack.
    unsafe {
        let layout = fonts
            .dw
            .CreateTextLayout(&wide, format, max.max(1.0), 100.0)?;
        let mut m = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut m)?;
        Ok(Text {
            layout,
            width: m.width.min(max),
        })
    }
}

/// A card's lines, laid out for its texts and count.
struct Lines {
    texts: PassTexts,
    count: u32,
    word: Text,
    caller: Text,
    chip: Option<Text>,
    command: Text,
    destination: Option<Text>,
    why: Text,
}

impl Lines {
    /// Line 1 `Passed · <caller>` and the ×N tag; line 2 the command cut where the destination starts; line 3.
    fn new(fonts: &Fonts, t: &PassTexts, count: u32) -> Result<Lines> {
        let word = text(fonts, &fonts.bold, &t.word, TEXT_W)?;
        let chip = if count >= 2 {
            Some(text(
                fonts,
                &fonts.chip,
                &format!("×{count}"),
                TEXT_W / 3.0,
            )?)
        } else {
            None
        };
        let chip_room = chip.as_ref().map_or(0.0, |c| c.width + 16.0);
        let caller = text(
            fonts,
            &fonts.bold,
            &format!(" · {}", t.caller),
            TEXT_W - word.width - chip_room,
        )?;
        let destination = match &t.destination {
            Some(d) => Some(text(
                fonts,
                &fonts.body,
                &format!(" → {d}"),
                TEXT_W * DEST_SHARE,
            )?),
            None => None,
        };
        let dest_w = destination.as_ref().map_or(0.0, |d| d.width);
        let command = text(fonts, &fonts.body, &t.command, TEXT_W - dest_w)?;
        let why = text(fonts, &fonts.small, &t.why, TEXT_W)?;
        Ok(Lines {
            texts: t.clone(),
            count,
            word,
            caller,
            chip,
            command,
            destination,
            why,
        })
    }
}

/// The memory bitmap the cards are drawn into, the Direct2D target bound to it and its brushes.
struct Surface {
    paint: Paint,
    /// Dropped before the device context it is bound to.
    rt: ID2D1DCRenderTarget,
    gdi: Gdi,
    size: (i32, i32),
    dpi: u32,
}

/// A memory device context with a 32-bit bitmap selected into it.
struct Gdi {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
}

impl Drop for Gdi {
    fn drop(&mut self) {
        // SAFETY: we made both; the old object goes back before the bitmap is deleted.
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

impl Surface {
    fn new(d2d: &ID2D1Factory, (w, h): (i32, i32), dpi: u32) -> Result<Surface> {
        // SAFETY: GDI and Direct2D calls with valid arguments; the guard frees what was made on any error.
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
            let gdi = Gdi { dc, bitmap, old };
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi as f32,
                dpiY: dpi as f32,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = d2d.CreateDCRenderTarget(&props)?;
            // ClearType needs an opaque background
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            rt.BindDC(
                gdi.dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                },
            )?;
            Ok(Surface {
                paint: Paint::new(&rt)?,
                rt,
                gdi,
                size: (w, h),
                dpi,
            })
        }
    }
}

/// The brushes of the cards; each card sets their opacity to its own before it is drawn.
struct Paint {
    fill: ID2D1SolidColorBrush,
    border: ID2D1SolidColorBrush,
    shadow: ID2D1SolidColorBrush,
    glow: ID2D1SolidColorBrush,
    green: ID2D1SolidColorBrush,
    pale: ID2D1SolidColorBrush,
    white: ID2D1SolidColorBrush,
    black: ID2D1SolidColorBrush,
    dim: ID2D1SolidColorBrush,
}

impl Paint {
    fn new(rt: &ID2D1DCRenderTarget) -> Result<Paint> {
        let brush = |c: D2D1_COLOR_F, a: f32| {
            let c = D2D1_COLOR_F { a, ..c };
            // SAFETY: the color lives across the call.
            unsafe { rt.CreateSolidColorBrush(&c, None) }
        };
        Ok(Paint {
            fill: brush(LIGHT_YELLOW, 1.0)?,
            border: brush(GRAY, 0.8)?,
            shadow: brush(BLACK, 0.16)?,
            glow: brush(GREEN, 0.9)?,
            green: brush(GREEN, 1.0)?,
            pale: brush(PALE_GREEN, 1.0)?,
            white: brush(WHITE, 1.0)?,
            black: brush(BLACK, 1.0)?,
            dim: brush(DIM, 1.0)?,
        })
    }

    /// A card's opacity, and its glow.
    fn set(&self, opacity: f32, glow: f32) {
        let o = opacity.clamp(0.0, 1.0);
        // SAFETY: brushes of a live target.
        unsafe {
            for b in [
                &self.fill,
                &self.border,
                &self.shadow,
                &self.green,
                &self.pale,
                &self.white,
                &self.black,
                &self.dim,
            ] {
                b.SetOpacity(o);
            }
            self.glow.SetOpacity(o * glow.clamp(0.0, 1.0));
        }
    }
}

fn rounded(left: f32, top: f32, right: f32, bottom: f32, r: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left,
            top,
            right,
            bottom,
        },
        radiusX: r,
        radiusY: r,
    }
}

/// The toasts' window and what it draws.
struct Ui {
    hwnd: HWND,
    d2d: ID2D1Factory,
    fonts: Fonts,
    surface: Option<Surface>,
    /// The screen of the cards shown, chosen when the first card of an empty stack came.
    screen: Option<Screen>,
    stack: Stack<Mark>,
    lines: HashMap<u64, Lines>,
    start: Instant,
    visible: bool,
    /// Put the window on top again at the next draw (a toast came).
    raise: bool,
    title: String,
}

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
        // placed and drawn for its screen's DPI by the UI thread
        WM_DPICHANGED => LRESULT(0),
        // SAFETY: the default procedure with the arguments as received.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

impl Ui {
    fn new(forget_ms: u64) -> Result<Ui> {
        register_class()?;
        // SAFETY: factory creation and a hidden window with a registered class.
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let loc = window::locale();
            let fonts = Fonts {
                bold: one_line(&dw, true, BODY_SIZE, BODY_LINE, &loc)?,
                body: one_line(&dw, false, BODY_SIZE, BODY_LINE, &loc)?,
                small: one_line(&dw, false, SMALL_SIZE, SMALL_LINE, &loc)?,
                chip: one_line(&dw, true, SMALL_SIZE, SMALL_LINE, &loc)?,
                dw,
            };
            let hinstance = GetModuleHandleW(None)?;
            let hwnd = CreateWindowExW(
                EX_STYLE,
                CLASS_NAME,
                w!(""),
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
            Ok(Ui {
                hwnd,
                d2d,
                fonts,
                surface: None,
                screen: None,
                stack: Stack::new(animation_on(), forget_ms),
                lines: HashMap::new(),
                start: Instant::now(),
                visible: false,
                raise: false,
                title: String::new(),
            })
        }
    }

    fn now(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// A toast came: the first card of an empty stack chooses the screen and reads the animation setting.
    fn pass(&mut self, now: u64, p: Pass) {
        self.stack.advance(now);
        if self.stack.is_empty() {
            self.screen = Some(screen::of_foreground());
            self.stack.set_animate(animation_on());
        }
        self.stack.pass(now, p.call, &p.answer, p.texts);
        self.raise = true;
    }

    /// Draws the cards at `now`, or hides the window when none is left.
    fn show(&mut self, now: u64) {
        self.stack.advance(now);
        if self.stack.is_empty() {
            if self.visible {
                // SAFETY: our window, on its own thread.
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
                self.visible = false;
            }
            self.lines.clear();
            self.set_title(String::new());
            return;
        }
        if self.draw(now).is_err() {
            // rebuilt at the next frame
            self.surface = None;
            return;
        }
        // SAFETY: our window, on its own thread; nothing here activates it.
        unsafe {
            if !self.visible {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
            if self.raise {
                let _ = SetWindowPos(
                    self.hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                self.raise = false;
            }
        }
        self.set_title(self.stack.heads().join("\n"));
    }

    fn set_title(&mut self, title: String) {
        if title != self.title {
            // SAFETY: our window, on its own thread.
            unsafe {
                let _ = SetWindowTextW(self.hwnd, &HSTRING::from(title.as_str()));
            }
            self.title = title;
        }
    }

    /// Draws every card into the bitmap and puts it on the screen.
    fn draw(&mut self, now: u64) -> Result<()> {
        let Some(screen) = self.screen else {
            return Ok(());
        };
        let size = (px(STACK_W, screen.dpi), px(STACK_H, screen.dpi));
        if self
            .surface
            .as_ref()
            .is_none_or(|s| s.size != size || s.dpi != screen.dpi)
        {
            self.surface = None;
            self.surface = Some(Surface::new(&self.d2d, size, screen.dpi)?);
        }
        let frame = self.stack.frame(now);
        // the lines of each card, laid out again when its count or texts changed (before BeginDraw: nothing may return
        // between it and EndDraw)
        self.lines.retain(|id, _| frame.iter().any(|c| c.id == *id));
        for c in &frame {
            if self
                .lines
                .get(&c.id)
                .is_none_or(|l| l.count != c.count || l.texts != *c.texts)
            {
                self.lines
                    .insert(c.id, Lines::new(&self.fonts, c.texts, c.count)?);
            }
        }
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        let rt = &surface.rt;
        // SAFETY: Direct2D drawing between BeginDraw and EndDraw on the target we own.
        unsafe {
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));
            // the leaving cards first, then from the bottom up: the top card is drawn last
            for c in frame.iter().rev() {
                if let Some(lines) = self.lines.get(&c.id) {
                    surface.paint.set(c.opacity, c.glow);
                    draw_card(rt, c, lines, &surface.paint);
                }
            }
            rt.EndDraw(None, None)?;
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&POINT {
                    x: screen.work.left,
                    y: screen.work.top,
                }),
                Some(&SIZE {
                    cx: size.0,
                    cy: size.1,
                }),
                Some(surface.gdi.dc),
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
        }
        Ok(())
    }
}

/// One card at its place: shadow, glow, light yellow face, gray border, the green check, three lines, the line of
/// the time left.
///
/// # Safety
///
/// Between `BeginDraw` and `EndDraw` of `rt`; the brushes are `rt`'s.
unsafe fn draw_card(rt: &ID2D1DCRenderTarget, c: &CardView<'_>, l: &Lines, p: &Paint) {
    let (x, y) = (EDGE, c.y);
    let (right, bottom) = (x + CARD_W, y + CARD_H);
    let at = |dx: f32, dy: f32| Vector2 {
        X: x + dx,
        Y: y + dy,
    };
    // SAFETY: (caller) inside BeginDraw and EndDraw, with brushes of this target.
    unsafe {
        rt.FillRoundedRectangle(
            &rounded(x - 1.0, y + 1.0, right + 1.0, bottom + 3.0, RADIUS + 1.0),
            &p.shadow,
        );
        if c.glow > 0.0 {
            rt.DrawRoundedRectangle(
                &rounded(x - 2.5, y - 2.5, right + 2.5, bottom + 2.5, RADIUS + 2.5),
                &p.glow,
                2.5,
                None,
            );
        }
        rt.FillRoundedRectangle(&rounded(x, y, right, bottom, RADIUS), &p.fill);
        rt.DrawRoundedRectangle(
            &rounded(x + 0.5, y + 0.5, right - 0.5, bottom - 0.5, RADIUS),
            &p.border,
            1.0,
            None,
        );
        let (cx, cy, r) = CHECK;
        rt.FillEllipse(
            &D2D1_ELLIPSE {
                point: at(cx, cy),
                radiusX: r,
                radiusY: r,
            },
            &p.green,
        );
        rt.DrawLine(
            at(cx - 4.0, cy + 0.3),
            at(cx - 1.2, cy + 3.1),
            &p.white,
            1.8,
            None,
        );
        rt.DrawLine(
            at(cx - 1.2, cy + 3.1),
            at(cx + 4.2, cy - 3.0),
            &p.white,
            1.8,
            None,
        );
        let options = D2D1_DRAW_TEXT_OPTIONS_NONE;
        rt.DrawTextLayout(at(TEXT_X, LINE1_TOP), &l.word.layout, &p.green, options);
        rt.DrawTextLayout(
            at(TEXT_X + l.word.width, LINE1_TOP),
            &l.caller.layout,
            &p.black,
            options,
        );
        if let Some(chip) = &l.chip {
            let left = TEXT_X + l.word.width + l.caller.width + 6.0;
            // the tag's text sits on line 1's baseline
            let top = LINE1_TOP + BODY_LINE.1 - SMALL_LINE.1;
            rt.FillRoundedRectangle(
                &rounded(
                    x + left,
                    y + top,
                    x + left + chip.width + 10.0,
                    y + top + 15.0,
                    4.0,
                ),
                &p.pale,
            );
            rt.DrawTextLayout(at(left + 5.0, top), &chip.layout, &p.green, options);
        }
        rt.DrawTextLayout(at(TEXT_X, LINE2_TOP), &l.command.layout, &p.black, options);
        if let Some(d) = &l.destination {
            rt.DrawTextLayout(
                at(TEXT_X + l.command.width, LINE2_TOP),
                &d.layout,
                &p.black,
                options,
            );
        }
        rt.DrawTextLayout(at(TEXT_X, LINE3_TOP), &l.why.layout, &p.dim, options);
        if let Some(left) = c.left {
            rt.FillRectangle(
                &D2D_RECT_F {
                    left: x + TEXT_X,
                    top: y + BAR_TOP,
                    right: x + TEXT_X + TEXT_W * left,
                    bottom: y + BAR_TOP + 2.0,
                },
                &p.green,
            );
        }
    }
}

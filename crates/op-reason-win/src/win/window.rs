//! The approval window, drawn with Direct2D and DirectWrite (design `2026-11-26-modern-window-design.md`): top right of
//! the work area, always on top, never takes the focus by itself, not in the taskbar. It draws the [`look::View`] of its
//! texts: a header with a tile, the title and the time; the reason as the largest text; chips for who and where; the
//! command in a monospace box with colored words; the references (an `op` call) or what the signature is for (the
//! relay); the seconds left to answer (the relay); a band with a picture of the screen that marks where 1Password's
//! prompt usually comes and the tray; the orange line while other op.exe run; the red footer. Its colors follow
//! Windows' app mode, dark or light ([`crate::theme`]), also when it changes while the window is open; its caption keeps
//! the close button and the window's text but draws neither ([`super::theme::frame`]).
//!
//! Its controls are the language switch `English | 日本語`, a pill at the right of the footer's row (or of the
//! countdown's row in the relay's window that asks; design `2026-11-15-language-toggle-design.md` §4): a click on the
//! other language lays the window out again in that language and hands the choice to the caller; the links of the
//! references the call reads (design `2026-11-17-refs-in-window-design.md` §4): a click opens the item in the 1Password
//! app; and, in the SSH relay's window (design `2026-11-17-ssh-relay-design.md` §6), a row of buttons at the bottom: a
//! press and a release on a button is the answer, which the owner reads with [`Window::answer`]. None of them activates
//! the window, and the window has no default button: a key typed elsewhere never reaches it.
//!
//! Only its owner brings it to the front, after the click on a button that hands an SSH request on to 1Password
//! ([`Window::come_forward`]; design `2026-11-22-front-after-allow-design.md` §5): the click lets this process set the
//! foreground window. The window then gives the front back when it closes, unless another window took it meanwhile.
//!
//! It opens on the screen of the foreground window, with that screen's DPI, and stays there ([`super::screen`]; design
//! `2026-11-18-pass-toasts-design.md` §5). It is laid out for that screen's work area: a window that would be taller
//! draws its reason smaller, then leaves the band out, then cuts the command's box (design §4.13).
//!
//! The steps in which a window's thread waits on Windows (making the window, showing it, drawing it) are noted in
//! process-wide atomics ([`busy`]), so that the SSH relay's watchdog can say where a window that does not appear waits
//! (design `2026-11-22-front-after-allow-design.md` §12). The relay's windows draw into their device context with
//! Direct2D's software renderer ([`Window::draw_without_gpu`]).

use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use op_reason_core::lang::Lang;
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F,
    D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_CAP_STYLE_FLAT, D2D1_DASH_STYLE_DASH,
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_LINE_JOIN_MITER, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory,
    ID2D1HwndRenderTarget, ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_HIT_TEST_METRICS, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory,
    IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, HDC, InvalidateRect, PAINTSTRUCT, ScreenToClient,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, SetForegroundWindow,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, HWND_TOPMOST, IDC_ARROW,
    LoadCursorW, MA_NOACTIVATE, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
    WM_SETTINGCHANGE, WM_SIZE, WNDCLASSEXW, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EndMenu, GetCursorPos, HTCLIENT, IDC_HAND, KillTimer, SendMessageW, SetCursor, SetTimer,
    SetWindowTextW, WM_CANCELMODE, WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_SETCURSOR,
    WM_TIMER,
};
use windows::core::{BOOL, HSTRING, PCWSTR, Result, w};
use windows_numerics::Vector2;

use crate::look::{self, ChipKind, FactKind, Icon, Reason, RowTone, Sketch, Tone, View, Word};
use crate::theme::{self, Palette, Rgb, Theme};
use crate::ui::{self, Both, Button, Rect, Texts};

// The frame and the margins (DIPs).
const OUTER_WIDTH: f32 = 520.0;
const PAD_LEFT: f32 = 14.0;
const PAD_TOP: f32 = 12.0;
const PAD_RIGHT: f32 = 14.0;
/// Below the footer's row (the switch) or the buttons: the tests click 21 and 26 DIPs above the client area's bottom.
const PAD_BOTTOM: f32 = 11.0;
const SCREEN_MARGIN: f32 = 16.0;
/// Between the parts of the window.
const SECTION_GAP: f32 = 14.0;
// The header: the tile, and the room between it, the title and the time.
const TILE: f32 = 28.0;
const TILE_RADIUS: f32 = 7.0;
const TILE_GAP: f32 = 10.0;
// Font sizes (DIPs).
const TITLE_SIZE: f32 = 14.0;
const REASON_SIZE: f32 = 17.0;
/// The reason of a window that would be taller than its screen's work area.
const REASON_COMPACT_SIZE: f32 = 14.0;
const CODE_SIZE: f32 = 12.0;
const REF_SIZE: f32 = 11.0;
// The chips: their height, the room around their text, the mark before it, the gap between chips.
const CHIP_H: f32 = 24.0;
const CHIP_PAD: f32 = 9.0;
const CHIP_MARK: f32 = 16.0;
const CHIP_GAP: f32 = 6.0;
// The command's box.
const CODE_PAD_X: f32 = 10.0;
const CODE_PAD_Y: f32 = 8.0;
const CODE_RADIUS: f32 = 6.0;
/// How far the references, "and N more" and the hint are indented (after their small square).
const REF_INDENT: f32 = 14.0;
/// The column of a mark before a line (a note, the purpose, the footer's shield).
const MARK_W: f32 = 18.0;
// The band and its picture.
const BAND_PAD_X: f32 = 10.0;
const BAND_PAD_Y: f32 = 8.0;
const BAND_RADIUS: f32 = 8.0;
const SKETCH: (f32, f32) = (64.0, 46.0);
// The language switch's pill: its height, the room around each name, the gap before it.
const PILL_H: f32 = 24.0;
const PILL_PAD: f32 = 9.0;
const PILL_GAP: f32 = 12.0;
/// The bar of the seconds left to answer.
const BAR_H: f32 = 4.0;

// The relay's process tree (design `2026-11-27-relay-process-tree-design.md` §3.5): how far each level indents, the
// height of a row's line, where its text starts after its dot or elbow, the height of its pills, how far the boxes
// under the rows end before the right edge (inside the frames), and the gap between a row's parts.
const TREE_INDENT: f32 = 16.0;
const TREE_LINE: f32 = 16.0;
const TREE_TEXT: f32 = 14.0;
const TREE_PILL_H: f32 = 18.0;
const TREE_INSET: f32 = 14.0;
const TREE_GAP: f32 = 6.0;
/// The width of a layout that must not wrap.
const NO_WRAP: f32 = 10_000.0;
/// How long a closing window waits for the threads that open links.
const OPENING_WAIT: Duration = Duration::from_secs(2);
// The row of buttons at the bottom (the SSH relay's window): its height, the gap above it and between buttons, the room
// around a label, the narrowest button and the corners. The tests click 20 DIPs inside the first button's left edge
// and the last button's right edge, 26 DIPs above the client area's bottom (PAD_BOTTOM and half of BUTTON_H).
const BUTTON_H: f32 = 30.0;
const BUTTON_TOP_GAP: f32 = 14.0;
const BUTTON_GAP: f32 = 8.0;
const BUTTON_PAD_X: f32 = 14.0;
const BUTTON_MIN_W: f32 = 96.0;
const BUTTON_RADIUS: f32 = 6.0;
/// Buttons take no press until this long after they appear: a click meant for the window underneath, at the moment this
/// one comes up on top, must not answer it.
const BUTTON_ARMING: Duration = Duration::from_millis(400);

const CLASS_NAME: PCWSTR = w!("op-reason-approval");
/// The timer of [`Window::watch`].
const WATCH_TIMER: usize = 1;
/// The timer that redraws the countdown ([`Window::set_deadline`]).
const COUNTDOWN_TIMER: usize = 2;
const COUNTDOWN_MS: u32 = 250;
/// The timer that pulses the lit border ([`Window::set_alert`]), and the border's width. Each tick draws the whole
/// window with the software renderer: ten frames a second for a pulse a second.
const ALERT_TIMER: usize = 3;
const ALERT_MS: u32 = 100;
const ALERT_W: f32 = 2.5;
const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0);
const EX_STYLE: WINDOW_EX_STYLE =
    WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0 | WS_EX_NOACTIVATE.0);

/// A step in which the thread of a window waits on Windows ([`busy`]): the window's own, and those of the relay's
/// opening of a window around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Step {
    /// The Direct2D and DirectWrite factories.
    Factories = 1,
    /// The texts laid out, and the screen.
    Layout,
    /// `CreateWindowExW`.
    Create,
    /// `ShowWindow` and topmost.
    Show,
    /// Making the render target.
    Target,
    /// Drawing and presenting a frame.
    Draw,
    /// Writing `holder.json` (the relay).
    Holder,
    /// Starting the window's Windows notification (the relay).
    Toast,
}

impl Step {
    fn name(code: u8) -> &'static str {
        match code {
            1 => "factories",
            2 => "layout",
            3 => "create",
            4 => "show",
            5 => "target",
            6 => "draw",
            7 => "holder",
            8 => "toast",
            _ => "?",
        }
    }
}

/// When the step that runs began, in milliseconds on [`clock_ms`] (0: none runs).
static BUSY_SINCE: AtomicU64 = AtomicU64::new(0);
static BUSY_STEP: AtomicU8 = AtomicU8::new(0);

/// Milliseconds since the first window step of this process (at least 1).
pub fn clock_ms() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// The window step that runs now: when it began ([`clock_ms`]) and its name.
pub fn busy() -> Option<(u64, &'static str)> {
    let since = BUSY_SINCE.load(Ordering::SeqCst);
    (since != 0).then(|| (since, Step::name(BUSY_STEP.load(Ordering::SeqCst))))
}

/// A step running; when dropped, the step that ran before it (if any) is the one that runs again. Process-wide: the
/// relay shows one window at a time.
pub(crate) struct Busy {
    before: (u64, u8),
}

impl Busy {
    pub(crate) fn enter(step: Step) -> Busy {
        let before = (
            BUSY_SINCE.load(Ordering::SeqCst),
            BUSY_STEP.load(Ordering::SeqCst),
        );
        let busy = Busy { before };
        busy.next(step);
        busy
    }

    /// The next step of the same run (its clock starts again).
    pub(crate) fn next(&self, step: Step) {
        BUSY_STEP.store(step as u8, Ordering::SeqCst);
        BUSY_SINCE.store(clock_ms(), Ordering::SeqCst);
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY_STEP.store(self.before.1, Ordering::SeqCst);
        BUSY_SINCE.store(self.before.0, Ordering::SeqCst);
    }
}

/// A color of Direct2D at `alpha`.
pub(super) fn color(c: Rgb, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(c.0) / 255.0,
        g: f32::from(c.1) / 255.0,
        b: f32::from(c.2) / 255.0,
        a: alpha,
    }
}

/// A role of the palette ([`Palette`]), resolved at each draw: the theme may change while the window is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ink {
    Background,
    Text,
    Secondary,
    Tertiary,
    Surface,
    Line,
    CodeBg,
    CodeBorder,
    CodeText,
    CodePath,
    Accent,
    OnAccent,
    Caution,
    OnCaution,
    Critical,
    Orange,
    Success,
    Claude,
    BandBg,
    BandBorder,
    BandTitle,
    BandText,
    PicScreen,
    PicFrame,
    PicTaskbar,
    PicStand,
    PicWindow,
    SwitchShown,
    AlertBg,
    Tree,
}

impl Ink {
    const ALL: [Ink; 30] = [
        Ink::Background,
        Ink::Text,
        Ink::Secondary,
        Ink::Tertiary,
        Ink::Surface,
        Ink::Line,
        Ink::CodeBg,
        Ink::CodeBorder,
        Ink::CodeText,
        Ink::CodePath,
        Ink::Accent,
        Ink::OnAccent,
        Ink::Caution,
        Ink::OnCaution,
        Ink::Critical,
        Ink::Orange,
        Ink::Success,
        Ink::Claude,
        Ink::BandBg,
        Ink::BandBorder,
        Ink::BandTitle,
        Ink::BandText,
        Ink::PicScreen,
        Ink::PicFrame,
        Ink::PicTaskbar,
        Ink::PicStand,
        Ink::PicWindow,
        Ink::SwitchShown,
        Ink::AlertBg,
        Ink::Tree,
    ];

    fn rgb(self, p: &Palette) -> Rgb {
        match self {
            Ink::Background => p.background,
            Ink::Text => p.text,
            Ink::Secondary => p.secondary,
            Ink::Tertiary => p.tertiary,
            Ink::Surface => p.surface,
            Ink::Line => p.line,
            Ink::CodeBg => p.code_bg,
            Ink::CodeBorder => p.code_border,
            Ink::CodeText => p.code_text,
            Ink::CodePath => p.code_path,
            Ink::Accent => p.accent,
            Ink::OnAccent => p.on_accent,
            Ink::Caution => p.caution,
            Ink::OnCaution => p.on_caution,
            Ink::Critical => p.critical,
            Ink::Orange => p.orange,
            Ink::Success => p.success,
            Ink::Claude => p.claude,
            Ink::BandBg => p.band_bg,
            Ink::BandBorder => p.band_border,
            Ink::BandTitle => p.band_title,
            Ink::BandText => p.band_text,
            Ink::PicScreen => p.pic_screen,
            Ink::PicFrame => p.pic_frame,
            Ink::PicTaskbar => p.pic_taskbar,
            Ink::PicStand => p.pic_stand,
            Ink::PicWindow => p.pic_window,
            Ink::SwitchShown => p.switch_shown,
            Ink::AlertBg => p.alert_bg,
            Ink::Tree => p.tree,
        }
    }
}

/// A text laid out at its place (DIPs of the client area).
struct Label {
    layout: IDWriteTextLayout,
    x: f32,
    y: f32,
    h: f32,
    ink: Ink,
    /// Ranges in other inks (UTF-16 units): set as drawing effects at each draw, with that draw's brushes.
    tints: Vec<(u32, u32, Ink)>,
    /// The URL a click on its text opens (a reference of the call).
    link: Option<String>,
    /// Drawn inside this rectangle only (the command's box cut to the work area).
    clip: Option<Rect>,
}

/// A shape, drawn before the texts.
enum Part {
    /// A rectangle with round corners, filled (an ink at an opacity) and stroked (an ink and a width), dashed or not.
    Box {
        r: Rect,
        radius: f32,
        fill: Option<(Ink, f32)>,
        stroke: Option<(Ink, f32)>,
        dashed: bool,
    },
    Dot {
        cx: f32,
        cy: f32,
        r: f32,
        fill: Option<Ink>,
        stroke: Option<(Ink, f32)>,
    },
    Line {
        a: (f32, f32),
        b: (f32, f32),
        ink: Ink,
        width: f32,
    },
}

/// The language switch laid out: its two halves (English, 日本語) and their names.
struct Pill {
    rects: [Rect; 2],
    names: [Label; 2],
}

/// A button of the row: what it answers, where it is, its label.
struct ButtonAt {
    button: Button,
    /// The first button, unless it is Deny: filled with the accent color, its label bold (design
    /// `2026-11-27-relay-process-tree-design.md` §4.4).
    filled: bool,
    rect: Rect,
    label: Label,
}

/// The countdown laid out: its bar, and where the line of the seconds left goes (redrawn each second).
struct CountdownAt {
    bar: Rect,
    x: f32,
    y: f32,
    w: f32,
    /// "N s left to answer" with `{0}` for N.
    template: String,
    /// What follows it: " · " and how long an answer lasts.
    rest: String,
}

/// Everything the window draws, laid out for one language.
struct Layout {
    parts: Vec<Part>,
    labels: Vec<Label>,
    pill: Option<Pill>,
    buttons: Vec<ButtonAt>,
    countdown: Option<CountdownAt>,
    /// The client area's height (DIPs).
    height: f32,
    /// The height of the command's text, for cutting it when the window would be too tall.
    code_h: f32,
}

/// How much a window that would be taller than the work area gives up (design §4.13); each level includes the ones
/// before it.
#[derive(Clone, Copy, PartialEq)]
enum Fit {
    Full,
    /// The reason drawn smaller.
    SmallReason,
    /// No band.
    NoBand,
    /// The processes between the root and the client in one row (design `2026-11-27-relay-process-tree-design.md`
    /// §3.6).
    FoldTree,
    /// The command's text cut to this height.
    CutCode(f32),
}

impl Fit {
    fn small_reason(self) -> bool {
        self != Fit::Full
    }

    fn no_band(self) -> bool {
        matches!(self, Fit::NoBand | Fit::FoldTree | Fit::CutCode(_))
    }

    fn fold_tree(self) -> bool {
        matches!(self, Fit::FoldTree | Fit::CutCode(_))
    }
}

/// What the left button went down on: a language of the switch, a link (its label), or a button.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Switch(Lang),
    Link(usize),
    Button(usize),
}

/// The text formats, kept to lay the window out again (another language, the orange line).
struct Fonts {
    dw: IDWriteFactory,
    title: IDWriteTextFormat,
    reason: IDWriteTextFormat,
    reason_small: IDWriteTextFormat,
    f12: IDWriteTextFormat,
    f12b: IDWriteTextFormat,
    f11: IDWriteTextFormat,
    f11b: IDWriteTextFormat,
    f10: IDWriteTextFormat,
    f10b: IDWriteTextFormat,
    m12: IDWriteTextFormat,
    m11: IDWriteTextFormat,
    m10: IDWriteTextFormat,
}

impl Fonts {
    fn new(dw: IDWriteFactory) -> Result<Fonts> {
        let loc = locale();
        let mono = mono_family(&dw);
        let normal = DWRITE_FONT_WEIGHT_NORMAL;
        let bold = DWRITE_FONT_WEIGHT_BOLD;
        Ok(Fonts {
            title: format(&dw, bold, TITLE_SIZE, &loc)?,
            reason: format(&dw, bold, REASON_SIZE, &loc)?,
            reason_small: format(&dw, bold, REASON_COMPACT_SIZE, &loc)?,
            f12: format(&dw, normal, 12.0, &loc)?,
            f12b: format(&dw, bold, 12.0, &loc)?,
            f11: format(&dw, normal, 11.0, &loc)?,
            f11b: format(&dw, bold, 11.0, &loc)?,
            f10: format(&dw, normal, 10.0, &loc)?,
            f10b: format(&dw, bold, 10.0, &loc)?,
            m12: format_in(&dw, mono, normal, CODE_SIZE, &loc)?,
            m11: format_in(&dw, mono, normal, REF_SIZE, &loc)?,
            m10: format_in(&dw, mono, normal, 10.0, &loc)?,
            dw,
        })
    }

    /// A text laid out in `fmt`, wrapped at `max_w`: the layout, its width and height.
    fn text(
        &self,
        fmt: &IDWriteTextFormat,
        s: &str,
        max_w: f32,
    ) -> Result<(IDWriteTextLayout, f32, f32)> {
        let wide: Vec<u16> = s.encode_utf16().collect();
        // SAFETY: DirectWrite copies the text; the out parameter lives on the stack.
        unsafe {
            let layout = self
                .dw
                .CreateTextLayout(&wide, fmt, max_w.max(1.0), f32::MAX)?;
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            Ok((layout, m.widthIncludingTrailingWhitespace, m.height))
        }
    }

    /// [`Fonts::text`] no taller than `max_h`, its last line ending with `…` when it is cut.
    fn cut_text(
        &self,
        fmt: &IDWriteTextFormat,
        s: &str,
        max_w: f32,
        max_h: f32,
    ) -> Result<(IDWriteTextLayout, f32, f32)> {
        let wide: Vec<u16> = s.encode_utf16().collect();
        // SAFETY: DirectWrite copies the text; the trimming sign is made for this format; out parameters on the stack.
        unsafe {
            let layout = self
                .dw
                .CreateTextLayout(&wide, fmt, max_w.max(1.0), max_h.max(1.0))?;
            let sign = self.dw.CreateEllipsisTrimmingSign(fmt)?;
            layout.SetTrimming(
                &DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                },
                &sign,
            )?;
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            Ok((
                layout,
                m.widthIncludingTrailingWhitespace,
                m.height.min(max_h),
            ))
        }
    }
}

/// Lays a window's view out ([`look::View`], design §4.2, §4.3).
struct Builder<'a> {
    f: &'a Fonts,
    parts: Vec<Part>,
    labels: Vec<Label>,
    /// The text area: from `PAD_LEFT`, this wide.
    width: f32,
}

impl Builder<'_> {
    fn right(&self) -> f32 {
        PAD_LEFT + self.width
    }

    /// Lays a text out and puts it at (`x`, `y`); returns its index, width and height.
    fn label(
        &mut self,
        fmt: &IDWriteTextFormat,
        s: &str,
        (x, y): (f32, f32),
        max_w: f32,
        ink: Ink,
    ) -> Result<(usize, f32, f32)> {
        let (layout, w, h) = self.f.text(fmt, s, max_w)?;
        self.labels.push(Label {
            layout,
            x,
            y,
            h,
            ink,
            tints: Vec::new(),
            link: None,
            clip: None,
        });
        Ok((self.labels.len() - 1, w, h))
    }

    /// A text with marked words ([`look::marked`]): the accent and the caution color.
    fn marked(
        &mut self,
        fmt: &IDWriteTextFormat,
        s: &str,
        at: (f32, f32),
        max_w: f32,
        ink: Ink,
    ) -> Result<f32> {
        let m = look::marked(s);
        let (i, _, h) = self.label(fmt, &m.text, at, max_w, ink)?;
        self.labels[i].tints = m
            .spans
            .iter()
            .map(|(start, len, tone)| {
                let ink = match tone {
                    Tone::Accent => Ink::Accent,
                    Tone::Caution => Ink::Caution,
                    Tone::Plain => ink,
                };
                (*start, *len, ink)
            })
            .collect();
        Ok(h)
    }

    fn rect(&mut self, r: Rect, radius: f32, fill: Option<(Ink, f32)>, stroke: Option<(Ink, f32)>) {
        self.parts.push(Part::Box {
            r,
            radius,
            fill,
            stroke,
            dashed: false,
        });
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), ink: Ink, width: f32) {
        self.parts.push(Part::Line { a, b, ink, width });
    }

    /// The command in its box with colored words (design §4.6), its top left at (`x`, `y`), `w` wide, cut to the height
    /// that `fit` keeps. Returns the box's height and the command's.
    fn code_box(&mut self, code: &str, (x, y): (f32, f32), w: f32, fit: Fit) -> Result<(f32, f32)> {
        let fonts = self.f;
        let inner_w = w - 2.0 * CODE_PAD_X;
        let (layout, _, code_h) = match fit {
            Fit::CutCode(keep) => fonts.cut_text(&fonts.m12, code, inner_w, keep)?,
            _ => fonts.text(&fonts.m12, code, inner_w)?,
        };
        let mut tints = Vec::new();
        for (start, len, word) in look::color_words(code) {
            match word {
                Word::Program => {
                    // SAFETY: a valid layout; the range is inside its text.
                    unsafe {
                        layout.SetFontWeight(
                            DWRITE_FONT_WEIGHT_BOLD,
                            DWRITE_TEXT_RANGE {
                                startPosition: start,
                                length: len,
                            },
                        )?;
                    }
                    tints.push((start, len, Ink::Accent));
                }
                Word::Path => tints.push((start, len, Ink::CodePath)),
                Word::Plain => {}
            }
        }
        let box_h = code_h + 2.0 * CODE_PAD_Y;
        self.rect(
            rect(x, y, w, box_h),
            CODE_RADIUS,
            Some((Ink::CodeBg, 1.0)),
            Some((Ink::CodeBorder, 1.0)),
        );
        self.labels.push(Label {
            layout,
            x: x + CODE_PAD_X,
            y: y + CODE_PAD_Y,
            h: code_h,
            ink: Ink::CodeText,
            tints,
            link: None,
            clip: matches!(fit, Fit::CutCode(_)).then(|| rect(x, y, w, box_h)),
        });
        Ok((box_h, code_h))
    }

    /// The relay's process tree from `y` (design `2026-11-27-relay-process-tree-design.md` §3): a row per process, each
    /// child indented under its parent with an elbow, the box (`v.code`, `v.code_note` under it) under its row, the small
    /// line, and the frames around the rows they cover, in the gaps. Returns where it ends and the command's height.
    fn tree(&mut self, tree: &look::Tree, v: &View, mut y: f32, fit: Fit) -> Result<(f32, f32)> {
        /// A part of a row: a text, or a text in a pill (the root's kind, or its mark).
        enum Piece {
            Text(IDWriteTextLayout, f32, f32, Ink),
            Pill(IDWriteTextLayout, f32, f32, bool),
        }
        let fonts = self.f;
        let x0 = PAD_LEFT;
        let right = self.right();
        let name_fmt = |solid: bool| if solid { &fonts.f10b } else { &fonts.f10 };
        let mut name_w = Vec::with_capacity(tree.frames.len());
        for f in &tree.frames {
            name_w.push(fonts.text(name_fmt(f.solid), &f.label, NO_WRAP)?.1);
        }
        let mut code_h = 0.0;
        let (mut tops, mut mids, mut ends) = (Vec::new(), Vec::new(), Vec::new());
        for (i, row) in tree.rows.iter().enumerate() {
            let rx = x0 + TREE_INDENT * row.depth as f32;
            let tx = rx + TREE_TEXT;
            // the parts flow onto more lines rather than run under the end, or under a frame's name on its first row
            let room = tree
                .frames
                .iter()
                .zip(&name_w)
                .filter(|(f, _)| f.first == i)
                .map(|(_, w)| w + 12.0)
                .fold(0.0, f32::max);
            let avail = (right - TREE_INSET - room - tx).max(60.0);
            let (name_f, name_ink) = match row.tone {
                RowTone::Strong => (&fonts.f12b, Ink::Text),
                RowTone::Plain => (&fonts.f12, Ink::Secondary),
                RowTone::Faint => (&fonts.f12, Ink::Tertiary),
            };
            let mut pieces = Vec::new();
            let (l, w, h) = fonts.text(name_f, &row.name, avail)?;
            pieces.push(Piece::Text(l, w, h, name_ink));
            if !row.detail.is_empty() {
                let (l, w, h) = fonts.text(&fonts.f11, &row.detail, avail)?;
                pieces.push(Piece::Text(l, w, h, Ink::Tertiary));
            }
            if let Some(p) = &row.pill {
                let (l, w, h) = fonts.text(&fonts.f11, p, avail - 14.0)?;
                pieces.push(Piece::Pill(l, w, h, false));
            }
            if let Some(m) = &row.mark {
                let (l, w, h) = fonts.text(&fonts.f11b, m, avail - 12.0)?;
                pieces.push(Piece::Pill(l, w, h, true));
            }
            let widths: Vec<f32> = pieces
                .iter()
                .map(|p| match p {
                    Piece::Text(_, w, _, _) => *w,
                    Piece::Pill(_, w, _, mark) => w + if *mark { 12.0 } else { 14.0 },
                })
                .collect();
            let heights: Vec<f32> = pieces
                .iter()
                .map(|p| match p {
                    Piece::Text(_, _, h, _) => *h,
                    Piece::Pill(_, _, h, _) => h.max(TREE_PILL_H),
                })
                .collect();
            let at = look::flow(&widths, avail, TREE_GAP);
            let lines = at.last().map_or(1, |a| a.0 + 1);
            let mut line_h = vec![TREE_LINE; lines];
            for (k, (r, _)) in at.iter().enumerate() {
                line_h[*r] = line_h[*r].max(heights[k]);
            }
            let top = y;
            let mut line_y = Vec::with_capacity(lines);
            let mut yy = top + 2.0;
            for h in &line_h {
                line_y.push(yy);
                yy += h;
            }
            let mid = top + 2.0 + line_h[0] / 2.0;
            // the elbow from the parent: down from below its dot or name, then right to this row
            if row.depth > 0
                && let Some(p) = (0..i).rev().find(|&k| tree.rows[k].depth + 1 == row.depth)
            {
                let px = x0 + TREE_INDENT * tree.rows[p].depth as f32 + 5.0;
                self.line((px, mids[p] + 6.0), (px, mid), Ink::Tree, 1.5);
                self.line((px, mid), (tx - 4.0, mid), Ink::Tree, 1.5);
            }
            if let Some(kind) = row.dot {
                let ink = match kind {
                    ChipKind::Claude => Ink::Claude,
                    ChipKind::Person => Ink::Success,
                    _ => Ink::Secondary,
                };
                self.parts.push(Part::Dot {
                    cx: rx + 5.0,
                    cy: mid,
                    r: 5.0,
                    fill: Some(ink),
                    stroke: None,
                });
            }
            for (k, (piece, (r, px))) in pieces.into_iter().zip(at).enumerate() {
                let (x, ly, lh) = (tx + px, line_y[r], line_h[r]);
                match piece {
                    Piece::Text(layout, _, h, ink) => self.labels.push(Label {
                        layout,
                        x,
                        y: ly + (lh - h) / 2.0,
                        h,
                        ink,
                        tints: Vec::new(),
                        link: None,
                        clip: None,
                    }),
                    Piece::Pill(layout, _, h, mark) => {
                        let (pad, fill, stroke, ink) = if mark {
                            (6.0, None, Some((Ink::Caution, 1.0)), Ink::Caution)
                        } else {
                            (
                                7.0,
                                Some((Ink::Surface, 1.0)),
                                Some((Ink::Line, 1.0)),
                                Ink::Claude,
                            )
                        };
                        let ph = heights[k];
                        let py = ly + (lh - ph) / 2.0;
                        self.rect(rect(x, py, widths[k], ph), ph / 2.0, fill, stroke);
                        self.labels.push(Label {
                            layout,
                            x: x + pad,
                            y: py + (ph - h) / 2.0,
                            h,
                            ink,
                            tints: Vec::new(),
                            link: None,
                            clip: None,
                        });
                    }
                }
            }
            y = yy + 2.0;
            let inner = right - TREE_INSET - tx;
            if i == tree.boxed {
                y += 2.0;
                let (box_h, h) = self.code_box(&v.code, (tx, y), inner, fit)?;
                code_h = h;
                y += box_h;
                if let Some(note) = &v.code_note {
                    y += 3.0;
                    let (_, _, h) = self.label(&fonts.m10, note, (tx, y), inner, Ink::Tertiary)?;
                    y += h;
                }
                y += 4.0;
            }
            if let Some((_, line)) = tree.small.as_ref().filter(|(k, _)| *k == i) {
                let (_, _, h) = self.label(&fonts.m10, line, (tx, y), inner, Ink::Tertiary)?;
                y += h + 4.0;
            }
            tops.push(top);
            mids.push(mid);
            ends.push(y);
        }
        // the frames, the outer one first: each inner one ends 8 DIPs inside the one around it
        let n = tree.frames.len();
        for (k, f) in tree.frames.iter().enumerate() {
            let (Some(&top), Some(&end), Some(first)) =
                (tops.get(f.first), ends.get(f.last), tree.rows.get(f.first))
            else {
                continue;
            };
            let left = x0 + TREE_INDENT * first.depth as f32 - 5.0;
            let fr = right - 8.0 * k as f32;
            self.parts.push(Part::Box {
                r: Rect {
                    left,
                    top: top - 1.0,
                    right: fr,
                    bottom: end + 1.0 + 3.0 * (n - 1 - k) as f32,
                },
                radius: 6.0,
                fill: None,
                stroke: Some((Ink::Accent, if f.solid { 1.5 } else { 1.0 })),
                dashed: !f.solid,
            });
            let (i, w, _) = self.label(
                name_fmt(f.solid),
                &f.label,
                (0.0, top + 3.0),
                NO_WRAP,
                Ink::Accent,
            )?;
            self.labels[i].x = fr - 6.0 - w;
        }
        Ok((y, code_h))
    }

    /// The header's tile: a lock on the accent color, or a key on the caution color.
    fn tile(&mut self, icon: Icon, x: f32, y: f32) {
        let (fill, ink) = match icon {
            Icon::Lock => (Ink::Accent, Ink::OnAccent),
            Icon::Key => (Ink::Caution, Ink::OnCaution),
        };
        self.rect(rect(x, y, TILE, TILE), TILE_RADIUS, Some((fill, 1.0)), None);
        match icon {
            Icon::Lock => {
                self.rect(
                    rect(x + 9.5, y + 7.0, 9.0, 10.0),
                    4.5,
                    None,
                    Some((ink, 2.0)),
                );
                self.rect(
                    rect(x + 8.0, y + 13.0, 12.0, 9.0),
                    2.0,
                    Some((ink, 1.0)),
                    None,
                );
            }
            Icon::Key => {
                self.parts.push(Part::Dot {
                    cx: x + 10.0,
                    cy: y + 14.0,
                    r: 4.0,
                    fill: None,
                    stroke: Some((ink, 2.0)),
                });
                self.line((x + 14.0, y + 14.0), (x + 22.0, y + 14.0), ink, 2.0);
                self.line((x + 19.0, y + 14.0), (x + 19.0, y + 18.0), ink, 2.0);
                self.line((x + 22.0, y + 14.0), (x + 22.0, y + 17.0), ink, 2.0);
            }
        }
    }

    /// The footer's shield (12 × 14).
    fn shield(&mut self, x: f32, y: f32) {
        let p = [
            (6.0, 0.0),
            (12.0, 2.5),
            (12.0, 7.0),
            (6.0, 14.0),
            (0.0, 7.0),
            (0.0, 2.5),
            (6.0, 0.0),
        ];
        for w in p.windows(2) {
            self.line(
                (x + w[0].0, y + w[0].1),
                (x + w[1].0, y + w[1].1),
                Ink::Critical,
                1.5,
            );
        }
    }

    /// A note's mark: "!" on a round caution mark.
    fn note_mark(&mut self, x: f32, y: f32) {
        self.parts.push(Part::Dot {
            cx: x + 6.0,
            cy: y + 7.0,
            r: 6.0,
            fill: Some(Ink::Caution),
            stroke: None,
        });
        self.line((x + 6.0, y + 3.8), (x + 6.0, y + 8.0), Ink::OnCaution, 1.8);
        self.parts.push(Part::Dot {
            cx: x + 6.0,
            cy: y + 10.2,
            r: 1.0,
            fill: Some(Ink::OnCaution),
            stroke: None,
        });
    }

    /// The purpose's mark: an arrow into a circle.
    fn purpose_mark(&mut self, x: f32, y: f32) {
        let (cx, cy) = (x + 7.0, y + 8.0);
        self.parts.push(Part::Dot {
            cx,
            cy,
            r: 6.0,
            fill: None,
            stroke: Some((Ink::Secondary, 1.5)),
        });
        self.line((cx - 3.0, cy), (cx + 3.0, cy), Ink::Secondary, 1.2);
        self.line((cx + 1.0, cy - 2.0), (cx + 3.0, cy), Ink::Secondary, 1.2);
        self.line((cx + 3.0, cy), (cx + 1.0, cy + 2.0), Ink::Secondary, 1.2);
    }

    /// The band's picture of the screen (design §4.9), or of 1Password's question when its prompt came (design
    /// `2026-11-26-prompt-guide-design.md` §6).
    fn sketch(&mut self, kind: Sketch, x: f32, y: f32) {
        if kind == Sketch::Prompt {
            self.prompt_sketch(x, y);
            return;
        }
        self.rect(
            rect(x, y, SKETCH.0, 40.0),
            3.0,
            Some((Ink::PicScreen, 1.0)),
            Some((Ink::PicFrame, 1.2)),
        );
        let window = if kind == Sketch::Ask {
            Ink::Caution
        } else {
            Ink::PicWindow
        };
        self.rect(
            rect(x + 44.0, y + 4.0, 16.0, 9.0),
            1.5,
            Some((window, 1.0)),
            Some((Ink::Secondary, 0.8)),
        );
        let prompt = rect(x + 22.0, y + 12.0, 20.0, 13.0);
        let (fill, stroke, dashed) = match kind {
            Sketch::Op => (Some((Ink::Accent, 1.0)), None, false),
            Sketch::Ask => (Some((Ink::Accent, 0.35)), Some((Ink::Accent, 1.0)), true),
            Sketch::Asking => (None, Some((Ink::Accent, 1.0)), true),
            Sketch::Tray | Sketch::Prompt => (None, None, false),
        };
        if fill.is_some() || stroke.is_some() {
            self.parts.push(Part::Box {
                r: prompt,
                radius: 2.0,
                fill,
                stroke,
                dashed,
            });
        }
        self.rect(
            rect(x + 1.0, y + 34.0, 62.0, 5.0),
            0.0,
            Some((Ink::PicTaskbar, 1.0)),
            None,
        );
        let (tx, ty) = (x + 57.0, y + 36.5);
        self.parts.push(Part::Dot {
            cx: tx,
            cy: ty,
            r: match kind {
                Sketch::Asking => 3.0,
                Sketch::Tray => 3.5,
                _ => 2.0,
            },
            fill: Some(Ink::Caution),
            stroke: None,
        });
        if kind == Sketch::Tray {
            // the tray ringed, and an arrow at it from the middle of the screen
            self.parts.push(Part::Dot {
                cx: tx,
                cy: ty,
                r: 6.5,
                fill: None,
                stroke: Some((Ink::Caution, 1.2)),
            });
            self.line(
                (x + 30.0, y + 16.0),
                (tx - 5.0, ty - 5.0),
                Ink::Caution,
                1.6,
            );
            self.line(
                (tx - 5.0, ty - 5.0),
                (tx - 10.5, ty - 6.0),
                Ink::Caution,
                1.6,
            );
            self.line(
                (tx - 5.0, ty - 5.0),
                (tx - 6.0, ty - 10.5),
                Ink::Caution,
                1.6,
            );
        }
        self.rect(
            rect(x + 28.0, y + 40.0, 8.0, 4.0),
            0.0,
            Some((Ink::PicStand, 1.0)),
            None,
        );
        self.rect(
            rect(x + 22.0, y + 44.0, 20.0, 2.0),
            0.0,
            Some((Ink::PicStand, 1.0)),
            None,
        );
    }

    /// A sketch of 1Password's question (design `2026-11-26-prompt-guide-design.md` §6): a dialog with a key, two lines of
    /// words and two buttons, drawn with shapes (not a copy of 1Password's window).
    fn prompt_sketch(&mut self, x: f32, y: f32) {
        self.rect(
            rect(x + 4.0, y + 2.0, 56.0, 42.0),
            4.0,
            Some((Ink::Surface, 1.0)),
            Some((Ink::Line, 1.0)),
        );
        self.parts.push(Part::Dot {
            cx: x + 15.0,
            cy: y + 14.0,
            r: 6.0,
            fill: Some(Ink::Accent),
            stroke: None,
        });
        self.parts.push(Part::Dot {
            cx: x + 13.5,
            cy: y + 14.0,
            r: 2.0,
            fill: None,
            stroke: Some((Ink::OnAccent, 1.2)),
        });
        self.line(
            (x + 15.5, y + 14.0),
            (x + 19.0, y + 14.0),
            Ink::OnAccent,
            1.2,
        );
        self.rect(
            rect(x + 25.0, y + 10.0, 28.0, 3.0),
            1.5,
            Some((Ink::Text, 1.0)),
            None,
        );
        self.rect(
            rect(x + 25.0, y + 16.0, 20.0, 3.0),
            1.5,
            Some((Ink::Secondary, 1.0)),
            None,
        );
        self.rect(
            rect(x + 11.0, y + 29.0, 19.0, 9.0),
            2.0,
            Some((Ink::Accent, 1.0)),
            None,
        );
        self.rect(
            rect(x + 34.0, y + 29.0, 19.0, 9.0),
            2.0,
            None,
            Some((Ink::Line, 1.0)),
        );
    }

    /// The switch's pill ending at `right`, its top at `top`; the language shown bold on its darker (lighter) half.
    fn pill(&self, lang: Lang, right: f32, top: f32) -> Result<Pill> {
        let mut laid = Vec::with_capacity(2);
        for (name, l) in ui::SWITCH {
            let fmt = if l == lang { &self.f.f11b } else { &self.f.f11 };
            laid.push((self.f.text(fmt, name, NO_WRAP)?, l));
        }
        let rects = look::pill([laid[0].0.1, laid[1].0.1], PILL_PAD, right, top, PILL_H);
        let mut names = laid
            .into_iter()
            .zip(rects)
            .map(|(((layout, _, h), l), r)| Label {
                layout,
                x: r.left + PILL_PAD,
                y: top + (PILL_H - h) / 2.0,
                h,
                ink: if l == lang { Ink::Text } else { Ink::Secondary },
                tints: Vec::new(),
                link: None,
                clip: None,
            });
        let (Some(en), Some(ja)) = (names.next(), names.next()) else {
            unreachable!("two languages");
        };
        Ok(Pill {
            rects,
            names: [en, ja],
        })
    }

    /// The width of the pill (it does not depend on where it goes).
    fn pill_width(&self, lang: Lang) -> Result<f32> {
        let p = self.pill(lang, 0.0, 0.0)?;
        Ok(p.rects[1].right - p.rects[0].left)
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

/// Lays `text` out in `lang`, `width` wide; the orange line counts only with `others`. A layout taller than `max_h`
/// gives up what design §4.13 says, in turn.
fn lay_out(
    fonts: &Fonts,
    text: &Texts,
    lang: Lang,
    width: f32,
    others: bool,
    max_h: f32,
) -> Result<Layout> {
    let mut layout = build(fonts, text, lang, width, others, Fit::Full)?;
    for fit in [Fit::SmallReason, Fit::NoBand, Fit::FoldTree] {
        if layout.height <= max_h {
            return Ok(layout);
        }
        layout = build(fonts, text, lang, width, others, fit)?;
    }
    if layout.height > max_h {
        let keep = (layout.code_h - (layout.height - max_h)).max(2.0 * CODE_SIZE * 1.4);
        if keep < layout.code_h {
            layout = build(fonts, text, lang, width, others, Fit::CutCode(keep))?;
        }
    }
    Ok(layout)
}

/// One layout of the view, top to bottom (design §4.2, §4.3).
fn build(
    fonts: &Fonts,
    t: &Texts,
    lang: Lang,
    width: f32,
    others: bool,
    fit: Fit,
) -> Result<Layout> {
    let v = &t.view;
    let mut b = Builder {
        f: fonts,
        parts: Vec::new(),
        labels: Vec::new(),
        width,
    };
    let x0 = PAD_LEFT;
    let right = b.right();
    let mut y = PAD_TOP;

    // the header: the tile, the title and the subtitle, the time and the date at the right
    b.tile(v.icon, x0, y);
    let (_, time_w, time_h) = b.label(&fonts.f12, &v.time, (0.0, y), NO_WRAP, Ink::Secondary)?;
    let time_i = b.labels.len() - 1;
    b.labels[time_i].x = right - time_w;
    let mut right_w = time_w;
    let mut date_h = 0.0;
    if !v.date.is_empty() {
        let (i, w, h) = b.label(
            &fonts.f10,
            &v.date,
            (0.0, y + time_h),
            NO_WRAP,
            Ink::Tertiary,
        )?;
        b.labels[i].x = right - w;
        right_w = right_w.max(w);
        date_h = h;
    }
    let tx = x0 + TILE + TILE_GAP;
    let tw = (right - right_w - 12.0 - tx).max(width / 3.0);
    let (_, _, title_h) = b.label(&fonts.title, &t.title, (tx, y - 1.0), tw, Ink::Text)?;
    let (_, _, sub_h) = b.label(
        &fonts.f11,
        &v.subtitle,
        (tx, y - 1.0 + title_h),
        tw,
        Ink::Secondary,
    )?;
    y += TILE.max(title_h + sub_h - 1.0).max(time_h + date_h) + SECTION_GAP;

    // the reason, the largest text (or the words in its place, in the caution color)
    let (_, _, h) = b.label(&fonts.f10, &v.reason_label, (x0, y), width, Ink::Secondary)?;
    y += h + 2.0;
    let (reason, ink) = match &v.reason {
        Reason::Given(r) => (r.as_str(), Ink::Text),
        Reason::Missing(m) => (m.as_str(), Ink::Caution),
    };
    let fmt = if fit.small_reason() {
        &fonts.reason_small
    } else {
        &fonts.reason
    };
    let (_, _, h) = b.label(fmt, reason, (x0, y), width, ink)?;
    y += h + 10.0;

    // the chips, in rows
    let mut chips = Vec::with_capacity(v.chips.len());
    for c in &v.chips {
        let mark = match c.kind {
            ChipKind::Claude | ChipKind::Person | ChipKind::Other | ChipKind::Repo => CHIP_MARK,
            ChipKind::Herdr | ChipKind::Wsl | ChipKind::Folder => 0.0,
        };
        let (layout, w, h) = fonts.text(&fonts.f11, &c.text, width - 2.0 * CHIP_PAD - mark)?;
        let chip_w = (2.0 * CHIP_PAD + mark + w).min(width);
        chips.push((c.kind, layout, chip_w, (h + 8.0).max(CHIP_H), h, mark));
    }
    if !chips.is_empty() {
        let widths: Vec<f32> = chips.iter().map(|c| c.2).collect();
        let at = look::flow(&widths, width, CHIP_GAP);
        let rows = at.last().map_or(0, |a| a.0 + 1);
        let mut row_h = vec![0f32; rows];
        for (i, (r, _)) in at.iter().enumerate() {
            row_h[*r] = row_h[*r].max(chips[i].3);
        }
        let mut row_y = Vec::with_capacity(rows);
        let mut yy = y;
        for h in &row_h {
            row_y.push(yy);
            yy += h + CHIP_GAP;
        }
        for ((kind, layout, w, h, text_h, mark), (r, cx)) in chips.into_iter().zip(at) {
            let (cx, cy) = (x0 + cx, row_y[r]);
            b.rect(
                rect(cx, cy, w, h),
                CHIP_H / 2.0,
                Some((Ink::Surface, 1.0)),
                Some((Ink::Line, 1.0)),
            );
            let mid = cy + CHIP_H / 2.0;
            let dot = match kind {
                ChipKind::Claude => Some(Ink::Claude),
                ChipKind::Person => Some(Ink::Success),
                ChipKind::Other => Some(Ink::Secondary),
                _ => None,
            };
            if let Some(ink) = dot {
                b.parts.push(Part::Dot {
                    cx: cx + CHIP_PAD + 5.0,
                    cy: mid,
                    r: 5.0,
                    fill: Some(ink),
                    stroke: None,
                });
            } else if kind == ChipKind::Repo {
                b.rect(
                    rect(cx + CHIP_PAD, mid - 5.0, 10.0, 10.0),
                    2.0,
                    None,
                    Some((Ink::Secondary, 1.5)),
                );
            }
            let ty = if text_h + 8.0 <= CHIP_H {
                cy + (CHIP_H - text_h) / 2.0
            } else {
                cy + 4.0
            };
            b.labels.push(Label {
                layout,
                x: cx + CHIP_PAD + mark,
                y: ty,
                h: text_h,
                ink: Ink::Text,
                tints: Vec::new(),
                link: None,
                clip: None,
            });
        }
        y = yy - CHIP_GAP + SECTION_GAP;
    }

    // the relay's process tree with the box under its row (on a short screen, the processes between in one row), or
    // the command in its box with colored words
    let tree = if fit.fold_tree() {
        v.tree_short.as_ref().or(v.tree.as_ref())
    } else {
        v.tree.as_ref()
    };
    let (_, _, h) = b.label(&fonts.f10, &v.code_label, (x0, y), width, Ink::Secondary)?;
    let code_h;
    if let Some(tree) = tree {
        y += h + 6.0;
        (y, code_h) = b.tree(tree, v, y, fit)?;
    } else {
        y += h + 4.0;
        let (box_h, h) = b.code_box(&v.code, (x0, y), width, fit)?;
        code_h = h;
        y += box_h;
        if let Some(note) = &v.code_note {
            y += 4.0;
            let (_, _, h) = b.label(&fonts.m10, note, (x0, y), width, Ink::Tertiary)?;
            y += h;
        }
    }

    // the references of an op call: a small square before each, the links underlined
    if let Some(r) = &t.refs {
        y += 12.0;
        let (_, _, h) = b.label(&fonts.f10, &r.head, (x0, y), width, Ink::Secondary)?;
        y += h + 4.0;
        for (text, url) in &r.items {
            let ink = if url.is_some() {
                Ink::Accent
            } else {
                Ink::Text
            };
            let (i, _, h) = b.label(
                &fonts.m11,
                text,
                (x0 + REF_INDENT, y),
                width - REF_INDENT,
                ink,
            )?;
            if url.is_some() {
                let length = text.encode_utf16().count() as u32;
                // SAFETY: a valid layout; the range is its whole text.
                unsafe {
                    b.labels[i].layout.SetUnderline(
                        true,
                        DWRITE_TEXT_RANGE {
                            startPosition: 0,
                            length,
                        },
                    )?;
                }
                b.labels[i].link = url.clone();
            }
            let line = h.min(REF_SIZE * 1.4);
            b.rect(
                rect(x0 + 2.0, y + (line - 6.0) / 2.0, 6.0, 6.0),
                1.5,
                Some((Ink::CodePath, 1.0)),
                None,
            );
            y += h + 2.0;
        }
        for line in [&r.more, &r.hint].into_iter().flatten() {
            let (_, _, h) = b.label(
                &fonts.f11,
                line,
                (x0 + REF_INDENT, y),
                width - REF_INDENT,
                Ink::Secondary,
            )?;
            y += h + 2.0;
        }
    }

    // the relay: what the signature is for, the destination and the key, the ssh command
    if !v.facts.is_empty() {
        y += 12.0;
        for fact in &v.facts {
            let at = (x0 + MARK_W, y);
            let w = width - MARK_W;
            match fact.kind {
                FactKind::Purpose => {
                    b.purpose_mark(x0, y);
                    let (_, _, h) = b.label(&fonts.f12, &fact.text, at, w, Ink::Text)?;
                    y += h + 3.0;
                }
                FactKind::Detail => {
                    let (_, _, h) = b.label(&fonts.f10, &fact.text, at, w, Ink::Tertiary)?;
                    y += h + 2.0;
                }
                FactKind::Mono => {
                    let (_, _, h) = b.label(&fonts.m10, &fact.text, at, w, Ink::Tertiary)?;
                    y += h + 2.0;
                }
            }
        }
    }

    // the notes of an op call
    for n in &v.notes {
        y += 8.0;
        b.note_mark(x0, y + 1.0);
        let (_, _, h) = b.label(
            &fonts.f11,
            n,
            (x0 + MARK_W, y),
            width - MARK_W,
            Ink::Secondary,
        )?;
        y += h;
    }

    // the seconds left to answer (the relay's window that asks), with the switch at the right of its row
    let mut pill = None;
    let mut countdown = None;
    if let Some(template) = &v.countdown {
        y += SECTION_GAP;
        let bar = rect(x0, y, width, BAR_H);
        y += BAR_H + 7.0;
        let p = b.pill(lang, right, y)?;
        let pill_w = p.rects[1].right - p.rects[0].left;
        let w = width - pill_w - PILL_GAP;
        let rest = if v.status.is_empty() {
            String::new()
        } else {
            format!(" · {}", v.status)
        };
        // measured with the widest number (an answer wait of an hour), so that the row keeps its height
        let (_, _, h) = fonts.text(
            &fonts.f10,
            &format!("{}{rest}", template.replace("{0}", "0000")),
            w,
        )?;
        countdown = Some(CountdownAt {
            bar,
            x: x0,
            y: y + 5.0,
            w,
            template: template.clone(),
            rest,
        });
        pill = Some(p);
        y += (h + 5.0).max(PILL_H);
    }

    // the band: where 1Password's prompt usually comes, and the tray (kept on a small screen while 1Password asks)
    if !fit.no_band() || v.band.keep {
        y += SECTION_GAP;
        let bx = x0 + BAND_PAD_X + SKETCH.0 + 12.0;
        let bw = right - BAND_PAD_X - bx;
        // the alert colors when 1Password's prompt did not come
        let (bg, border, title_ink, line_ink) = if v.band.alert {
            (Ink::AlertBg, Ink::Caution, Ink::Caution, Ink::Text)
        } else {
            (Ink::BandBg, Ink::BandBorder, Ink::BandTitle, Ink::BandText)
        };
        let first = b.labels.len();
        let th = b.marked(&fonts.f12b, &v.band.title, (bx, 0.0), bw, title_ink)?;
        let lh = b.marked(&fonts.f11, &v.band.line, (bx, 0.0), bw, line_ink)?;
        let inner = th + 3.0 + lh;
        let bh = inner.max(SKETCH.1) + 2.0 * BAND_PAD_Y;
        let ty = y + (bh - inner) / 2.0;
        b.labels[first].y = ty;
        b.labels[first + 1].y = ty + th + 3.0;
        // the box goes under the texts: parts are drawn first
        b.rect(
            rect(x0, y, width, bh),
            BAND_RADIUS,
            Some((bg, 1.0)),
            Some((border, 1.0)),
        );
        b.sketch(v.band.sketch, x0 + BAND_PAD_X, y + (bh - SKETCH.1) / 2.0);
        y += bh;
    }

    // the orange line, while other op.exe run
    if others && !t.others.is_empty() {
        y += 10.0;
        let (_, _, h) = b.label(&fonts.f11b, &t.others, (x0, y), width, Ink::Orange)?;
        y += h;
    }

    // the footer, with the switch at the right of its row's bottom when the countdown has not taken it
    let pill_here = pill.is_none();
    if !v.footer.is_empty() || pill_here {
        y += 12.0;
        let pill_w = if pill_here {
            b.pill_width(lang)? + PILL_GAP
        } else {
            0.0
        };
        let mut h = 0.0;
        let mut text_i = None;
        if !v.footer.is_empty() {
            let (i, _, fh) = b.label(
                &fonts.f11,
                &v.footer,
                (x0 + MARK_W, y),
                width - MARK_W - pill_w,
                Ink::Critical,
            )?;
            text_i = Some(i);
            h = fh;
        }
        let row = if pill_here { h.max(PILL_H) } else { h };
        let ty = if pill_here && h < PILL_H {
            y + (PILL_H - h) / 2.0
        } else {
            y
        };
        if let Some(i) = text_i {
            b.labels[i].y = ty;
            b.shield(x0, ty + 1.0);
        }
        if pill_here {
            pill = Some(b.pill(lang, right, y + row - PILL_H)?);
        }
        y += row;
    }

    // the buttons (the relay's window that asks): Deny at the right end
    let mut buttons = Vec::with_capacity(t.buttons.len());
    if !t.buttons.is_empty() {
        y += BUTTON_TOP_GAP;
        let mut x = x0;
        for (n, (button, label)) in t.buttons.iter().enumerate() {
            let filled = n == 0 && *button != Button::Deny;
            let fmt = if filled { &fonts.f12b } else { &fonts.f12 };
            let (layout, w, h) = fonts.text(fmt, label, NO_WRAP)?;
            let bw = (w + 2.0 * BUTTON_PAD_X).max(BUTTON_MIN_W);
            let left = if *button == Button::Deny {
                (right - bw).max(x)
            } else {
                x
            };
            let ink = if filled {
                Ink::OnAccent
            } else if *button == Button::Deny {
                Ink::Critical
            } else {
                Ink::Accent
            };
            buttons.push(ButtonAt {
                button: *button,
                filled,
                rect: rect(left, y, bw, BUTTON_H),
                label: Label {
                    layout,
                    x: left + (bw - w) / 2.0,
                    y: y + (BUTTON_H - h) / 2.0,
                    h,
                    ink,
                    tints: Vec::new(),
                    link: None,
                    clip: None,
                },
            });
            x = left + bw + BUTTON_GAP;
        }
        y += BUTTON_H;
    }
    y += PAD_BOTTOM;
    Ok(Layout {
        parts: b.parts,
        labels: b.labels,
        pill,
        buttons,
        countdown,
        height: y,
        code_h,
    })
}

/// Where the window draws ([`State::make_surface`]).
enum Surface {
    /// A render target of the window: Direct2D's choice (the GPU when there is one), presented at the display's
    /// refresh.
    Hwnd(ID2D1HwndRenderTarget),
    /// A DC render target on Direct2D's software renderer, bound to the window's device context in WM_PAINT: no swap
    /// chain and no wait for the display's refresh (the relay's windows).
    Dc(ID2D1DCRenderTarget),
}

impl Surface {
    fn rt(&self) -> &ID2D1RenderTarget {
        match self {
            Surface::Hwnd(t) => t,
            Surface::Dc(t) => t,
        }
    }
}

struct State {
    d2d: ID2D1Factory,
    /// The dashes of the prompt in the band's picture.
    dash: Option<ID2D1StrokeStyle>,
    target: Option<Surface>,
    fonts: Fonts,
    /// The width of the text area (DIPs).
    width: f32,
    /// The tallest client area that fits the work area (DIPs).
    max_h: f32,
    texts: Both,
    /// The language shown.
    lang: Lang,
    layout: Layout,
    show_others: bool,
    theme: Theme,
    dpi: u32,
    destroyed: bool,
    /// Draw emoji in color (turned off if drawing fails with it).
    color_fonts: bool,
    /// A frame was drawn and presented (EndDraw succeeded at least once).
    painted: bool,
    /// The op.exe whose end must break a modal loop (moving the window, its system menu).
    watched: Option<HANDLE>,
    /// What the left button went down on (a click is a press and a release on the same word or link).
    pressed: Option<Target>,
    /// Told the language chosen with the switch (taken out while it runs).
    on_choose: Option<Box<dyn FnMut(Lang)>>,
    /// The threads that open links (waited for a moment when the window closes).
    opening: Vec<JoinHandle<()>>,
    /// When the buttons appeared ([`BUTTON_ARMING`]).
    buttons_since: Instant,
    /// The button pressed and released (the answer).
    answer: Option<Button>,
    /// Set when the window came to the front ([`Window::come_forward`]): the window that was in front then (null when
    /// there was none), to give the front back to when this one closes.
    front_before: Option<HWND>,
    /// Draw with Direct2D's software renderer into the window's device context ([`Window::draw_without_gpu`]).
    software: bool,
    /// The end of the answer wait and its length ([`Window::set_deadline`]).
    deadline: Option<(Instant, Duration)>,
    /// The line of the seconds left as last laid out: the seconds and the layout.
    countdown_text: Option<(u64, IDWriteTextLayout)>,
    /// Lit up since then ([`Window::set_alert`]), pulsing when Windows' animation effects are on.
    alert: Option<Instant>,
    pulse: bool,
}

impl State {
    /// The link under the point (DIPs of the client area): the index of its label. Only the text itself counts
    /// (DirectWrite's hit test), on any of its lines.
    fn link_at(&self, x: f32, y: f32) -> Option<usize> {
        self.layout.labels.iter().enumerate().find_map(|(i, l)| {
            l.link.as_ref()?;
            let (lx, ly) = (x - l.x, y - l.y);
            if lx < 0.0 || ly < 0.0 || ly >= l.h {
                return None;
            }
            let mut trailing = BOOL(0);
            let mut inside = BOOL(0);
            let mut m = DWRITE_HIT_TEST_METRICS::default();
            // SAFETY: a valid layout and out parameters on the stack.
            unsafe {
                l.layout
                    .HitTestPoint(lx, ly, &mut trailing, &mut inside, &mut m)
                    .ok()?;
            }
            inside.as_bool().then_some(i)
        })
    }

    /// What a press or a release at the point is on: the switch's other language, a link, or a button.
    fn target(&self, x: f32, y: f32) -> Option<Target> {
        self.layout
            .pill
            .as_ref()
            .and_then(|p| look::pill_hit(&p.rects, self.lang, x, y))
            .map(Target::Switch)
            .or_else(|| self.link_at(x, y).map(Target::Link))
            .or_else(|| {
                self.layout
                    .buttons
                    .iter()
                    .position(|b| b.rect.contains(x, y))
                    .map(Target::Button)
            })
    }

    /// Lays the window out in `lang`. Everything is built before anything is replaced, so a failure keeps the old
    /// layout.
    fn relayout(&mut self, lang: Lang) -> Result<()> {
        let layout = lay_out(
            &self.fonts,
            self.texts.get(lang),
            lang,
            self.width,
            self.show_others,
            self.max_h,
        )?;
        self.layout = layout;
        self.lang = lang;
        self.countdown_text = None;
        Ok(())
    }

    /// Where the window draws: by default a render target of its own (Direct2D's choice: the GPU when there is one,
    /// presented at the display's refresh); for the relay's windows ([`Window::draw_without_gpu`]) a DC render target
    /// on Direct2D's software renderer, and the default only if that cannot be made.
    fn make_surface(&self, hwnd: HWND) -> Result<Surface> {
        if self.software {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: self.dpi as f32,
                dpiY: self.dpi as f32,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            // SAFETY: the properties live across the call.
            if let Ok(t) = unsafe { self.d2d.CreateDCRenderTarget(&props) } {
                return Ok(Surface::Dc(t));
            }
        }
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
        Ok(Surface::Hwnd(unsafe {
            self.d2d.CreateHwndRenderTarget(&props, &hprops)?
        }))
    }

    /// The share of the answer wait left and the seconds left (rounded up); all of it before the deadline is known.
    fn left(&self) -> (f32, Option<u64>) {
        match self.deadline {
            Some((end, total)) => {
                let left = end.saturating_duration_since(Instant::now());
                let share = left.as_secs_f32() / total.as_secs_f32().max(0.001);
                (
                    share.clamp(0.0, 1.0),
                    Some(left.as_secs_f32().ceil() as u64),
                )
            }
            None => (1.0, None),
        }
    }

    /// The line of the seconds left, laid out again when the second changed.
    fn countdown_line(&mut self, secs: Option<u64>) -> Result<()> {
        let Some(c) = self.layout.countdown.as_ref() else {
            return Ok(());
        };
        let key = secs.unwrap_or(u64::MAX);
        if self.countdown_text.as_ref().is_some_and(|(s, _)| *s == key) {
            return Ok(());
        }
        let n = secs.map_or_else(|| "…".to_owned(), |s| s.to_string());
        let s = format!("{}{}", c.template.replace("{0}", &n), c.rest);
        let (layout, _, _) = self.fonts.text(&self.fonts.f10, &s, c.w)?;
        self.countdown_text = Some((key, layout));
        Ok(())
    }

    /// Draws a frame: into the window's device context `hdc` of WM_PAINT for a DC render target.
    fn render(&mut self, hwnd: HWND, hdc: HDC) -> Result<()> {
        let busy = Busy::enter(Step::Target);
        if self.target.is_none() {
            self.target = Some(self.make_surface(hwnd)?);
        }
        busy.next(Step::Draw);
        let bound = match self.target.as_ref() {
            Some(Surface::Dc(dc)) => {
                let mut rc = RECT::default();
                // SAFETY: valid window handle, its paint DC and out parameters.
                unsafe { GetClientRect(hwnd, &mut rc).and_then(|()| dc.BindDC(hdc, &rc)) }.is_ok()
            }
            Some(Surface::Hwnd(_)) => true,
            None => return Ok(()),
        };
        if !bound {
            self.target = None;
            return Ok(());
        }
        let (share, secs) = self.left();
        self.countdown_line(secs)?;
        let Some(surface) = self.target.as_ref() else {
            return Ok(());
        };
        let t = surface.rt();
        let p = theme::palette(self.theme);
        // Brushes, drawing effects and positions first, so that nothing can return between BeginDraw and EndDraw.
        let mut brushes: Vec<ID2D1SolidColorBrush> = Vec::with_capacity(Ink::ALL.len());
        for ink in Ink::ALL {
            // SAFETY: the color lives across the call.
            brushes.push(unsafe { t.CreateSolidColorBrush(&color(ink.rgb(p), 1.0), None)? });
        }
        let brush = |i: Ink| &brushes[Ink::ALL.iter().position(|x| *x == i).unwrap_or(0)];
        let mut faint = Vec::new();
        for part in &self.layout.parts {
            faint.push(match part {
                Part::Box {
                    fill: Some((ink, alpha)),
                    ..
                } if *alpha < 1.0 => {
                    // SAFETY: the color lives across the call.
                    Some(unsafe { t.CreateSolidColorBrush(&color(ink.rgb(p), *alpha), None)? })
                }
                _ => None,
            });
        }
        for l in &self.layout.labels {
            for (start, len, ink) in &l.tints {
                // SAFETY: a valid layout and a brush of this target; the range is inside its text.
                unsafe {
                    l.layout.SetDrawingEffect(
                        brush(*ink),
                        DWRITE_TEXT_RANGE {
                            startPosition: *start,
                            length: *len,
                        },
                    )?;
                }
            }
        }
        // the lit border, at its opacity now (a pulse a second)
        let alert = match self.alert {
            Some(since) => {
                let opacity = if self.pulse {
                    let phase = since.elapsed().as_secs_f32() * std::f32::consts::TAU;
                    0.55 + 0.45 * (0.5 + 0.5 * phase.cos())
                } else {
                    1.0
                };
                // SAFETY: the color lives across the call.
                Some(unsafe { t.CreateSolidColorBrush(&color(p.caution, opacity), None)? })
            }
            None => None,
        };
        let client = Rect {
            left: 0.0,
            top: 0.0,
            right: PAD_LEFT + self.width + PAD_RIGHT,
            bottom: self.layout.height,
        };
        let pressed = match self.pressed {
            Some(Target::Button(i)) => Some(i),
            _ => None,
        };
        let options = if self.color_fonts {
            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT
        } else {
            D2D1_DRAW_TEXT_OPTIONS_NONE
        };
        let dash = self.dash.as_ref();
        let text = |l: &Label| {
            // SAFETY: (caller) inside BeginDraw and EndDraw, with a brush of this target.
            unsafe {
                if let Some(c) = l.clip {
                    t.PushAxisAlignedClip(&d2d_rect(c), D2D1_ANTIALIAS_MODE_ALIASED);
                }
                t.DrawTextLayout(Vector2 { X: l.x, Y: l.y }, &l.layout, brush(l.ink), options);
                if l.clip.is_some() {
                    t.PopAxisAlignedClip();
                }
            }
        };
        // SAFETY: Direct2D drawing between BeginDraw and EndDraw on the target we own.
        let r = unsafe {
            t.BeginDraw();
            t.Clear(Some(&color(p.background, 1.0)));
            for (part, faint) in self.layout.parts.iter().zip(&faint) {
                match part {
                    Part::Box {
                        r,
                        radius,
                        fill,
                        stroke,
                        dashed,
                    } => {
                        let rr = rounded(*r, *radius);
                        if let Some((ink, _)) = fill {
                            t.FillRoundedRectangle(&rr, faint.as_ref().unwrap_or(brush(*ink)));
                        }
                        if let Some((ink, width)) = stroke {
                            let style = if *dashed { dash } else { None };
                            t.DrawRoundedRectangle(&rr, brush(*ink), *width, style);
                        }
                    }
                    Part::Dot {
                        cx,
                        cy,
                        r,
                        fill,
                        stroke,
                    } => {
                        let e = D2D1_ELLIPSE {
                            point: Vector2 { X: *cx, Y: *cy },
                            radiusX: *r,
                            radiusY: *r,
                        };
                        if let Some(ink) = fill {
                            t.FillEllipse(&e, brush(*ink));
                        }
                        if let Some((ink, width)) = stroke {
                            t.DrawEllipse(&e, brush(*ink), *width, None);
                        }
                    }
                    Part::Line { a, b, ink, width } => {
                        t.DrawLine(
                            Vector2 { X: a.0, Y: a.1 },
                            Vector2 { X: b.0, Y: b.1 },
                            brush(*ink),
                            *width,
                            None,
                        );
                    }
                }
            }
            for l in &self.layout.labels {
                text(l);
            }
            if let Some(c) = &self.layout.countdown {
                t.FillRoundedRectangle(&rounded(c.bar, BAR_H / 2.0), brush(Ink::Line));
                let filled = Rect {
                    right: c.bar.left + (c.bar.right - c.bar.left) * share,
                    ..c.bar
                };
                if share > 0.0 {
                    t.FillRoundedRectangle(&rounded(filled, BAR_H / 2.0), brush(Ink::Caution));
                }
                if let Some((_, layout)) = &self.countdown_text {
                    t.DrawTextLayout(
                        Vector2 { X: c.x, Y: c.y },
                        layout,
                        brush(Ink::Secondary),
                        options,
                    );
                }
            }
            if let Some(pill) = &self.layout.pill {
                let all = Rect {
                    left: pill.rects[0].left,
                    ..pill.rects[1]
                };
                t.FillRoundedRectangle(&rounded(all, PILL_H / 2.0), brush(Ink::Surface));
                t.DrawRoundedRectangle(
                    &rounded(inset(all, 0.5), PILL_H / 2.0),
                    brush(Ink::Line),
                    1.0,
                    None,
                );
                let shown = usize::from(self.lang == Lang::Ja);
                t.FillRoundedRectangle(
                    &rounded(inset(pill.rects[shown], 2.0), PILL_H / 2.0 - 2.0),
                    brush(Ink::SwitchShown),
                );
                for name in &pill.names {
                    text(name);
                }
            }
            for (i, b) in self.layout.buttons.iter().enumerate() {
                let rr = rounded(b.rect, BUTTON_RADIUS);
                let (fill, border) = if b.filled {
                    (Ink::Accent, None)
                } else if b.button == Button::Deny {
                    (Ink::Surface, Some(Ink::Critical))
                } else {
                    (Ink::Surface, Some(Ink::Accent))
                };
                t.FillRoundedRectangle(&rr, brush(fill));
                // a pressed button: a wider border, in its label's color (the fill and the label stay)
                if pressed == Some(i) {
                    t.DrawRoundedRectangle(
                        &rounded(inset(b.rect, 1.0), BUTTON_RADIUS - 1.0),
                        brush(b.label.ink),
                        2.0,
                        None,
                    );
                } else if let Some(ink) = border {
                    t.DrawRoundedRectangle(
                        &rounded(inset(b.rect, 0.5), BUTTON_RADIUS),
                        brush(ink),
                        1.0,
                        None,
                    );
                }
                text(&b.label);
            }
            if let Some(lit) = &alert {
                t.DrawRoundedRectangle(
                    &rounded(inset(client, ALERT_W / 2.0), 6.0),
                    lit,
                    ALERT_W,
                    None,
                );
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

fn d2d_rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn rounded(r: Rect, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: d2d_rect(r),
        radiusX: radius,
        radiusY: radius,
    }
}

fn inset(r: Rect, by: f32) -> Rect {
    Rect {
        left: r.left + by,
        top: r.top + by,
        right: r.right - by,
        bottom: r.bottom - by,
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
            let e = windows::core::Error::from_thread();
            // Another thread of the relay registered it first
            if e.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
                return Err(e);
            }
        }
    }
    let _ = DONE.set(true);
    Ok(())
}

/// The user's locale name, NUL-terminated (the text formats' locale).
pub(super) fn locale() -> Vec<u16> {
    let mut buf = [0u16; 85];
    // SAFETY: the buffer is LOCALE_NAME_MAX_LENGTH long.
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n <= 0 {
        return "en-US\0".encode_utf16().collect();
    }
    buf[..n as usize].to_vec()
}

/// A Segoe UI text format that wraps words.
pub(super) fn format(
    dw: &IDWriteFactory,
    weight: DWRITE_FONT_WEIGHT,
    size: f32,
    locale: &[u16],
) -> Result<IDWriteTextFormat> {
    format_in(dw, w!("Segoe UI"), weight, size, locale)
}

/// A text format of `family` that wraps words.
pub(super) fn format_in(
    dw: &IDWriteFactory,
    family: PCWSTR,
    weight: DWRITE_FONT_WEIGHT,
    size: f32,
    locale: &[u16],
) -> Result<IDWriteTextFormat> {
    // SAFETY: the strings are NUL-terminated and outlive the call.
    unsafe {
        let f = dw.CreateTextFormat(
            family,
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

/// The monospace family of the command (design §4.5): Cascadia Mono when the system has it (Windows 11), else Consolas.
pub(super) fn mono_family(dw: &IDWriteFactory) -> PCWSTR {
    let mut fonts: Option<IDWriteFontCollection> = None;
    // SAFETY: an out parameter on the stack, and plain queries of the collection.
    let cascadia = unsafe {
        dw.GetSystemFontCollection(&mut fonts, false).is_ok()
            && fonts.is_some_and(|c| {
                let mut index = 0u32;
                let mut exists = BOOL(0);
                c.FindFamilyName(w!("Cascadia Mono"), &mut index, &mut exists)
                    .is_ok()
                    && exists.as_bool()
            })
    };
    if cascadia {
        w!("Cascadia Mono")
    } else {
        w!("Consolas")
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
    /// Creates the window (hidden) in `lang`. `others` shows the orange line from the start. `on_choose` is told the
    /// language chosen with the switch, after the window has switched.
    pub fn create(
        texts: &Both,
        lang: Lang,
        others: bool,
        on_choose: Box<dyn FnMut(Lang)>,
    ) -> Result<Window> {
        let busy = Busy::enter(Step::Factories);
        register_class()?;
        // SAFETY: factory creation and layout calls with valid arguments; the state pointer is handed to the
        // window and freed in Drop after the window is destroyed.
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            busy.next(Step::Layout);
            let fonts = Fonts::new(dw)?;
            let dash = d2d
                .CreateStrokeStyle(
                    &D2D1_STROKE_STYLE_PROPERTIES {
                        startCap: D2D1_CAP_STYLE_FLAT,
                        endCap: D2D1_CAP_STYLE_FLAT,
                        dashCap: D2D1_CAP_STYLE_FLAT,
                        lineJoin: D2D1_LINE_JOIN_MITER,
                        miterLimit: 10.0,
                        dashStyle: D2D1_DASH_STYLE_DASH,
                        dashOffset: 0.0,
                    },
                    None,
                )
                .ok();

            let screen = super::screen::of_foreground();
            let dpi = screen.dpi;
            let theme = super::theme::current();

            // The text width: the outer width minus the side frames and the padding; the tallest client area that
            // fits the work area with the margins and the frame.
            let mut frame = RECT::default();
            AdjustWindowRectExForDpi(&mut frame, STYLE, false, EX_STYLE, dpi)?;
            let k = 96.0 / dpi.max(1) as f32;
            let width = OUTER_WIDTH - (frame.right - frame.left) as f32 * k - PAD_LEFT - PAD_RIGHT;
            let max_h = (screen.work.bottom - screen.work.top) as f32 * k
                - 2.0 * SCREEN_MARGIN
                - (frame.bottom - frame.top) as f32 * k;

            let layout = lay_out(&fonts, texts.get(lang), lang, width, others, max_h)?;
            let title = HSTRING::from(texts.get(lang).title.as_str());

            let hinstance = GetModuleHandleW(None)?;
            let state = Box::into_raw(Box::new(State {
                d2d,
                dash,
                target: None,
                fonts,
                width,
                max_h,
                texts: texts.clone(),
                lang,
                layout,
                show_others: others,
                theme,
                dpi,
                destroyed: false,
                color_fonts: true,
                painted: false,
                watched: None,
                pressed: None,
                on_choose: Some(on_choose),
                opening: Vec::new(),
                buttons_since: Instant::now(),
                answer: None,
                front_before: None,
                software: false,
                deadline: None,
                countdown_text: None,
                alert: None,
                pulse: false,
            }));
            let (w, h) = match outer_size((*state).layout.height, dpi) {
                Ok(s) => s,
                Err(e) => {
                    drop(Box::from_raw(state));
                    return Err(e);
                }
            };
            let area = screen.work;
            let margin = scale(SCREEN_MARGIN, dpi);
            let x = area.right - w - margin;
            let y = area.top + margin;
            busy.next(Step::Create);
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
            super::theme::frame(hwnd, theme);
            Ok(Window { hwnd, state })
        }
    }

    /// Shows the window without activating it, then puts it on top (topmost has to be applied after showing).
    pub fn show(&self) {
        let _busy = Busy::enter(Step::Show);
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

    /// Draws the window with Direct2D's software renderer into its device context at each WM_PAINT (before
    /// [`Window::show`]): no GPU, no swap chain and no wait for the display's refresh. The SSH relay's window stopped
    /// answering at its first frame on a real desktop, with a build that the machine had never run (design
    /// `2026-11-22-front-after-allow-design.md` §12); a window this small draws as fast without the GPU.
    pub fn draw_without_gpu(&self) {
        // SAFETY: `state` stays valid until Drop; no Win32 call while the reference lives.
        unsafe {
            let st = &mut *self.state;
            st.software = true;
            st.target = None;
        }
    }

    /// The end of the answer wait, `total` long (the relay's window that asks): its bar and seconds are drawn from it
    /// and redrawn by a timer.
    pub fn set_deadline(&self, deadline: Instant, total: Duration) {
        // SAFETY: `state` stays valid until Drop; the reference ends before SetTimer.
        unsafe {
            (*self.state).deadline = Some((deadline, total));
            let _ = SetTimer(Some(self.hwnd), COUNTDOWN_TIMER, COUNTDOWN_MS, None);
        }
    }

    /// Lights the window up (a border in the caution color inside it, pulsing unless Windows' animation effects are off,
    /// and the frame's border on Windows 11) or puts it back: 1Password's prompt did not come (design
    /// `2026-11-26-prompt-guide-design.md` §6). Nothing here activates the window.
    pub fn set_alert(&self, on: bool) {
        if !self.is_open() {
            return;
        }
        // SAFETY: `state` stays valid until Drop; the reference ends before the Win32 calls.
        let (changed, theme) = unsafe {
            let st = &mut *self.state;
            let changed = st.alert.is_some() != on;
            if changed {
                st.alert = on.then(Instant::now);
                st.pulse = on && super::pass_window::animation_on();
            }
            (changed, st.theme)
        };
        if !changed {
            return;
        }
        super::theme::border(self.hwnd, on.then(|| theme::palette(theme).caution));
        // SAFETY: valid window handle on its own thread.
        unsafe {
            if on && (*self.state).pulse {
                let _ = SetTimer(Some(self.hwnd), ALERT_TIMER, ALERT_MS, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), ALERT_TIMER);
            }
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    /// The button that was pressed and released, once one was.
    pub fn answer(&self) -> Option<Button> {
        // SAFETY: `state` stays valid until Drop.
        unsafe { (*self.state).answer }
    }

    /// Brings the window to the front and activates it, right after the click on a button that hands an SSH request on
    /// to 1Password (design `2026-11-22-front-after-allow-design.md` §5.1). This process received that click, the last
    /// input, so Windows lets it set the foreground window although the window never activated itself. Remembers the
    /// window that was in front, to give the front back when this one closes. Returns whether the window is in front
    /// now.
    pub fn come_forward(&self) -> bool {
        if !self.is_open() {
            return false;
        }
        // SAFETY: plain queries and a request about our own window, on its thread. No reference into the state is held
        // across them: activating the window runs its procedure.
        let (before, now) = unsafe {
            let before = GetForegroundWindow();
            if before == self.hwnd {
                return true;
            }
            let _ = SetForegroundWindow(self.hwnd);
            (before, GetForegroundWindow())
        };
        let came = now == self.hwnd;
        if came {
            // SAFETY: `state` stays valid until Drop.
            unsafe { (*self.state).front_before = Some(before) };
        }
        came
    }

    /// The window came to the front ([`Window::come_forward`]).
    pub fn came_forward(&self) -> bool {
        // SAFETY: `state` stays valid until Drop.
        unsafe { (*self.state).front_before.is_some() }
    }

    /// Gives the front back to the window that had it before [`Window::come_forward`] (design
    /// `2026-11-22-front-after-allow-design.md` §5.3): only while this window still has the front or no window has it
    /// (1Password's prompt, or a window the person went to, keeps it), and only to a window that is still there, shown
    /// and not minimized. The foreground process may set the foreground window.
    fn give_back(&self) {
        // SAFETY: `state` stays valid until Drop; the value is taken out before the Win32 calls.
        let Some(before) = (unsafe { (*self.state).front_before.take() }) else {
            return;
        };
        // SAFETY: plain queries and a request to Windows; `before` may be stale, which IsWindow answers.
        unsafe {
            let now = GetForegroundWindow();
            if before.is_invalid() || !(now == self.hwnd || now.is_invalid()) {
                return;
            }
            if IsWindow(Some(before)).as_bool()
                && IsWindowVisible(before).as_bool()
                && !IsIconic(before).as_bool()
            {
                let _ = SetForegroundWindow(before);
            }
        }
    }

    /// Fits the window's height to its layout (the top right corner stays) and redraws it.
    fn fit(&self) {
        // SAFETY: `state` stays valid until Drop; the values are read before the Win32 calls, which re-enter the
        // window procedure.
        let (client_h, dpi) = unsafe { ((*self.state).layout.height, (*self.state).dpi) };
        // SAFETY: valid window handle on its own thread.
        unsafe {
            if let Ok((w, h)) = outer_size(client_h, dpi) {
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    0,
                    0,
                    w,
                    h,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    /// Puts new texts in (the same window: a changed count of waiting calls, the buttons gone after an answer) and lays
    /// the window out again in the language shown; the top right corner stays.
    pub fn set_texts(&self, texts: &Both) {
        if !self.is_open() {
            return;
        }
        // SAFETY: `state` stays valid until Drop; the reference ends before the Win32 calls, which re-enter the window
        // procedure.
        let title = unsafe {
            let st = &mut *self.state;
            let old = std::mem::replace(&mut st.texts, texts.clone());
            let had = st.layout.buttons.len();
            if st.relayout(st.lang).is_err() {
                st.texts = old;
                return;
            }
            // the same buttons (a new count of waiting calls): a press in progress stays; new buttons arm again
            if st.layout.buttons.len() != had {
                st.pressed = None;
                st.buttons_since = Instant::now();
            }
            HSTRING::from(st.texts.get(st.lang).title.as_str())
        };
        // SAFETY: valid window handle on its own thread.
        unsafe {
            let _ = SetWindowTextW(self.hwnd, &title);
        }
        self.fit();
    }

    /// Shows or hides the "other op.exe" line and fits the height.
    pub fn set_others(&self, show: bool) {
        if !self.is_open() {
            return;
        }
        // Read what we need, then call Win32 without holding a reference into the state: SetWindowPos runs the
        // window procedure, which takes its own reference.
        // SAFETY: `state` stays valid until Drop; the reference ends before the calls below.
        let changed = unsafe {
            let st = &mut *self.state;
            if st.show_others == show {
                false
            } else {
                st.show_others = show;
                let lang = st.lang;
                st.relayout(lang).is_ok()
            }
        };
        if changed {
            self.fit();
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // The front goes back first, while this window may still have it.
        self.give_back();
        // SAFETY: destroy the window first (its procedure reads the state), then free the state.
        let opening = unsafe {
            if !(*self.state).destroyed {
                let _ = DestroyWindow(self.hwnd);
            }
            let opening = std::mem::take(&mut (*self.state).opening);
            drop(Box::from_raw(self.state));
            opening
        };
        // A link clicked just before the call ended: give the shell a moment to hand it on.
        let t0 = Instant::now();
        for t in opening {
            while !t.is_finished() && t0.elapsed() < OPENING_WAIT {
                std::thread::sleep(Duration::from_millis(20));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

/// The process of the window in front, 0 when there is none.
pub fn front_pid() -> u32 {
    let mut pid = 0u32;
    // SAFETY: plain queries with an out parameter on the stack; a null window gives no process.
    unsafe {
        let fg = GetForegroundWindow();
        if !fg.is_invalid() {
            GetWindowThreadProcessId(fg, Some(&mut pid));
        }
    }
    pid
}

/// Lets the process `pid` set the foreground window until the next input that is not for it (1Password, to bring its
/// prompt in front of a window that came to the front; design `2026-11-22-front-after-allow-design.md` §5.2). Only the
/// process that may set the foreground window itself can hand that on.
pub fn let_come_forward(pid: u32) {
    // SAFETY: a plain call without pointers.
    unsafe {
        let _ = AllowSetForegroundWindow(pid);
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

/// Whether the mouse pointer is on the switch's other language, a link or a button.
///
/// # Safety
///
/// `ptr` is the window's state.
unsafe fn pointer_on_target(hwnd: HWND, ptr: *mut State) -> bool {
    let mut pt = POINT::default();
    // SAFETY: valid window handle and out parameter.
    let found = unsafe { GetCursorPos(&mut pt).is_ok() && ScreenToClient(hwnd, &mut pt).as_bool() };
    if !found {
        return false;
    }
    // SAFETY: (caller) a valid state; the reference ends in this function.
    let st = unsafe { &*ptr };
    let k = 96.0 / st.dpi.max(1) as f32;
    st.target(pt.x as f32 * k, pt.y as f32 * k).is_some()
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
            st.layout.height,
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
                let hdc = BeginPaint(hwnd, &mut ps);
                let _ = (*ptr).render(hwnd, hdc);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_SIZE => {
                // a DC render target is bound to the client area at each paint
                if let Some(Surface::Hwnd(t)) = (*ptr).target.as_ref() {
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
                        t.rt().SetDpi(dpi as f32, dpi as f32);
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
            // Windows' app mode changed: the frame and the colors follow, the layout stays
            WM_SETTINGCHANGE if super::theme::changed(lparam) => {
                let now = super::theme::current();
                let changed = {
                    let st = &mut *ptr;
                    let changed = st.theme != now;
                    st.theme = now;
                    changed
                };
                if changed {
                    super::theme::frame(hwnd, now);
                    if (*ptr).alert.is_some() {
                        super::theme::border(hwnd, Some(theme::palette(now).caution));
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            // The hand over the switch's other language, links and buttons; elsewhere the class's arrow.
            WM_SETCURSOR
                if (lparam.0 & 0xFFFF) as u32 == HTCLIENT && pointer_on_target(hwnd, ptr) =>
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
                let on_button = {
                    let st = &mut *ptr;
                    let (x, y) = dip_point(lparam, st.dpi);
                    let target = st.target(x, y);
                    let early = matches!(target, Some(Target::Button(_)))
                        && st.buttons_since.elapsed() < BUTTON_ARMING;
                    st.pressed = if early { None } else { target };
                    matches!(st.pressed, Some(Target::Button(_)))
                };
                if on_button {
                    // the release comes here even outside the window, so that only a press and a release on the
                    // same button answers
                    SetCapture(hwnd);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                let was_button = {
                    let st = &mut *ptr;
                    let was = matches!(st.pressed, Some(Target::Button(_)));
                    if was {
                        st.pressed = None;
                    }
                    was
                };
                if was_button {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let (chosen, was_button) = {
                    let st = &mut *ptr;
                    let (x, y) = dip_point(lparam, st.dpi);
                    let pressed = st.pressed.take();
                    (
                        st.target(x, y).filter(|t| pressed == Some(*t)),
                        matches!(pressed, Some(Target::Button(_))),
                    )
                };
                if was_button {
                    let _ = ReleaseCapture();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                match chosen {
                    Some(Target::Switch(lang)) => choose(hwnd, ptr, lang),
                    Some(Target::Link(i)) => {
                        let st = &mut *ptr;
                        if let Some(url) = st.layout.labels.get(i).and_then(|l| l.link.clone())
                            && let Some(t) = super::shell::open_item(&url)
                        {
                            st.opening.push(t);
                        }
                    }
                    Some(Target::Button(i)) => {
                        let st = &mut *ptr;
                        if st.answer.is_none() {
                            st.answer = st.layout.buttons.get(i).map(|b| b.button);
                        }
                    }
                    None => {}
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
            WM_TIMER if wparam.0 == ALERT_TIMER => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == COUNTDOWN_TIMER => {
                if (*ptr).layout.countdown.is_some() {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else {
                    let _ = KillTimer(Some(hwnd), COUNTDOWN_TIMER);
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

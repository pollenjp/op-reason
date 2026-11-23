//! The settings window's controls (design `2026-11-23-settings-design.md` §11.2), made as the author's `type-clipboard`
//! makes its own: Windows' standard controls (comctl32 version 6 from the manifest), the system's message font for the
//! window's DPI, and a layout in pixels at 96 DPI scaled to it.

use std::ffi::c_void;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, GetDC, GetTextExtentPoint32W, HFONT, LOGFONTW, ReleaseDC,
    SelectObject,
};
use windows::Win32::System::SystemServices::{
    SS_CENTERIMAGE, SS_ENDELLIPSIS, SS_LEFT, SS_NOPREFIX, SS_PATHELLIPSIS,
};
use windows::Win32::UI::Controls::{
    LIST_VIEW_ITEM_STATE_FLAGS, LVCF_TEXT, LVCF_WIDTH, LVCOLUMNW, LVIF_TEXT, LVIS_FOCUSED,
    LVIS_SELECTED, LVITEMW, LVM_DELETEALLITEMS, LVM_GETNEXTITEM, LVM_INSERTCOLUMNW,
    LVM_INSERTITEMW, LVM_SETCOLUMNW, LVM_SETCOLUMNWIDTH, LVM_SETEXTENDEDLISTVIEWSTYLE,
    LVM_SETITEMSTATE, LVM_SETITEMTEXTW, LVNI_SELECTED, LVS_EX_DOUBLEBUFFER, LVS_EX_FULLROWSELECT,
    LVS_NOSORTHEADER, LVS_REPORT, LVS_SHOWSELALWAYS, LVS_SINGLESEL, UDM_SETBUDDY, UDM_SETRANGE32,
    UDS_ALIGNRIGHT, UDS_ARROWKEYS, UDS_NOTHOUSANDS, UDS_SETBUDDYINT, UPDOWN_CLASSW, WC_BUTTONW,
    WC_COMBOBOXW, WC_EDITW, WC_LISTVIEWW, WC_STATICW,
};
use windows::Win32::UI::HiDpi::SystemParametersInfoForDpi;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_GROUPBOX, BS_PUSHBUTTON, CB_ADDSTRING,
    CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CBS_DROPDOWNLIST, CreateWindowExW, ES_AUTOHSCROLL,
    ES_NUMBER, GetWindowTextW, HMENU, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS, SWP_NOACTIVATE,
    SWP_NOZORDER, SendMessageW, SetWindowPos, SetWindowTextW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_SETFONT, WS_CHILD, WS_EX_CLIENTEDGE, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

use crate::settings_ui::{Texts, ids};
use op_reason_core::settings::values;

/// The ID of labels and group boxes (they take no input).
const ID_STATIC: u16 = 0xFFFF;
/// The modes, as the settings write them (the same words in both languages).
const MODES: [&str; 2] = ["per-call", "show-only"];

/// A length at 96 DPI in pixels at `dpi`.
pub fn scale(value: i32, dpi: u32) -> i32 {
    (i64::from(value) * i64::from(dpi) / 96) as i32
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub fn set_text(hwnd: HWND, text: &str) {
    // SAFETY: the string outlives the call.
    let _ = unsafe { SetWindowTextW(hwnd, &HSTRING::from(text)) };
}

pub fn text_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: the buffer's length is passed.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0)])
}

pub fn enable(hwnd: HWND, on: bool) {
    // SAFETY: changes the control's state only.
    let _ = unsafe { EnableWindow(hwnd, on) };
}

pub fn is_checked(hwnd: HWND) -> bool {
    // SAFETY: a question to a button.
    unsafe { SendMessageW(hwnd, BM_GETCHECK, None, None) }.0 == 1
}

pub fn set_checked(hwnd: HWND, on: bool) {
    // SAFETY: an order to a button.
    unsafe { SendMessageW(hwnd, BM_SETCHECK, Some(WPARAM(usize::from(on))), None) };
}

/// The chosen item of a combo box.
pub fn chosen(combo: HWND) -> usize {
    // SAFETY: a question to a combo box.
    let i = unsafe { SendMessageW(combo, CB_GETCURSEL, None, None) }.0;
    usize::try_from(i).unwrap_or(0)
}

pub fn choose(combo: HWND, index: usize) {
    // SAFETY: an order to a combo box.
    unsafe { SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(index)), None) };
}

fn fill(combo: HWND, items: &[&str]) {
    // SAFETY: orders to a combo box; each string outlives its call.
    unsafe {
        SendMessageW(combo, CB_RESETCONTENT, None, None);
        for item in items {
            let text = wide(item);
            SendMessageW(
                combo,
                CB_ADDSTRING,
                None,
                Some(LPARAM(text.as_ptr() as isize)),
            );
        }
    }
}

fn place(hwnd: HWND, r: RECT) {
    // SAFETY: moves a child window.
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            r.left,
            r.top,
            (r.right - r.left).max(0),
            (r.bottom - r.top).max(0),
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

fn child(
    parent: HWND,
    instance: HINSTANCE,
    class: PCWSTR,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    id: u16,
) -> windows::core::Result<HWND> {
    // SAFETY: a child window with an empty text; its ID goes in the HMENU argument.
    unsafe {
        CreateWindowExW(
            ex_style,
            class,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(usize::from(id) as *mut c_void)),
            Some(instance),
            None,
        )
    }
}

/// The window's controls, in their tab order.
pub struct Controls {
    pub lang_group: HWND,
    pub lang_label: HWND,
    pub lang: HWND,
    pub relay_group: HWND,
    pub relay_line: HWND,
    pub start: HWND,
    pub stop: HWND,
    pub restart: HWND,
    pub autostart: HWND,
    pub tray_icon: HWND,
    pub mode_label: HWND,
    pub mode: HWND,
    pub time_label: HWND,
    pub time_limit: HWND,
    pub time_spin: HWND,
    pub wait_label: HWND,
    pub answer_wait: HWND,
    pub wait_spin: HWND,
    pub apply_note: HWND,
    pub apply: HWND,
    pub skill_group: HWND,
    pub skills: HWND,
    pub place: HWND,
    pub remove: HWND,
    pub check: HWND,
    pub add_folder: HWND,
    pub other_group: HWND,
    pub start_menu: HWND,
    pub records_label: HWND,
    pub records: HWND,
    pub open: HWND,
    pub accounts: HWND,
    pub forget: HWND,
    pub status: HWND,
    pub refresh: HWND,
    pub close: HWND,
}

/// The measures of a layout at one DPI.
pub struct Metrics {
    pub dpi: u32,
    pub font: HFONT,
    pub char_height: i32,
}

impl Controls {
    pub fn create(parent: HWND, instance: HINSTANCE) -> windows::core::Result<Controls> {
        let make = |class, style, ex_style, id| child(parent, instance, class, style, ex_style, id);
        let none = WINDOW_EX_STYLE::default();
        let label = WINDOW_STYLE(SS_LEFT.0 | SS_NOPREFIX.0 | SS_CENTERIMAGE.0);
        let line = WINDOW_STYLE(SS_LEFT.0 | SS_NOPREFIX.0 | SS_ENDELLIPSIS.0 | SS_CENTERIMAGE.0);
        let lines = WINDOW_STYLE(SS_LEFT.0 | SS_NOPREFIX.0);
        let group = WINDOW_STYLE(BS_GROUPBOX as u32);
        let button = WINDOW_STYLE(BS_PUSHBUTTON as u32) | WS_TABSTOP;
        let check = WINDOW_STYLE(BS_AUTOCHECKBOX as u32) | WS_TABSTOP;
        let combo = WINDOW_STYLE(CBS_DROPDOWNLIST as u32) | WS_TABSTOP | WS_VSCROLL;
        let number = WINDOW_STYLE((ES_NUMBER | ES_AUTOHSCROLL) as u32) | WS_TABSTOP;
        let spin = WINDOW_STYLE(UDS_SETBUDDYINT | UDS_ALIGNRIGHT | UDS_ARROWKEYS | UDS_NOTHOUSANDS);
        let list = WINDOW_STYLE(LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | LVS_NOSORTHEADER)
            | WS_TABSTOP;
        let c = Controls {
            lang_group: make(WC_BUTTONW, group, none, ID_STATIC)?,
            lang_label: make(WC_STATICW, label, none, ID_STATIC)?,
            lang: make(WC_COMBOBOXW, combo, none, ids::LANG)?,
            relay_group: make(WC_BUTTONW, group, none, ID_STATIC)?,
            relay_line: make(WC_STATICW, lines, none, ids::RELAY_LINE)?,
            start: make(WC_BUTTONW, button, none, ids::START)?,
            stop: make(WC_BUTTONW, button, none, ids::STOP)?,
            restart: make(WC_BUTTONW, button, none, ids::RESTART)?,
            autostart: make(WC_BUTTONW, check, none, ids::AUTOSTART)?,
            tray_icon: make(WC_BUTTONW, check, none, ids::TRAY_ICON)?,
            mode_label: make(WC_STATICW, label, none, ID_STATIC)?,
            mode: make(WC_COMBOBOXW, combo, none, ids::MODE)?,
            time_label: make(WC_STATICW, label, none, ID_STATIC)?,
            time_limit: make(WC_EDITW, number, WS_EX_CLIENTEDGE, ids::TIME_LIMIT)?,
            time_spin: make(UPDOWN_CLASSW, spin, none, ids::TIME_LIMIT_SPIN)?,
            wait_label: make(WC_STATICW, label, none, ID_STATIC)?,
            answer_wait: make(WC_EDITW, number, WS_EX_CLIENTEDGE, ids::ANSWER_WAIT)?,
            wait_spin: make(UPDOWN_CLASSW, spin, none, ids::ANSWER_WAIT_SPIN)?,
            apply_note: make(WC_STATICW, line, none, ids::APPLY_NOTE)?,
            apply: make(WC_BUTTONW, button, none, ids::APPLY)?,
            skill_group: make(WC_BUTTONW, group, none, ID_STATIC)?,
            skills: make(WC_LISTVIEWW, list, WS_EX_CLIENTEDGE, ids::SKILLS)?,
            place: make(WC_BUTTONW, button, none, ids::PLACE)?,
            remove: make(WC_BUTTONW, button, none, ids::REMOVE)?,
            check: make(WC_BUTTONW, button, none, ids::CHECK)?,
            add_folder: make(WC_BUTTONW, button, none, ids::ADD_FOLDER)?,
            other_group: make(WC_BUTTONW, group, none, ID_STATIC)?,
            start_menu: make(WC_BUTTONW, check, none, ids::START_MENU)?,
            records_label: make(WC_STATICW, label, none, ID_STATIC)?,
            records: make(
                WC_STATICW,
                WINDOW_STYLE(SS_LEFT.0 | SS_NOPREFIX.0 | SS_PATHELLIPSIS.0 | SS_CENTERIMAGE.0),
                none,
                ids::RECORDS,
            )?,
            open: make(WC_BUTTONW, button, none, ids::OPEN_RECORDS)?,
            accounts: make(WC_STATICW, line, none, ids::ACCOUNTS)?,
            forget: make(WC_BUTTONW, button, none, ids::FORGET)?,
            status: make(WC_STATICW, lines, none, ids::STATUS)?,
            refresh: make(WC_BUTTONW, button, none, ids::REFRESH)?,
            close: make(WC_BUTTONW, button, none, ids::CLOSE)?,
        };
        fill(c.mode, &MODES);
        c.init_list();
        let max_minutes = (values::MAX_TIME_LIMIT / 60) as isize;
        let max_seconds = values::MAX_ANSWER_WAIT as isize;
        // SAFETY: orders to the up-downs (their range).
        unsafe {
            SendMessageW(
                c.time_spin,
                UDM_SETRANGE32,
                Some(WPARAM(1)),
                Some(LPARAM(max_minutes)),
            );
            SendMessageW(
                c.wait_spin,
                UDM_SETRANGE32,
                Some(WPARAM(1)),
                Some(LPARAM(max_seconds)),
            );
        }
        Ok(c)
    }

    fn init_list(&self) {
        let extended = (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize;
        // SAFETY: orders to the list view; the column texts are set by `set_texts`.
        unsafe {
            SendMessageW(
                self.skills,
                LVM_SETEXTENDEDLISTVIEWSTYLE,
                Some(WPARAM(extended as usize)),
                Some(LPARAM(extended)),
            );
            for index in 0..3usize {
                let mut text = wide("");
                let column = LVCOLUMNW {
                    mask: LVCF_TEXT | LVCF_WIDTH,
                    cx: 100,
                    pszText: PWSTR(text.as_mut_ptr()),
                    ..Default::default()
                };
                SendMessageW(
                    self.skills,
                    LVM_INSERTCOLUMNW,
                    Some(WPARAM(index)),
                    Some(LPARAM(&column as *const LVCOLUMNW as isize)),
                );
            }
        }
    }

    /// The texts of one language, everywhere they are fixed.
    pub fn set_texts(&self, t: &Texts) {
        for (hwnd, text) in [
            (self.lang_group, t.language),
            (self.lang_label, t.language),
            (self.relay_group, t.relay),
            (self.start, t.start),
            (self.stop, t.stop),
            (self.restart, t.restart),
            (self.autostart, t.autostart),
            (self.tray_icon, t.tray_icon),
            (self.mode_label, t.mode),
            (self.time_label, t.time_limit),
            (self.wait_label, t.answer_wait),
            (self.apply, t.apply),
            (self.skill_group, t.skill),
            (self.place, t.place_or_update),
            (self.remove, t.remove),
            (self.check, t.check),
            (self.add_folder, t.add_folder),
            (self.other_group, t.start_menu_group),
            (self.start_menu, t.start_menu),
            (self.records_label, t.records),
            (self.open, t.open),
            (self.forget, t.forget),
            (self.refresh, t.refresh),
            (self.close, t.close),
        ] {
            set_text(hwnd, text);
        }
        let index = chosen(self.lang);
        fill(self.lang, &t.lang_choices);
        choose(self.lang, index);
        for (index, name) in t.columns.iter().enumerate() {
            let mut text = wide(name);
            let column = LVCOLUMNW {
                mask: LVCF_TEXT,
                pszText: PWSTR(text.as_mut_ptr()),
                ..Default::default()
            };
            // SAFETY: the text outlives the call.
            unsafe {
                SendMessageW(
                    self.skills,
                    LVM_SETCOLUMNW,
                    Some(WPARAM(index)),
                    Some(LPARAM(&column as *const LVCOLUMNW as isize)),
                )
            };
        }
    }

    fn all(&self) -> [HWND; 36] {
        [
            self.lang_group,
            self.lang_label,
            self.lang,
            self.relay_group,
            self.relay_line,
            self.start,
            self.stop,
            self.restart,
            self.autostart,
            self.tray_icon,
            self.mode_label,
            self.mode,
            self.time_label,
            self.time_limit,
            self.time_spin,
            self.wait_label,
            self.answer_wait,
            self.wait_spin,
            self.apply_note,
            self.apply,
            self.skill_group,
            self.skills,
            self.place,
            self.remove,
            self.check,
            self.add_folder,
            self.other_group,
            self.start_menu,
            self.records_label,
            self.records,
            self.open,
            self.accounts,
            self.forget,
            self.status,
            self.refresh,
            self.close,
        ]
    }

    /// The controls that act (all but Close, which always may).
    pub fn inputs(&self) -> [HWND; 21] {
        [
            self.lang,
            self.start,
            self.stop,
            self.restart,
            self.autostart,
            self.tray_icon,
            self.mode,
            self.time_limit,
            self.time_spin,
            self.answer_wait,
            self.wait_spin,
            self.apply,
            self.skills,
            self.place,
            self.remove,
            self.check,
            self.add_folder,
            self.start_menu,
            self.open,
            self.forget,
            self.refresh,
        ]
    }

    pub fn set_font(&self, font: HFONT) {
        for hwnd in self.all() {
            // SAFETY: the font lives in `Fonts` until the controls have a new one.
            unsafe {
                SendMessageW(
                    hwnd,
                    WM_SETFONT,
                    Some(WPARAM(font.0 as usize)),
                    Some(LPARAM(1)),
                )
            };
        }
    }

    /// Places the controls in a client area `width` × `height`: the skill list takes the height that is left.
    pub fn arrange(&self, width: i32, height: i32, m: &Metrics) {
        let rest = self.layout(width, 0, m, false);
        let list = (height - rest).max(self.list_rows(m, 2));
        self.layout(width, list, m, true);
    }

    /// The smallest client height: a skill list of two rows.
    pub fn arrange_need(&self, width: i32, m: &Metrics) -> i32 {
        self.layout(width, 0, m, false) + self.list_rows(m, 2)
    }

    /// The client height the window opens with: a skill list of six rows.
    pub fn preferred_height(&self, width: i32, m: &Metrics) -> i32 {
        self.layout(width, 0, m, false) + self.list_rows(m, 6)
    }

    /// The height of the skill list with its header and `rows` rows.
    fn list_rows(&self, m: &Metrics, rows: i32) -> i32 {
        let s = |v| scale(v, m.dpi);
        m.char_height + s(12) + (m.char_height + s(10)) * rows + s(4)
    }

    /// The width a button needs for its text.
    fn button_width(&self, hwnd: HWND, m: &Metrics) -> i32 {
        (text_width(m.font, &text_of(hwnd)) + scale(24, m.dpi)).max(scale(84, m.dpi))
    }

    fn label_width(&self, hwnd: HWND, m: &Metrics) -> i32 {
        text_width(m.font, &text_of(hwnd)) + scale(4, m.dpi)
    }

    /// The layout with a skill list `list` high; places the controls when `apply`. Returns the height without the
    /// list.
    fn layout(&self, width: i32, list: i32, m: &Metrics, apply: bool) -> i32 {
        let s = |v| scale(v, m.dpi);
        let put = |hwnd: HWND, r: RECT| {
            if apply {
                place(hwnd, r);
            }
        };
        let margin = s(12);
        let gap = s(8);
        let pad = s(10);
        let line = (m.char_height + s(8)).max(s(20));
        let button = (m.char_height + s(14)).max(s(26));
        let caption = m.char_height + s(6);
        let x = margin;
        let w = (width - margin * 2).max(s(420));
        let ix = x + pad;
        let iw = w - pad * 2;
        let mut y = margin;

        // Language
        let top = y;
        y += caption;
        let lw = self.label_width(self.lang_label, m);
        put(self.lang_label, rect(ix, y, lw, button));
        put(
            self.lang,
            rect(ix + lw + gap, y, s(280).min(iw - lw - gap), button + s(160)),
        );
        y += button + pad;
        put(self.lang_group, rect(x, top, w, y - top));
        y += gap;

        // SSH relay
        let top = y;
        y += caption;
        put(self.relay_line, rect(ix, y, iw, line * 2));
        y += line * 2;
        let mut bx = ix;
        for b in [self.start, self.stop, self.restart] {
            let bw = self.button_width(b, m);
            put(b, rect(bx, y, bw, button));
            bx += bw + gap;
        }
        y += button + gap;
        put(self.autostart, rect(ix, y, iw, line));
        y += line;
        put(self.tray_icon, rect(ix, y, iw, line));
        y += line + gap;
        let (ml, tl, wl) = (
            self.label_width(self.mode_label, m),
            self.label_width(self.time_label, m),
            self.label_width(self.wait_label, m),
        );
        let (mw, ew) = (s(120), s(76));
        let one_row = ml + mw + tl + ew + wl + ew + gap * 3 + s(16) * 2 <= iw;
        put(self.mode_label, rect(ix, y, ml, button));
        put(self.mode, rect(ix + ml + gap, y, mw, button + s(80)));
        let (mut tx, mut ty) = (ix + ml + mw + gap + s(16), y);
        if !one_row {
            y += button + gap;
            (tx, ty) = (ix, y);
        }
        put(self.time_label, rect(tx, ty, tl, button));
        put(self.time_limit, rect(tx + tl + gap, ty, ew, button));
        let wx = tx + tl + gap + ew + s(16);
        put(self.wait_label, rect(wx, ty, wl, button));
        put(self.answer_wait, rect(wx + wl + gap, ty, ew, button));
        if apply {
            // the up-downs follow their fields
            // SAFETY: orders to the up-downs.
            unsafe {
                SendMessageW(
                    self.time_spin,
                    UDM_SETBUDDY,
                    Some(WPARAM(self.time_limit.0 as usize)),
                    None,
                );
                SendMessageW(
                    self.wait_spin,
                    UDM_SETBUDDY,
                    Some(WPARAM(self.answer_wait.0 as usize)),
                    None,
                );
            }
        }
        y += button + gap;
        let aw = self.button_width(self.apply, m);
        put(self.apply_note, rect(ix, y, iw - aw - gap, button));
        put(self.apply, rect(ix + iw - aw, y, aw, button));
        y += button + pad;
        put(self.relay_group, rect(x, top, w, y - top));
        y += gap;

        // Skill
        let top = y;
        y += caption;
        put(self.skills, rect(ix, y, iw, list));
        if apply {
            let first = iw * 30 / 100;
            let second = iw * 30 / 100;
            for (index, cx) in [first, second, iw - first - second - s(24)]
                .into_iter()
                .enumerate()
            {
                // SAFETY: an order to the list view.
                unsafe {
                    SendMessageW(
                        self.skills,
                        LVM_SETCOLUMNWIDTH,
                        Some(WPARAM(index)),
                        Some(LPARAM(cx.max(s(60)) as isize)),
                    )
                };
            }
        }
        y += list + gap;
        let mut bx = ix;
        for b in [self.place, self.remove, self.check, self.add_folder] {
            let bw = self.button_width(b, m);
            put(b, rect(bx, y, bw, button));
            bx += bw + gap;
        }
        y += button + pad;
        put(self.skill_group, rect(x, top, w, y - top));
        y += gap;

        // Start menu and records
        let top = y;
        y += caption;
        put(self.start_menu, rect(ix, y, iw, line));
        y += line + s(4);
        let rl = self.label_width(self.records_label, m);
        let ow = self.button_width(self.open, m);
        put(self.records_label, rect(ix, y, rl, button));
        put(
            self.records,
            rect(ix + rl + gap, y, iw - rl - ow - gap * 2, button),
        );
        put(self.open, rect(ix + iw - ow, y, ow, button));
        y += button + s(4);
        let fw = self.button_width(self.forget, m);
        put(self.accounts, rect(ix, y, iw - fw - gap, button));
        put(self.forget, rect(ix + iw - fw, y, fw, button));
        y += button + pad;
        put(self.other_group, rect(x, top, w, y - top));
        y += gap;

        // What the last action did, with Refresh and Close at its right
        let cw = self.button_width(self.close, m);
        let rw = self.button_width(self.refresh, m);
        let row = (line * 2).max(button);
        put(self.status, rect(x, y, w - cw - rw - gap * 2, row));
        put(
            self.refresh,
            rect(x + w - cw - gap - rw, y + row - button, rw, button),
        );
        put(self.close, rect(x + w - cw, y + row - button, cw, button));
        y += row + margin;
        y - list
    }
}

/// The width of `text` in `font`, in pixels.
fn text_width(font: HFONT, text: &str) -> i32 {
    let units: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: a screen DC borrowed and released here; the font is selected back out before release.
    unsafe {
        let dc = GetDC(None);
        let old = SelectObject(dc, font.into());
        let mut size = windows::Win32::Foundation::SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &units, &mut size);
        SelectObject(dc, old);
        ReleaseDC(None, dc);
        size.cx
    }
}

/// The rows of the skill list.
pub mod list {
    use super::*;

    pub fn clear(list: HWND) {
        // SAFETY: an order to the list view.
        unsafe { SendMessageW(list, LVM_DELETEALLITEMS, None, None) };
    }

    pub fn insert(list: HWND, index: usize, cells: [&str; 3]) {
        let mut texts: Vec<Vec<u16>> = cells.iter().map(|c| wide(c)).collect();
        let mut item = LVITEMW {
            mask: LVIF_TEXT,
            iItem: index as i32,
            pszText: PWSTR(texts[0].as_mut_ptr()),
            ..Default::default()
        };
        // SAFETY: the texts outlive the calls.
        unsafe {
            let row = SendMessageW(
                list,
                LVM_INSERTITEMW,
                None,
                Some(LPARAM(&item as *const LVITEMW as isize)),
            )
            .0;
            for (sub, text) in texts.iter_mut().enumerate().skip(1) {
                item.iSubItem = sub as i32;
                item.pszText = PWSTR(text.as_mut_ptr());
                SendMessageW(
                    list,
                    LVM_SETITEMTEXTW,
                    Some(WPARAM(row as usize)),
                    Some(LPARAM(&item as *const LVITEMW as isize)),
                );
            }
        }
    }

    /// The chosen row.
    pub fn selected(list: HWND) -> Option<usize> {
        // SAFETY: a question to the list view (-1: from the start).
        let row = unsafe {
            SendMessageW(
                list,
                LVM_GETNEXTITEM,
                Some(WPARAM(usize::MAX)),
                Some(LPARAM(LVNI_SELECTED as isize)),
            )
        }
        .0;
        usize::try_from(row).ok()
    }

    pub fn select(list: HWND, index: usize) {
        let mask = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
        let item = LVITEMW {
            stateMask: mask,
            state: mask,
            ..Default::default()
        };
        // SAFETY: the item outlives the call.
        unsafe {
            SendMessageW(
                list,
                LVM_SETITEMSTATE,
                Some(WPARAM(index)),
                Some(LPARAM(&item as *const LVITEMW as isize)),
            )
        };
    }
}

/// The system's message font at one DPI.
pub struct Fonts {
    pub normal: HFONT,
    pub char_height: i32,
}

impl Fonts {
    pub fn new(dpi: u32) -> Fonts {
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        // SAFETY: the structure's size is passed in cbSize.
        let ok = unsafe {
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS.0,
                metrics.cbSize,
                Some(&mut metrics as *mut NONCLIENTMETRICSW as *mut c_void),
                0,
                dpi,
            )
        }
        .is_ok();
        let font = if ok {
            metrics.lfMessageFont
        } else {
            let mut font = LOGFONTW {
                lfHeight: -scale(12, dpi),
                ..Default::default()
            };
            for (dst, src) in font.lfFaceName.iter_mut().zip("Segoe UI".encode_utf16()) {
                *dst = src;
            }
            font
        };
        // SAFETY: a valid LOGFONTW.
        let normal = unsafe { CreateFontIndirectW(&font) };
        Fonts {
            normal,
            char_height: font.lfHeight.abs().max(scale(12, dpi)),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        // SAFETY: the controls were given a new font before this one goes.
        unsafe {
            let _ = DeleteObject(self.normal.into());
        }
    }
}

//! The two looks of `op-reason.exe`'s windows and toasts (design `2026-11-26-modern-window-design.md` §4.4, §4.14): dark
//! and light, as Windows' app mode says. The colors of each look are the roles of the window (background, text, the
//! accent, the caution and critical colors, the band and its picture …); the window and the toasts take every color
//! from here.

/// Windows' app mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    /// From `AppsUseLightTheme` (`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`): 0 is dark;
    /// anything else, a missing value or one that cannot be read is light, Windows' default.
    pub fn from_apps_use_light_theme(value: Option<u32>) -> Theme {
        match value {
            Some(0) => Theme::Dark,
            _ => Theme::Light,
        }
    }
}

/// A color, eight bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `0xRRGGBB`.
    pub const fn hex(v: u32) -> Rgb {
        Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    /// The relative luminance of WCAG 2.x.
    pub fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.0) + 0.7152 * lin(self.1) + 0.0722 * lin(self.2)
    }
}

/// The contrast ratio of two colors (WCAG 2.x): 1 to 21.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// The colors of one look, by role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// The window and its caption.
    pub background: Rgb,
    /// The title, the reason, chips, the purpose.
    pub text: Rgb,
    /// Labels, the subtitle, the time, notes, "and N more".
    pub secondary: Rgb,
    /// The date, the small facts, the root line (on the background only).
    pub tertiary: Rgb,
    /// Chips, the switch, the toasts.
    pub surface: Rgb,
    /// The borders of chips, the switch and the toasts; the bar's track.
    pub line: Rgb,
    pub code_bg: Rgb,
    pub code_border: Rgb,
    /// The command's words.
    pub code_text: Rgb,
    /// File names, paths and references in the command; the references' squares.
    pub code_path: Rgb,
    /// The tile of an `op` call, the program in the command, links, Allow, "the middle of the screen".
    pub accent: Rgb,
    /// On the accent: the text of Allow, the lock.
    pub on_accent: Rgb,
    /// The relay's tile, "no reason given", the bar, "the tray", the notes' mark.
    pub caution: Rgb,
    /// On the caution color: the key, the "!" of a note.
    pub on_caution: Rgb,
    /// The footer, Deny.
    pub critical: Rgb,
    /// The line while other `op.exe` run.
    pub orange: Rgb,
    /// A person's dot; the toasts' "Passed", check, bar and ring.
    pub success: Rgb,
    /// The check on the success color.
    pub on_success: Rgb,
    /// The toasts' ×N tag.
    pub success_tint: Rgb,
    /// Claude Code's dot.
    pub claude: Rgb,
    pub band_bg: Rgb,
    pub band_border: Rgb,
    pub band_title: Rgb,
    pub band_text: Rgb,
    /// The picture in the band: the screen, its frame, the taskbar, the stand, op-reason's window.
    pub pic_screen: Rgb,
    pub pic_frame: Rgb,
    pub pic_taskbar: Rgb,
    pub pic_stand: Rgb,
    pub pic_window: Rgb,
    /// The part of the switch of the language shown.
    pub switch_shown: Rgb,
    /// The band when 1Password's prompt did not come, and the pointer to the tray (design
    /// `2026-11-26-prompt-guide-design.md` §6): its background; its title is in the caution color, its line in the text
    /// color.
    pub alert_bg: Rgb,
}

/// The dark look: the approved drafts, close to Windows 11's dark colors.
pub const DARK: Palette = Palette {
    background: Rgb::hex(0x202020),
    text: Rgb::hex(0xffffff),
    secondary: Rgb::hex(0xa0a0a0),
    tertiary: Rgb::hex(0x8a8a8a),
    surface: Rgb::hex(0x2b2b2b),
    line: Rgb::hex(0x3c3c3c),
    code_bg: Rgb::hex(0x141414),
    code_border: Rgb::hex(0x333333),
    code_text: Rgb::hex(0xd4d4d4),
    code_path: Rgb::hex(0xe5b07b),
    accent: Rgb::hex(0x60cdff),
    on_accent: Rgb::hex(0x000000),
    caution: Rgb::hex(0xfce100),
    on_caution: Rgb::hex(0x202020),
    critical: Rgb::hex(0xff99a4),
    orange: Rgb::hex(0xffb35c),
    success: Rgb::hex(0x6ccb5f),
    on_success: Rgb::hex(0x000000),
    success_tint: Rgb::hex(0x1f3a1c),
    claude: Rgb::hex(0xc3a6ff),
    band_bg: Rgb::hex(0x1b2a36),
    band_border: Rgb::hex(0x2d4b63),
    band_title: Rgb::hex(0xe6f3fb),
    band_text: Rgb::hex(0x9cc9e8),
    pic_screen: Rgb::hex(0x15202a),
    pic_frame: Rgb::hex(0x9cc9e8),
    pic_taskbar: Rgb::hex(0x2d4b63),
    pic_stand: Rgb::hex(0x4a6b85),
    pic_window: Rgb::hex(0x3c3c3c),
    switch_shown: Rgb::hex(0x454545),
    alert_bg: Rgb::hex(0x332b00),
};

/// The light look: the same roles in Windows 11's light colors.
pub const LIGHT: Palette = Palette {
    background: Rgb::hex(0xf9f9f9),
    text: Rgb::hex(0x1a1a1a),
    secondary: Rgb::hex(0x5d5d5d),
    tertiary: Rgb::hex(0x707070),
    surface: Rgb::hex(0xffffff),
    line: Rgb::hex(0xe0e0e0),
    code_bg: Rgb::hex(0xf0f0f0),
    code_border: Rgb::hex(0xdcdcdc),
    code_text: Rgb::hex(0x24292f),
    code_path: Rgb::hex(0x8a4b00),
    accent: Rgb::hex(0x005fb8),
    on_accent: Rgb::hex(0xffffff),
    caution: Rgb::hex(0x9d5d00),
    on_caution: Rgb::hex(0xffffff),
    critical: Rgb::hex(0xc42b1c),
    orange: Rgb::hex(0xb35900),
    success: Rgb::hex(0x0f7b0f),
    on_success: Rgb::hex(0xffffff),
    success_tint: Rgb::hex(0xdff6dd),
    claude: Rgb::hex(0x7e57c2),
    band_bg: Rgb::hex(0xeaf3fb),
    band_border: Rgb::hex(0xb3d3ee),
    band_title: Rgb::hex(0x0b3550),
    band_text: Rgb::hex(0x2d5f86),
    pic_screen: Rgb::hex(0xffffff),
    pic_frame: Rgb::hex(0x2d5f86),
    pic_taskbar: Rgb::hex(0xb3d3ee),
    pic_stand: Rgb::hex(0x8fb3d1),
    pic_window: Rgb::hex(0xd0d0d0),
    switch_shown: Rgb::hex(0xe6e6e6),
    alert_bg: Rgb::hex(0xfff4ce),
};

/// The palette of a look.
pub fn palette(theme: Theme) -> &'static Palette {
    match theme {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

/// A text color and the background it is drawn on.
type Pair = fn(&Palette) -> (Rgb, Rgb);

/// Every text color of the windows and the toasts on its background: each must meet 4.5:1 (WCAG AA) in both looks.
pub const TEXT_PAIRS: &[(&str, Pair)] = &[
    ("text on the background", |p| (p.text, p.background)),
    ("secondary on the background", |p| {
        (p.secondary, p.background)
    }),
    ("tertiary on the background", |p| (p.tertiary, p.background)),
    ("text on the surface (chips, the toasts)", |p| {
        (p.text, p.surface)
    }),
    ("secondary on the surface (the toasts, the switch)", |p| {
        (p.secondary, p.surface)
    }),
    ("text on the switch's shown part", |p| {
        (p.text, p.switch_shown)
    }),
    ("the command's words", |p| (p.code_text, p.code_bg)),
    ("the command's paths", |p| (p.code_path, p.code_bg)),
    ("the command's program", |p| (p.accent, p.code_bg)),
    ("links on the background", |p| (p.accent, p.background)),
    ("no reason given on the background", |p| {
        (p.caution, p.background)
    }),
    ("the footer on the background", |p| {
        (p.critical, p.background)
    }),
    ("the orange line on the background", |p| {
        (p.orange, p.background)
    }),
    ("Allow's label", |p| (p.on_accent, p.accent)),
    ("the terminal button's label", |p| (p.accent, p.surface)),
    ("Deny's label", |p| (p.critical, p.surface)),
    ("the band's title", |p| (p.band_title, p.band_bg)),
    ("the band's line", |p| (p.band_text, p.band_bg)),
    ("the middle of the screen in the band", |p| {
        (p.accent, p.band_bg)
    }),
    ("the tray in the band", |p| (p.caution, p.band_bg)),
    ("Passed on a toast", |p| (p.success, p.surface)),
    ("the ×N tag", |p| (p.success, p.success_tint)),
    ("the alert band's title", |p| (p.caution, p.alert_bg)),
    ("the alert band's line", |p| (p.text, p.alert_bg)),
    ("the pointer's line", |p| (p.secondary, p.surface)),
    ("the pointer's seconds", |p| (p.caution, p.surface)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_from_the_registry_value() {
        assert_eq!(Theme::from_apps_use_light_theme(Some(0)), Theme::Dark);
        assert_eq!(Theme::from_apps_use_light_theme(Some(1)), Theme::Light);
        assert_eq!(
            Theme::from_apps_use_light_theme(Some(2)),
            Theme::Light,
            "any other value: Windows' default"
        );
        assert_eq!(
            Theme::from_apps_use_light_theme(None),
            Theme::Light,
            "missing or unreadable: Windows' default"
        );
    }

    #[test]
    fn every_text_color_meets_aa_on_its_background() {
        for theme in [Theme::Dark, Theme::Light] {
            let p = palette(theme);
            for (name, pair) in TEXT_PAIRS {
                let (fg, bg) = pair(p);
                let c = contrast(fg, bg);
                assert!(c >= 4.5, "{theme:?}: {name} is {c:.2}:1 ({fg:?} on {bg:?})");
            }
        }
    }

    #[test]
    fn the_spec_colors() {
        assert_eq!(DARK.background, Rgb(0x20, 0x20, 0x20));
        assert_eq!(DARK.accent, Rgb(0x60, 0xcd, 0xff));
        assert_eq!(DARK.caution, Rgb(0xfc, 0xe1, 0x00));
        assert_eq!(DARK.critical, Rgb(0xff, 0x99, 0xa4));
        assert_eq!(LIGHT.background, Rgb(0xf9, 0xf9, 0xf9));
        assert_eq!(LIGHT.accent, Rgb(0x00, 0x5f, 0xb8));
        assert_eq!(LIGHT.caution, Rgb(0x9d, 0x5d, 0x00));
        assert_eq!(LIGHT.critical, Rgb(0xc4, 0x2b, 0x1c));
        assert_eq!(palette(Theme::Dark), &DARK);
        assert_eq!(palette(Theme::Light), &LIGHT);
    }

    #[test]
    fn contrast_of_black_and_white() {
        let c = contrast(Rgb::hex(0x000000), Rgb::hex(0xffffff));
        assert!((c - 21.0).abs() < 1e-9, "{c}");
        assert!((contrast(Rgb::hex(0x777777), Rgb::hex(0x777777)) - 1.0).abs() < 1e-9);
    }
}

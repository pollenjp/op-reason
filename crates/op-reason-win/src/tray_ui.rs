//! The portable part of the relay's icon in the notification area (design `2026-11-23-settings-design.md` §12): its
//! tooltip and menu in English and Japanese, what each item runs, and the icon itself, drawn at run time (the exe has
//! no icon resource).

use op_reason_core::lang::Lang;

/// An item of the icon's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// Open op-reason settings (also a click on the icon).
    Open,
    /// Open the records folder.
    Records,
    /// Restart the relay.
    Restart,
    /// Stop the relay (until it is started again).
    Stop,
    /// Hide this icon (`relay.tray-icon off`).
    Hide,
}

impl Item {
    /// Every item, in the menu's order.
    pub const ALL: [Item; 5] = [
        Item::Open,
        Item::Records,
        Item::Restart,
        Item::Stop,
        Item::Hide,
    ];

    /// The menu command's ID (also what a `WM_COMMAND` to the icon's window runs).
    pub fn id(self) -> u16 {
        match self {
            Item::Open => 11,
            Item::Records => 12,
            Item::Restart => 13,
            Item::Stop => 14,
            Item::Hide => 15,
        }
    }

    pub fn from_id(id: u16) -> Option<Item> {
        Item::ALL.into_iter().find(|i| i.id() == id)
    }

    /// The arguments of `op-reason.exe` that the item runs, in a process of its own.
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Item::Open => &["--settings"],
            Item::Records => &["--settings", "open-records"],
            Item::Restart => &["--settings", "relay", "restart"],
            Item::Stop => &["--settings", "relay", "stop"],
            Item::Hide => &["--settings", "set", "relay.tray-icon", "off"],
        }
    }

    /// Stop and Restart act only on the relay of op-reason's task.
    pub fn needs_ours(self) -> bool {
        matches!(self, Item::Restart | Item::Stop)
    }

    /// The item's text.
    pub fn text(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Item::Open, Lang::En) => "Open op-reason settings",
            (Item::Open, Lang::Ja) => "op-reason の設定を開く",
            (Item::Records, Lang::En) => "Open the records folder",
            (Item::Records, Lang::Ja) => "記録のフォルダを開く",
            (Item::Restart, Lang::En) => "Restart the relay",
            (Item::Restart, Lang::Ja) => "中継を起こし直す",
            (Item::Stop, Lang::En) => "Stop the relay (until it is started again)",
            (Item::Stop, Lang::Ja) => "中継を止める (起こすまで)",
            (Item::Hide, Lang::En) => "Hide this icon",
            (Item::Hide, Lang::Ja) => "このアイコンを隠す",
        }
    }
}

/// The icon's tooltip.
pub fn tip(lang: Lang, mode: &str) -> String {
    match lang {
        Lang::En => format!("op-reason SSH relay · {mode}"),
        Lang::Ja => format!("op-reason の SSH の中継 · {mode}"),
    }
}

/// The menu's first line (greyed): the relay, its mode, and who started it.
pub fn line(lang: Lang, mode: &str, pid: u32, ours: bool) -> String {
    match (lang, ours) {
        (Lang::En, true) => format!("SSH relay · {mode} · pid {pid} · started by op-reason's task"),
        (Lang::En, false) => {
            format!("SSH relay · {mode} · pid {pid} · started outside op-reason's task")
        }
        (Lang::Ja, true) => {
            format!("SSH の中継 · {mode} · pid {pid} · op-reason のタスクが起こしたもの")
        }
        (Lang::Ja, false) => {
            format!("SSH の中継 · {mode} · pid {pid} · op-reason のタスクの外で起こしたもの")
        }
    }
}

/// The icon's blue (the background of the key).
const BLUE: [u8; 3] = [0x25, 0x63, 0xEB];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
/// Samples per pixel on each axis (anti-aliasing).
const SAMPLES: u32 = 4;

/// What covers the point (`x`, `y`) of a square of side 1: nothing, the blue rounded square, or the white key.
fn paint_at(x: f32, y: f32) -> Option<[u8; 3]> {
    let r = 0.22;
    let (cx, cy) = (x.clamp(r, 1.0 - r), y.clamp(r, 1.0 - r));
    if (x - cx).powi(2) + (y - cy).powi(2) > r * r {
        return None;
    }
    // the key: a ring (its bow), a shaft to the right, and two teeth below it
    let ring = (x - 0.34).powi(2) + (y - 0.5).powi(2);
    let bow = ring <= 0.21f32.powi(2) && ring >= 0.09f32.powi(2);
    let shaft = (0.5..=0.86).contains(&x) && (y - 0.5).abs() <= 0.065;
    let teeth =
        ((0.64..=0.72).contains(&x) || (0.78..=0.86).contains(&x)) && (0.5..=0.7).contains(&y);
    Some(if bow || shaft || teeth { WHITE } else { BLUE })
}

/// The icon's pixels, `size` × `size`, top row first, as 0xAARRGGBB with straight (not premultiplied) alpha.
pub fn icon_pixels(size: u32) -> Vec<u32> {
    let n = size.max(1);
    let step = 1.0 / (n * SAMPLES) as f32;
    let mut out = Vec::with_capacity((n * n) as usize);
    for py in 0..n {
        for px in 0..n {
            let (mut covered, mut sum) = (0u32, [0u32; 3]);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = ((px * SAMPLES + sx) as f32 + 0.5) * step;
                    let y = ((py * SAMPLES + sy) as f32 + 0.5) * step;
                    if let Some(c) = paint_at(x, y) {
                        covered += 1;
                        for (s, v) in sum.iter_mut().zip(c) {
                            *s += u32::from(v);
                        }
                    }
                }
            }
            let pixel = if covered == 0 {
                0
            } else {
                let alpha = covered * 255 / (SAMPLES * SAMPLES);
                let [r, g, b] = sum.map(|s| s / covered);
                alpha << 24 | r << 16 | g << 8 | b
            };
            out.push(pixel);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(pixels: &[u32], size: u32, x: f32, y: f32) -> u32 {
        let px = ((x * size as f32) as u32).min(size - 1);
        let py = ((y * size as f32) as u32).min(size - 1);
        pixels[(py * size + px) as usize]
    }

    #[test]
    fn the_tip_and_the_menu_in_both_languages() {
        assert_eq!(tip(Lang::En, "per-call"), "op-reason SSH relay · per-call");
        assert!(tip(Lang::Ja, "show-only").ends_with("show-only"));
        assert_eq!(
            line(Lang::En, "per-call", 42, true),
            "SSH relay · per-call · pid 42 · started by op-reason's task"
        );
        assert!(line(Lang::Ja, "per-call", 42, false).contains("外"));
        for item in Item::ALL {
            assert!(!item.text(Lang::En).is_empty());
            assert!(!item.text(Lang::Ja).is_empty());
            assert_eq!(Item::from_id(item.id()), Some(item));
            assert_eq!(item.args()[0], "--settings");
        }
        assert_eq!(Item::from_id(1), None);
        assert!(Item::Stop.needs_ours() && Item::Restart.needs_ours());
        assert!(!Item::Open.needs_ours() && !Item::Hide.needs_ours());
        assert_eq!(
            Item::Hide.args(),
            ["--settings", "set", "relay.tray-icon", "off"]
        );
    }

    #[test]
    fn the_icon_is_a_white_key_on_a_blue_rounded_square() {
        for size in [16, 20, 24, 32, 48] {
            let p = icon_pixels(size);
            assert_eq!(p.len(), (size * size) as usize);
            assert_eq!(p[0] >> 24, 0, "a corner is clear ({size})");
            let blue = 0xFF00_0000 | 0x25 << 16 | 0x63 << 8 | 0xEB;
            assert_eq!(at(&p, size, 0.5, 0.15), blue, "the background ({size})");
            assert_eq!(at(&p, size, 0.34, 0.5), blue, "the bow's hole ({size})");
            assert_eq!(at(&p, size, 0.6, 0.5), 0xFFFF_FFFF, "the shaft ({size})");
        }
        assert_eq!(icon_pixels(32), icon_pixels(32), "the same every time");
    }
}

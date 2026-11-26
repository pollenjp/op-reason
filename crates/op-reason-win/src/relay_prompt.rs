//! After a click that allows, whether 1Password's prompt came, and where the pointer to the tray goes (design
//! `2026-11-26-prompt-guide-design.md` §4, §5). 1Password keeps the prompt of an application that is not in front behind
//! its icon in the tray; it did so once for a request while it was locked, with op-reason's window in front after the
//! click. The relay watches for the prompt from the moment the request goes to 1Password: a window of 1Password (or of
//! Windows Hello) that comes to the front, or a window of 1Password that appears. When none has come within
//! [`GRACE_MS`], the window lights up and a small window points at the tray ([`spot`]). A wrong guess only adds a guide,
//! which a prompt that comes later takes away.

/// How long the relay waits for 1Password's prompt before it points at the tray (milliseconds after the request went).
pub const GRACE_MS: u64 = 2000;

/// About how long 1Password keeps a request that it did not show before it closes it (seconds after the request went).
pub const CLOSES_AFTER_SECS: u64 = 60;

/// Whose a window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// 1Password: the process that serves its pipe, or a process named `1Password.exe`.
    OnePassword,
    /// Windows Hello (`CredentialUIBroker.exe`), which 1Password asks to unlock it or to approve.
    Hello,
    Other,
}

/// Whose the window of the process `pid` (image name `image`) is; `server` is the process that serves 1Password's pipe.
pub fn owner(pid: u32, image: &str, server: Option<u32>) -> Owner {
    let name = image.rsplit(['\\', '/']).next().unwrap_or(image);
    if server == Some(pid) || name.eq_ignore_ascii_case("1Password.exe") {
        Owner::OnePassword
    } else if name.eq_ignore_ascii_case("CredentialUIBroker.exe") {
        Owner::Hello
    } else {
        Owner::Other
    }
}

/// What one look saw, compared with the moment the request went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Look {
    /// The window in front is another one than then, and it is 1Password's or Windows Hello's.
    pub front: bool,
    /// A visible top-level window of 1Password is there that was not then.
    pub new_window: bool,
}

/// Where the watch is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    /// Nothing seen yet, within [`GRACE_MS`].
    Waiting,
    /// 1Password's prompt came.
    Seen,
    /// Not seen within [`GRACE_MS`]: the window lights up and points at the tray.
    Hidden,
}

/// The watch of one request, from the moment it went to 1Password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    state: Prompt,
    /// When the prompt was seen, and when the pointer came (milliseconds after the request went).
    seen_at: Option<u64>,
    hint_at: Option<u64>,
}

impl Default for Watch {
    fn default() -> Watch {
        Watch::new()
    }
}

impl Watch {
    pub fn new() -> Watch {
        Watch {
            state: Prompt::Waiting,
            seen_at: None,
            hint_at: None,
        }
    }

    /// One look, `now` milliseconds after the request went: seen holds, and a prompt that comes after the pointer takes
    /// it away again.
    pub fn look(&mut self, now: u64, saw: Look) -> Prompt {
        if saw.front || saw.new_window {
            if self.state != Prompt::Seen {
                self.state = Prompt::Seen;
                self.seen_at = Some(now);
            }
        } else if self.state == Prompt::Waiting && now >= GRACE_MS {
            self.state = Prompt::Hidden;
            self.hint_at = Some(now);
        }
        self.state
    }

    pub fn state(&self) -> Prompt {
        self.state
    }

    /// For the record: seconds from the request to the prompt, to a tenth.
    pub fn seen_secs(&self) -> Option<f64> {
        self.seen_at.map(tenths)
    }

    /// For the record: seconds from the request to the pointer, to a tenth.
    pub fn hint_secs(&self) -> Option<f64> {
        self.hint_at.map(tenths)
    }
}

fn tenths(ms: u64) -> f64 {
    (ms as f64 / 100.0).round() / 10.0
}

/// The seconds left until 1Password closes the request, about, `now` milliseconds after it went (counted down, not below
/// 0).
pub fn left_secs(now: u64) -> u64 {
    (CLOSES_AFTER_SECS * 1000)
        .saturating_sub(now)
        .div_ceil(1000)
}

/// A rectangle in pixels of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Px {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The edge of the screen the taskbar is on: the pointer's arrow points that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Bottom,
    Top,
    Left,
    Right,
}

/// Where the pointer goes: its top left corner, and where its arrow's tip is along the side it points to (pixels from
/// its left edge for a taskbar at the bottom or the top, from its top edge for one at the left or the right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    pub left: i32,
    pub top: i32,
    pub tip: i32,
}

/// Where a pointer `w` × `h` pixels goes (arrow included): next to `target` (the notification area) on a taskbar at
/// `side`, `gap` pixels from it, inside `screen`; its arrow's tip at the target's middle, at least `inset` pixels from
/// the pointer's corners.
pub fn spot(screen: Px, side: Side, target: Px, (w, h): (i32, i32), gap: i32, inset: i32) -> Spot {
    let clamp = |v: i32, lo: i32, hi: i32| v.max(lo).min(hi.max(lo));
    let (mid_x, mid_y) = (
        (target.left + target.right) / 2,
        (target.top + target.bottom) / 2,
    );
    match side {
        Side::Bottom | Side::Top => {
            let left = clamp(mid_x - w + 2 * inset, screen.left, screen.right - w);
            let top = if side == Side::Bottom {
                target.top - gap - h
            } else {
                target.bottom + gap
            };
            let top = clamp(top, screen.top, screen.bottom - h);
            Spot {
                left,
                top,
                tip: clamp(mid_x - left, inset, w - inset),
            }
        }
        Side::Left | Side::Right => {
            let top = clamp(mid_y - h + 2 * inset, screen.top, screen.bottom - h);
            let left = if side == Side::Right {
                target.left - gap - w
            } else {
                target.right + gap
            };
            let left = clamp(left, screen.left, screen.right - w);
            Spot {
                left,
                top,
                tip: clamp(mid_y - top, inset, h - inset),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whose_a_window_is() {
        assert_eq!(
            owner(
                40,
                r"C:\Program Files\1Password\app\8\1Password.exe",
                Some(7)
            ),
            Owner::OnePassword
        );
        assert_eq!(
            owner(7, "anything.exe", Some(7)),
            Owner::OnePassword,
            "the pipe's server"
        );
        assert_eq!(owner(40, "1PASSWORD.EXE", None), Owner::OnePassword);
        assert_eq!(
            owner(41, r"C:\Windows\System32\CredentialUIBroker.exe", Some(7)),
            Owner::Hello
        );
        assert_eq!(owner(42, "WindowsTerminal.exe", Some(7)), Owner::Other);
        assert_eq!(
            owner(43, "1Password-BrowserSupport.exe", Some(7)),
            Owner::Other
        );
    }

    #[test]
    fn a_prompt_that_comes_within_two_seconds_is_seen() {
        let mut w = Watch::new();
        assert_eq!(w.look(100, Look::default()), Prompt::Waiting);
        assert_eq!(
            w.look(
                450,
                Look {
                    front: true,
                    new_window: false
                }
            ),
            Prompt::Seen
        );
        assert_eq!(w.look(3000, Look::default()), Prompt::Seen, "seen holds");
        assert_eq!(w.seen_secs(), Some(0.5));
        assert_eq!(w.hint_secs(), None);
        let mut w = Watch::new();
        assert_eq!(
            w.look(
                900,
                Look {
                    front: false,
                    new_window: true
                }
            ),
            Prompt::Seen,
            "a new window counts too"
        );
    }

    #[test]
    fn no_prompt_within_two_seconds_points_at_the_tray_until_it_comes() {
        let mut w = Watch::new();
        assert_eq!(w.look(1999, Look::default()), Prompt::Waiting);
        assert_eq!(w.look(2000, Look::default()), Prompt::Hidden);
        assert_eq!(w.look(5000, Look::default()), Prompt::Hidden);
        assert_eq!(w.hint_secs(), Some(2.0));
        assert_eq!(w.seen_secs(), None);
        assert_eq!(
            w.look(
                23_450,
                Look {
                    front: true,
                    new_window: false
                }
            ),
            Prompt::Seen,
            "it came after all"
        );
        assert_eq!(w.seen_secs(), Some(23.5));
        assert_eq!(
            w.hint_secs(),
            Some(2.0),
            "the record keeps when the pointer came"
        );
    }

    #[test]
    fn the_seconds_left_until_1password_closes_the_request() {
        assert_eq!(left_secs(0), 60);
        assert_eq!(left_secs(2000), 58);
        assert_eq!(left_secs(2001), 58, "rounded up");
        assert_eq!(left_secs(59_500), 1);
        assert_eq!(left_secs(60_000), 0);
        assert_eq!(left_secs(75_000), 0);
    }

    const SCREEN: Px = Px {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    #[test]
    fn above_the_tray_of_a_taskbar_at_the_bottom() {
        // the notification area at the right of a taskbar 48 px high
        let tray = Px {
            left: 1700,
            top: 1032,
            right: 1900,
            bottom: 1080,
        };
        let s = spot(SCREEN, Side::Bottom, tray, (320, 140), 4, 24);
        assert_eq!(s.top, 1032 - 4 - 140, "just above the taskbar");
        assert!(s.left + 320 <= 1920, "inside the screen");
        assert_eq!(s.left + s.tip, 1800, "the tip at the tray's middle");
        // a tray at the very right: the pointer stays on the screen and its tip moves right, not past its corner
        let edge = Px {
            left: 1880,
            top: 1032,
            right: 1920,
            bottom: 1080,
        };
        let s = spot(SCREEN, Side::Bottom, edge, (320, 140), 4, 24);
        assert_eq!(s.left, 1920 - 320);
        assert_eq!(s.tip, 320 - 24);
    }

    #[test]
    fn next_to_the_tray_of_a_taskbar_elsewhere() {
        let top_tray = Px {
            left: 1700,
            top: 0,
            right: 1900,
            bottom: 48,
        };
        let s = spot(SCREEN, Side::Top, top_tray, (320, 140), 4, 24);
        assert_eq!(s.top, 52, "below the taskbar");
        assert_eq!(s.left + s.tip, 1800);
        // a taskbar at the right: its notification area at its bottom
        let right_tray = Px {
            left: 1860,
            top: 900,
            right: 1920,
            bottom: 1080,
        };
        let s = spot(SCREEN, Side::Right, right_tray, (320, 140), 4, 24);
        assert_eq!(s.left, 1860 - 4 - 320, "left of the taskbar");
        assert!(s.top + 140 <= 1080);
        assert_eq!(s.top + s.tip, 990);
        let left_tray = Px {
            left: 0,
            top: 900,
            right: 60,
            bottom: 1080,
        };
        let s = spot(SCREEN, Side::Left, left_tray, (320, 140), 4, 24);
        assert_eq!(s.left, 64, "right of the taskbar");
        assert_eq!(s.top + s.tip, 990);
    }
}

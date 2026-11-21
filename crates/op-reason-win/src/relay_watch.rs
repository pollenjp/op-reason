//! The watchdog of the SSH relay's windows (design `2026-11-22-front-after-allow-design.md` §12). A window whose thread
//! has waited on Windows for five seconds in one step (making the window, showing it, drawing it) has not appeared: the
//! relay records it once (`window-stuck`, with the step), and again when the step ends (`window-unstuck`, with how long
//! it took). Portable: the Windows side reads the step and the clock once a second.

/// How long one step may take before the window counts as stuck, in milliseconds.
pub const STUCK_MS: u64 = 5000;

/// A step of a window that runs now: when it began (milliseconds on the watchdog's clock) and its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Busy {
    pub since: u64,
    pub step: &'static str,
}

/// What to record after a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    /// A step has run for [`STUCK_MS`]: its name.
    Stuck(&'static str),
    /// The step recorded as stuck has ended, about this long after it began (to the watchdog's next look).
    WentOn { after_ms: u64 },
}

/// The watchdog's memory: the step that it recorded as stuck (by when it began).
#[derive(Debug, Default)]
pub struct Watch {
    told: Option<u64>,
}

impl Watch {
    /// One look at `now`, with the step that runs, if any.
    pub fn look(&mut self, busy: Option<Busy>, now: u64) -> Option<Note> {
        if let Some(since) = self.told
            && busy.is_none_or(|b| b.since != since)
        {
            self.told = None;
            return Some(Note::WentOn {
                after_ms: now.saturating_sub(since),
            });
        }
        let b = busy?;
        if self.told.is_none() && now.saturating_sub(b.since) >= STUCK_MS {
            self.told = Some(b.since);
            return Some(Note::Stuck(b.step));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn busy(since: u64, step: &'static str) -> Option<Busy> {
        Some(Busy { since, step })
    }

    #[test]
    fn steps_that_end_in_time_say_nothing() {
        let mut w = Watch::default();
        assert_eq!(w.look(None, 1_000), None);
        assert_eq!(w.look(busy(1_500, "draw"), 2_000), None);
        assert_eq!(
            w.look(busy(1_500, "draw"), 6_499),
            None,
            "just under five seconds"
        );
        assert_eq!(w.look(None, 7_000), None);
    }

    #[test]
    fn a_step_of_five_seconds_is_stuck_once_and_went_on_when_it_ends() {
        let mut w = Watch::default();
        assert_eq!(w.look(busy(1_000, "draw"), 3_000), None);
        assert_eq!(
            w.look(busy(1_000, "draw"), 6_000),
            Some(Note::Stuck("draw"))
        );
        assert_eq!(w.look(busy(1_000, "draw"), 7_000), None, "once");
        assert_eq!(w.look(busy(1_000, "draw"), 300_000), None);
        assert_eq!(
            w.look(None, 322_000),
            Some(Note::WentOn { after_ms: 321_000 })
        );
        assert_eq!(w.look(None, 323_000), None);
    }

    #[test]
    fn a_new_step_after_a_stuck_one_starts_again() {
        let mut w = Watch::default();
        assert_eq!(w.look(busy(0, "show"), 5_000), Some(Note::Stuck("show")));
        // the stuck step ended and the next one began between two looks
        assert_eq!(
            w.look(busy(9_500, "target"), 10_000),
            Some(Note::WentOn { after_ms: 10_000 })
        );
        assert_eq!(w.look(busy(9_500, "target"), 11_000), None);
        assert_eq!(
            w.look(busy(9_500, "target"), 14_500),
            Some(Note::Stuck("target"))
        );
    }
}

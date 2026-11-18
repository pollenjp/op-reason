//! The toasts for signatures that pass without a window (design `2026-11-18-pass-toasts-design.md` §4.4): which cards
//! are shown, where, and how visible at a time, as plain data. The relay's toast window (`win::pass_window`, Windows
//! only) draws them; this part is tested everywhere. Times are milliseconds on the caller's clock, lengths DIPs.
//!
//! - One card per call: a signature of a call that has a card adds one to its count, takes its texts, glows once and
//!   starts the four seconds again (a fading card comes back).
//! - A new call's card enters at the top and the others move down; a fourth card pushes the oldest out at once.
//! - Four seconds after its last signature a card fades out in 0.8 seconds, and the cards below move up.
//! - The count is that of the call's signatures since its answer, also across cards: it goes on when the call signs
//!   again after its card has gone.
//!
//! Without motion (Windows' animation effects off) cards neither slide, fade nor glow: they appear and go at once.

use std::collections::HashMap;
use std::hash::Hash;

use crate::relay_ui::PassTexts;

/// How long a card stays after the last signature of its call.
pub const SHOW_MS: u64 = 4000;
/// How long a card takes to fade out.
pub const FADE_MS: u64 = 800;
/// How long a new card takes to slide in, and the others to move to their new places.
pub const ENTER_MS: u64 = 240;
/// How long a new card takes to appear, and a fading one to come back.
pub const APPEAR_MS: u64 = 220;
/// How long a card that a fourth one pushes out takes to go.
pub const EVICT_MS: u64 = 140;
/// How long a card glows when its call signs again.
pub const BUMP_MS: u64 = 420;
/// The time between two frames while cards are shown with motion.
pub const FRAME_MS: u64 = 16;
/// How many cards are shown at most.
pub const MAX_CARDS: usize = 3;
/// The room between the work area's edges and the cards.
pub const EDGE: f32 = 12.0;
/// The room between two cards.
pub const GAP: f32 = 8.0;
/// A card's width.
pub const CARD_W: f32 = 300.0;
/// A card's height.
pub const CARD_H: f32 = 70.0;
/// How far above its place a new card starts.
pub const DROP: f32 = 12.0;
/// A card pushed out is dropped this long after it has gone transparent.
const EVICT_LINGER_MS: u64 = 40;

/// The top of the card in place `slot` (0 at the top).
pub fn slot_y(slot: usize) -> f32 {
    EDGE + slot as f32 * (CARD_H + GAP)
}

/// A value that moves from `from` to `to` in `dur` milliseconds from `at` (linear, or easing out).
#[derive(Debug, Clone, Copy)]
struct Anim {
    from: f32,
    to: f32,
    at: u64,
    dur: u64,
    ease_out: bool,
}

impl Anim {
    fn still(v: f32) -> Anim {
        Anim {
            from: v,
            to: v,
            at: 0,
            dur: 0,
            ease_out: false,
        }
    }

    fn value(&self, now: u64) -> f32 {
        if self.dur == 0 || now >= self.at + self.dur {
            return self.to;
        }
        if now <= self.at {
            return self.from;
        }
        let x = (now - self.at) as f32 / self.dur as f32;
        let k = if self.ease_out {
            1.0 - (1.0 - x).powi(3)
        } else {
            x
        };
        self.from + (self.to - self.from) * k
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Shown until four seconds after `armed`.
    Live,
    /// Fading out since then.
    Fading { since: u64 },
    /// Pushed out by a fourth card since then (no longer holding a place).
    Leaving { since: u64 },
}

#[derive(Debug)]
struct Card<K> {
    id: u64,
    call: K,
    texts: PassTexts,
    count: u32,
    /// The last signature.
    armed: u64,
    /// When the call signed again (the glow).
    bump: Option<u64>,
    y: Anim,
    opacity: Anim,
    state: State,
}

/// One card as drawn at a time.
#[derive(Debug, Clone, PartialEq)]
pub struct CardView<'a> {
    pub id: u64,
    /// The card's top, from the top of the stack.
    pub y: f32,
    pub opacity: f32,
    /// The glow of a call that signed again, 0 to 1.
    pub glow: f32,
    /// The share of its four seconds left, while it is shown with motion and not fading.
    pub left: Option<f32>,
    /// The signatures of its call ([`PassTexts::head`]).
    pub count: u32,
    pub texts: &'a PassTexts,
}

/// The cards of the toasts, keyed by call.
#[derive(Debug)]
pub struct Stack<K> {
    animate: bool,
    forget_ms: u64,
    /// The cards that hold a place, top first.
    cards: Vec<Card<K>>,
    /// The cards pushed out, going.
    leaving: Vec<Card<K>>,
    /// The signatures of each call under each answer, and the last one's time.
    counts: HashMap<(K, String), (u32, u64)>,
    next_id: u64,
}

impl<K: Clone + Eq + Hash> Stack<K> {
    /// No cards. `animate`: with motion; `forget_ms`: how long a call's count is kept without a signature (the relay's
    /// time limit: no answer lasts longer).
    pub fn new(animate: bool, forget_ms: u64) -> Stack<K> {
        Stack {
            animate,
            forget_ms,
            cards: Vec::new(),
            leaving: Vec::new(),
            counts: HashMap::new(),
            next_id: 1,
        }
    }

    /// Motion or not, for the cards that come next (read when a stack starts empty).
    pub fn set_animate(&mut self, animate: bool) {
        self.animate = animate;
    }

    /// No card is shown.
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty() && self.leaving.is_empty()
    }

    fn dur(&self, ms: u64) -> u64 {
        if self.animate { ms } else { 0 }
    }

    /// A signature of `call` passed at `now`, under the answer recorded as `answer`.
    pub fn pass(&mut self, now: u64, call: K, answer: &str, texts: PassTexts) {
        self.advance(now);
        let forget = self.forget_ms;
        self.counts
            .retain(|_, (_, last)| now.saturating_sub(*last) < forget);
        let entry = self
            .counts
            .entry((call.clone(), answer.to_owned()))
            .or_insert((0, now));
        entry.0 += 1;
        entry.1 = now;
        let count = entry.0;
        let appear = self.dur(APPEAR_MS);
        let animate = self.animate;
        if let Some(c) = self.cards.iter_mut().find(|c| c.call == call) {
            c.count = count;
            c.texts = texts;
            c.armed = now;
            c.bump = animate.then_some(now);
            if matches!(c.state, State::Fading { .. }) {
                c.opacity = Anim {
                    from: c.opacity.value(now),
                    to: 1.0,
                    at: now,
                    dur: appear,
                    ease_out: false,
                };
            }
            c.state = State::Live;
            return;
        }
        let top = slot_y(0);
        let card = Card {
            id: self.next_id,
            call,
            texts,
            count,
            armed: now,
            bump: None,
            y: if animate {
                Anim {
                    from: top - DROP,
                    to: top,
                    at: now,
                    dur: ENTER_MS,
                    ease_out: true,
                }
            } else {
                Anim::still(top)
            },
            opacity: if animate {
                Anim {
                    from: 0.0,
                    to: 1.0,
                    at: now,
                    dur: APPEAR_MS,
                    ease_out: false,
                }
            } else {
                Anim::still(1.0)
            },
            state: State::Live,
        };
        self.next_id += 1;
        self.cards.insert(0, card);
        // cards only enter at the top: the last is the oldest
        while self.cards.len() > MAX_CARDS {
            let Some(mut old) = self.cards.pop() else {
                break;
            };
            if animate {
                old.opacity = Anim {
                    from: old.opacity.value(now),
                    to: 0.0,
                    at: now,
                    dur: EVICT_MS,
                    ease_out: false,
                };
                old.state = State::Leaving { since: now };
                self.leaving.push(old);
            }
        }
        self.layout(now);
    }

    /// Moves every card that holds a place towards its place, from `now`.
    fn layout(&mut self, now: u64) {
        let dur = self.dur(ENTER_MS);
        for (i, c) in self.cards.iter_mut().enumerate() {
            let to = slot_y(i);
            if c.y.to != to {
                c.y = Anim {
                    from: c.y.value(now),
                    to,
                    at: now,
                    dur,
                    ease_out: true,
                };
            }
        }
    }

    /// When a card changes next: it starts to fade, or it goes.
    fn due(&self, c: &Card<K>) -> u64 {
        match c.state {
            State::Live => c.armed + SHOW_MS,
            State::Fading { since } => since + self.dur(FADE_MS),
            State::Leaving { since } => since + EVICT_MS + EVICT_LINGER_MS,
        }
    }

    /// Applies what is due by `now`, each at its own time and in time order.
    pub fn advance(&mut self, now: u64) {
        loop {
            let held = self
                .cards
                .iter()
                .enumerate()
                .map(|(i, c)| (self.due(c), false, i));
            let left = self
                .leaving
                .iter()
                .enumerate()
                .map(|(i, c)| (self.due(c), true, i));
            let Some((t, leaving, i)) = held
                .chain(left)
                .filter(|(t, _, _)| *t <= now)
                .min_by_key(|(t, _, _)| *t)
            else {
                return;
            };
            if leaving {
                self.leaving.remove(i);
                continue;
            }
            match self.cards[i].state {
                State::Live => {
                    let fade = self.dur(FADE_MS);
                    let c = &mut self.cards[i];
                    c.state = State::Fading { since: t };
                    c.opacity = Anim {
                        from: c.opacity.value(t),
                        to: 0.0,
                        at: t,
                        dur: fade,
                        ease_out: false,
                    };
                }
                State::Fading { .. } | State::Leaving { .. } => {
                    self.cards.remove(i);
                    self.layout(t);
                }
            }
        }
    }

    /// The cards at `now`: those holding a place, top first, then those leaving. Call [`Stack::advance`] first.
    pub fn frame(&self, now: u64) -> Vec<CardView<'_>> {
        self.cards
            .iter()
            .chain(self.leaving.iter())
            .map(|c| CardView {
                id: c.id,
                y: c.y.value(now),
                opacity: c.opacity.value(now),
                glow: c
                    .bump
                    .map_or(0.0, |b| glow(now.saturating_sub(b), now >= b)),
                left: (self.animate && c.state == State::Live).then(|| {
                    (1.0 - now.saturating_sub(c.armed) as f32 / SHOW_MS as f32).clamp(0.0, 1.0)
                }),
                count: c.count,
                texts: &c.texts,
            })
            .collect()
    }

    /// Line 1 of each card that holds a place, top first (the toast window's text).
    pub fn heads(&self) -> Vec<String> {
        self.cards.iter().map(|c| c.texts.head(c.count)).collect()
    }

    /// When to draw next: a frame later while cards move, else when one goes; `None` without cards.
    pub fn next_wake(&self, now: u64) -> Option<u64> {
        if self.is_empty() {
            return None;
        }
        if self.animate {
            return Some(now + FRAME_MS);
        }
        self.cards
            .iter()
            .chain(self.leaving.iter())
            .map(|c| self.due(c).max(now))
            .min()
    }
}

/// The glow `ms` after a signature: up in a quarter of [`BUMP_MS`], down in the rest.
fn glow(ms: u64, started: bool) -> f32 {
    if !started || ms >= BUMP_MS {
        return 0.0;
    }
    let p = ms as f32 / BUMP_MS as f32;
    if p < 0.25 {
        p / 0.25
    } else {
        1.0 - (p - 0.25) / 0.75
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: u64 = 60 * 60 * 1000;

    fn t(caller: &str) -> PassTexts {
        dest(caller, caller)
    }

    fn dest(caller: &str, destination: &str) -> PassTexts {
        PassTexts {
            word: "Passed".into(),
            caller: caller.into(),
            command: format!("ssh.exe {destination}"),
            destination: Some(destination.into()),
            why: "allowed for this call · r".into(),
        }
    }

    fn callers<'a>(f: &'a [CardView<'_>]) -> Vec<&'a str> {
        f.iter().map(|c| c.texts.caller.as_str()).collect()
    }

    #[test]
    fn a_new_card_slides_in_at_the_top_and_pushes_the_others_down() {
        let mut s = Stack::new(true, HOUR);
        s.pass(0, "a", "r1", t("A"));
        let f = s.frame(0);
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].y, f[0].opacity), (EDGE - DROP, 0.0), "from above");
        let f = s.frame(ENTER_MS);
        assert_eq!((f[0].y, f[0].opacity), (EDGE, 1.0), "in its place");
        s.pass(1000, "b", "r2", t("B"));
        s.pass(2000, "c", "r3", t("C"));
        s.advance(2000 + ENTER_MS / 2);
        let moving = s.frame(2000 + ENTER_MS / 2)[1].y;
        assert!(EDGE < moving && moving < EDGE + CARD_H + GAP, "{moving}");
        s.advance(2000 + ENTER_MS);
        let f = s.frame(2000 + ENTER_MS);
        assert_eq!(callers(&f), ["C", "B", "A"], "the newest at the top");
        let ys: Vec<f32> = f.iter().map(|c| c.y).collect();
        assert_eq!(ys, [12.0, 90.0, 168.0]);
    }

    #[test]
    fn one_card_per_call_counts_and_shows_the_last_line() {
        let mut s = Stack::new(true, HOUR);
        s.pass(0, "a", "r1", dest("A", "web-01"));
        s.pass(300, "a", "r1", dest("A", "web-02"));
        s.pass(600, "a", "r1", dest("A", "web-03"));
        let f = s.frame(600);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].count, 3);
        assert_eq!(f[0].texts.destination.as_deref(), Some("web-03"));
        assert_eq!(s.heads(), ["Passed · A ×3"]);
        assert!(s.frame(600 + BUMP_MS / 4)[0].glow > 0.9, "it glows");
        assert_eq!(s.frame(600 + BUMP_MS)[0].glow, 0.0);
        assert_eq!(s.frame(600)[0].left, Some(1.0), "four seconds again");
        assert_eq!(s.frame(600 + SHOW_MS / 2)[0].left, Some(0.5));
        s.advance(600 + SHOW_MS - 1);
        assert_eq!(s.frame(600 + SHOW_MS - 1)[0].opacity, 1.0);
    }

    #[test]
    fn a_burst_of_one_call_is_one_card() {
        let mut s = Stack::new(true, HOUR);
        for _ in 0..50 {
            s.pass(5, "a", "r1", t("A"));
        }
        let f = s.frame(5);
        assert_eq!((f.len(), f[0].count), (1, 50));
        assert_eq!(s.heads(), ["Passed · A ×50"]);
    }

    #[test]
    fn four_seconds_after_the_last_pass_it_fades_and_the_others_close_up() {
        let mut s = Stack::new(true, HOUR);
        s.pass(0, "a", "r1", t("A"));
        s.pass(100, "b", "r2", t("B"));
        s.pass(2000, "a", "r1", t("A"));
        s.advance(4099);
        let f = s.frame(4099);
        assert_eq!(callers(&f), ["B", "A"]);
        assert_eq!(f[0].opacity, 1.0);
        s.advance(4500);
        let f = s.frame(4500);
        assert_eq!(f[0].opacity, 0.5, "half way through its 0.8 s");
        assert_eq!(f[0].left, None, "no line while it fades");
        s.advance(4900);
        let f = s.frame(4900);
        assert_eq!(callers(&f), ["A"], "gone after 0.8 s");
        assert_eq!(f[0].y, EDGE + CARD_H + GAP, "and A starts to move up");
        s.advance(4900 + ENTER_MS);
        assert_eq!(s.frame(4900 + ENTER_MS)[0].y, EDGE);
    }

    #[test]
    fn a_pass_brings_a_fading_card_back() {
        let mut s = Stack::new(true, HOUR);
        s.pass(0, "a", "r1", t("A"));
        s.advance(4300);
        assert!(s.frame(4300)[0].opacity < 1.0, "fading");
        s.pass(4300, "a", "r1", t("A"));
        assert_eq!(s.frame(4300 + APPEAR_MS)[0].opacity, 1.0, "back");
        s.advance(8299);
        assert_eq!(s.frame(8299)[0].opacity, 1.0, "four seconds again");
        s.advance(8300 + FADE_MS);
        assert!(s.is_empty());
    }

    #[test]
    fn the_fourth_card_pushes_the_oldest_out_at_once() {
        let mut s = Stack::new(true, HOUR);
        for (k, call) in ["a", "b", "c", "d"].into_iter().enumerate() {
            s.pass(10 * k as u64, call, "r", t(&call.to_uppercase()));
        }
        let f = s.frame(30);
        assert_eq!(callers(&f), ["D", "C", "B", "A"], "A leaves, last");
        assert_eq!(s.heads().len(), 3, "three hold a place");
        assert_eq!(s.frame(30 + EVICT_MS)[3].opacity, 0.0, "gone in 140 ms");
        s.advance(30 + EVICT_MS + 40);
        assert_eq!(callers(&s.frame(30 + EVICT_MS + 40)), ["D", "C", "B"]);
    }

    #[test]
    fn the_count_goes_on_after_the_card_has_gone_and_starts_again_for_a_new_answer() {
        let mut s = Stack::new(true, HOUR);
        s.pass(0, "a", "r1", t("A"));
        s.advance(5000);
        assert!(s.is_empty());
        s.pass(5000, "a", "r1", t("A"));
        assert_eq!(s.frame(5000)[0].count, 2, "the same answer: it goes on");
        s.advance(10_000);
        s.pass(10_000, "a", "r9", t("A"));
        assert_eq!(s.frame(10_000)[0].count, 1, "a new answer: from one");
    }

    #[test]
    fn counts_unused_for_the_time_limit_are_forgotten() {
        let mut s = Stack::new(true, 1000);
        s.pass(0, "a", "r1", t("A"));
        s.pass(500, "a", "r1", t("A"));
        assert_eq!(s.frame(500)[0].count, 2);
        s.advance(6000);
        s.pass(6000, "a", "r1", t("A"));
        assert_eq!(s.frame(6000)[0].count, 1);
    }

    #[test]
    fn without_animation_cards_appear_and_go_at_once() {
        let mut s = Stack::new(false, HOUR);
        s.pass(0, "a", "r1", t("A"));
        let f = s.frame(0);
        assert_eq!((f[0].y, f[0].opacity), (EDGE, 1.0));
        assert_eq!(f[0].left, None, "no line");
        s.pass(100, "b", "r2", t("B"));
        let f = s.frame(100);
        assert_eq!(callers(&f), ["B", "A"]);
        assert_eq!(f[1].y, EDGE + CARD_H + GAP, "moved at once");
        s.pass(200, "a", "r1", t("A"));
        assert_eq!(s.frame(200)[1].glow, 0.0, "no glow");
        s.advance(4099);
        assert_eq!(s.frame(4099).len(), 2);
        s.advance(4100);
        let f = s.frame(4100);
        assert_eq!(callers(&f), ["A"], "B went at four seconds");
        assert_eq!(f[0].y, EDGE, "A moved up at once");
        s.pass(4150, "c", "r", t("C"));
        s.pass(4150, "d", "r", t("D"));
        assert_eq!(callers(&s.frame(4150)), ["D", "C", "A"]);
        s.pass(4150, "e", "r", t("E"));
        assert_eq!(
            callers(&s.frame(4150)),
            ["E", "D", "C"],
            "pushed out, A went at once"
        );
    }

    #[test]
    fn when_to_draw_next() {
        let mut s = Stack::new(true, HOUR);
        assert_eq!(s.next_wake(0), None);
        s.pass(0, "a", "r1", t("A"));
        assert_eq!(s.next_wake(0), Some(FRAME_MS));
        let mut still = Stack::new(false, HOUR);
        still.pass(0, "a", "r1", t("A"));
        assert_eq!(still.next_wake(10), Some(SHOW_MS), "when it goes");
        assert_eq!(
            still.next_wake(SHOW_MS + 5),
            Some(SHOW_MS + 5),
            "overdue: now"
        );
        still.advance(SHOW_MS);
        assert_eq!(still.next_wake(SHOW_MS), None);
    }
}

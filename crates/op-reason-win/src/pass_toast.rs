//! The toasts for signatures that pass without a window (design `2026-11-18-pass-toasts-design.md` §4.4): which cards
//! are shown, where, and how visible at a time, as plain data. The relay's toast window (`win::pass_window`, Windows
//! only) draws them; this part is tested everywhere. Times are milliseconds on the caller's clock, lengths DIPs.
//!
//! - One card per call: a signature of a call that has a card adds one to its count, takes its texts, glows once and
//!   starts the four seconds again (a fading card comes back).
//! - A new call's card enters at the top and the others move down; a fourth card pushes out at once the card whose
//!   last signature is the oldest.
//! - The cards pushed out are counted (`+N` under the third card) until four seconds after their last signature; a
//!   call pushed out that signs again comes back as a new card at the top. The cards shown always signed after those
//!   pushed out, so the count is back at 0 before a card shown starts to fade.
//! - Four seconds after its last signature a card fades out in 0.8 seconds, and the cards below move up.
//! - The count is that of the call's signatures since its answer, also across cards: it goes on when the call signs
//!   again after its card has gone.
//! - A card shown is drawn at 70% opacity (the text behind shows through), also while it glows: the glow is a ring
//!   around it, drawn at full strength.
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
/// A card's opacity while it is shown: the text behind it shows through (its glow's ring is not part of it).
pub const OPACITY: f32 = 0.7;
/// The room between the third place and the count of the cards pushed out (`+N`).
pub const MORE_GAP: f32 = 4.0;
/// The height of that count's tag.
pub const MORE_H: f32 = 16.0;
/// A card pushed out is dropped this long after it has gone transparent.
const EVICT_LINGER_MS: u64 = 40;

/// The top of the card in place `slot` (0 at the top).
pub const fn slot_y(slot: usize) -> f32 {
    EDGE + slot as f32 * (CARD_H + GAP)
}

/// The top of the count of the cards pushed out (`+N`): under the third place.
pub const fn more_y() -> f32 {
    slot_y(MAX_CARDS - 1) + CARD_H + MORE_GAP
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

/// A card pushed out, counted until its four seconds have passed.
#[derive(Debug)]
struct Pushed<K> {
    call: K,
    until: u64,
}

/// One card as drawn at a time.
#[derive(Debug, Clone, PartialEq)]
pub struct CardView<'a> {
    pub id: u64,
    /// The card's top, from the top of the stack.
    pub y: f32,
    /// The whole card's opacity: [`OPACITY`] once it has appeared, less while it appears or fades.
    pub opacity: f32,
    /// How strongly the ring of a call that signed again shows around the card, 0 to 1 (it fades with the card).
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
    /// The cards pushed out whose four seconds have not passed (`+N`).
    pushed: Vec<Pushed<K>>,
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
            pushed: Vec::new(),
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
        // a call pushed out comes back as a new card
        self.pushed.retain(|p| p.call != call);
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
        while self.cards.len() > MAX_CARDS {
            // the card whose last signature is the oldest (of two, the lower one); never the new one at the top
            let Some(i) = (1..self.cards.len())
                .rev()
                .min_by_key(|&i| self.cards[i].armed)
            else {
                break;
            };
            let mut old = self.cards.remove(i);
            if old.state == State::Live {
                self.pushed.push(Pushed {
                    call: old.call.clone(),
                    until: old.armed + SHOW_MS,
                });
            }
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
        self.pushed.retain(|p| p.until > now);
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
            .map(|c| {
                // how far the card has appeared: 1 once in, 0 when gone
                let shown = c.opacity.value(now);
                let glow = c
                    .bump
                    .map_or(0.0, |b| glow(now.saturating_sub(b), now >= b));
                CardView {
                    id: c.id,
                    y: c.y.value(now),
                    opacity: shown * OPACITY,
                    glow: glow * shown,
                    left: (self.animate && c.state == State::Live).then(|| {
                        (1.0 - now.saturating_sub(c.armed) as f32 / SHOW_MS as f32).clamp(0.0, 1.0)
                    }),
                    count: c.count,
                    texts: &c.texts,
                }
            })
            .collect()
    }

    /// How many cards were pushed out whose four seconds have not passed (`+N`). Call [`Stack::advance`] first.
    pub fn more(&self) -> usize {
        self.pushed.len()
    }

    /// Line 1 of each card that holds a place, top first (the toast window's text).
    pub fn heads(&self) -> Vec<String> {
        self.cards.iter().map(|c| c.texts.head(c.count)).collect()
    }

    /// When to draw next: a frame later while cards move, else when a card or the count changes; `None` without cards.
    pub fn next_wake(&self, now: u64) -> Option<u64> {
        if self.is_empty() {
            return None;
        }
        if self.animate {
            return Some(now + FRAME_MS);
        }
        let cards = self
            .cards
            .iter()
            .chain(self.leaving.iter())
            .map(|c| self.due(c));
        cards
            .chain(self.pushed.iter().map(|p| p.until))
            .map(|t| t.max(now))
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
        assert_eq!((f[0].y, f[0].opacity), (EDGE, OPACITY), "in its place");
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
        assert_eq!(s.frame(600 + SHOW_MS - 1)[0].opacity, OPACITY);
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
        assert_eq!(f[0].opacity, OPACITY);
        s.advance(4500);
        let f = s.frame(4500);
        assert_eq!(
            f[0].opacity,
            0.5 * OPACITY,
            "half way through its 0.8 s, from 70%"
        );
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
        assert!(s.frame(4300)[0].opacity < OPACITY, "fading");
        s.pass(4300, "a", "r1", t("A"));
        assert_eq!(s.frame(4300 + APPEAR_MS)[0].opacity, OPACITY, "back");
        s.advance(8299);
        assert_eq!(s.frame(8299)[0].opacity, OPACITY, "four seconds again");
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
        assert_eq!((f[0].y, f[0].opacity), (EDGE, OPACITY), "70% at once");
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
    fn a_card_is_drawn_at_seventy_percent() {
        let mut s = Stack::new(true, HOUR);
        s.pass(0, "a", "r1", t("A"));
        assert_eq!(s.frame(0)[0].opacity, 0.0, "it appears from nothing");
        assert_eq!(s.frame(APPEAR_MS)[0].opacity, OPACITY, "then 70%");
        s.advance(SHOW_MS + FADE_MS / 4);
        assert_eq!(
            s.frame(SHOW_MS + FADE_MS / 4)[0].opacity,
            0.75 * OPACITY,
            "it fades from 70%"
        );
    }

    #[test]
    fn a_bump_rings_a_card_that_stays_at_seventy_percent() {
        let mut s = Stack::new(true, HOUR);
        s.pass(0, "a", "r1", t("A"));
        s.pass(1000, "a", "r1", t("A"));
        let peak = s.frame(1000 + BUMP_MS / 4);
        assert_eq!(
            (peak[0].opacity, peak[0].glow),
            (OPACITY, 1.0),
            "the ring at full, the card at 70%: a burst keeps the text behind readable"
        );
        assert_eq!(s.frame(1000 + BUMP_MS)[0].glow, 0.0);
        s.advance(5000 + FADE_MS / 2);
        s.pass(5000 + FADE_MS / 2, "a", "r1", t("A"));
        let back = s.frame(5000 + FADE_MS / 2 + BUMP_MS / 4);
        assert!(
            back[0].glow < 1.0 && back[0].opacity < OPACITY,
            "a card coming back rings as much as it shows: {back:?}"
        );
    }

    #[test]
    fn of_cards_that_signed_at_once_the_lowest_is_pushed_out() {
        let mut s = Stack::new(true, HOUR);
        for call in ["a", "b", "c", "d"] {
            s.pass(5, call, "r", t(&call.to_uppercase()));
        }
        assert_eq!(
            s.heads(),
            ["Passed · D", "Passed · C", "Passed · B"],
            "four calls in one wake: A goes"
        );
        assert_eq!(s.more(), 1);
    }

    #[test]
    fn cards_pushed_out_are_counted_until_their_four_seconds_pass() {
        let mut s = Stack::new(true, HOUR);
        for _ in 0..3 {
            s.pass(0, "a", "r", t("A"));
        }
        for (k, call) in ["b", "c", "d", "e", "f", "g", "h", "i", "j"]
            .into_iter()
            .enumerate()
        {
            s.pass(100 * (k as u64 + 1), call, "r", t(&call.to_uppercase()));
        }
        s.advance(3999);
        assert_eq!(s.heads(), ["Passed · J", "Passed · I", "Passed · H"]);
        assert_eq!(
            s.more(),
            7,
            "ten calls at once: three shown and +7 (A ×3 counts once)"
        );
        s.advance(SHOW_MS);
        assert_eq!(s.more(), 6, "A's four seconds have passed");
        s.advance(SHOW_MS + 350);
        assert_eq!(s.more(), 3, "B, C and D too");
        s.advance(SHOW_MS + 600);
        assert_eq!(s.more(), 0, "G at last");
        assert_eq!(s.heads().len(), 3, "before the cards shown fade");
        assert_eq!(more_y(), 242.0, "the tag: 4 DIPs under the third place");
    }

    #[test]
    fn the_card_pushed_out_is_the_one_whose_last_signature_is_the_oldest() {
        let mut s = Stack::new(true, HOUR);
        s.pass(0, "a", "r", t("A"));
        s.pass(100, "b", "r", t("B"));
        s.pass(200, "c", "r", t("C"));
        s.pass(3000, "a", "r", t("A"));
        s.pass(3100, "d", "r", t("D"));
        assert_eq!(
            s.heads(),
            ["Passed · D", "Passed · C", "Passed · A ×2"],
            "B goes, not A, the bottom card that signed again"
        );
        assert_eq!(s.more(), 1);
        s.advance(3100 + ENTER_MS);
        let f = s.frame(3100 + ENTER_MS);
        assert_eq!(callers(&f), ["D", "C", "A"]);
        let ys: Vec<f32> = f.iter().map(|c| c.y).collect();
        assert_eq!(ys, [12.0, 90.0, 168.0], "A stays at the bottom");
    }

    #[test]
    fn a_call_pushed_out_that_signs_again_comes_back_on_top() {
        let mut s = Stack::new(true, HOUR);
        for (k, call) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
            s.pass(100 * k as u64, call, "r", t(&call.to_uppercase()));
        }
        assert_eq!(s.more(), 2, "A and B pushed out");
        s.pass(1000, "a", "r", t("A"));
        assert_eq!(
            s.heads(),
            ["Passed · A ×2", "Passed · E", "Passed · D"],
            "back at the top, its count going on"
        );
        assert_eq!(s.more(), 2, "A left the count and C joined it");
        s.advance(4100);
        assert_eq!(s.more(), 1, "B's four seconds have passed");
        s.advance(4200);
        assert_eq!(s.more(), 0, "and C's");
    }

    #[test]
    fn the_count_is_gone_before_a_card_shown_fades() {
        let mut s = Stack::new(true, HOUR);
        s.pass(0, "a", "r", t("A"));
        s.pass(100, "b", "r", t("B"));
        s.pass(200, "c", "r", t("C"));
        s.pass(3000, "a", "r", t("A"));
        s.pass(3100, "d", "r", t("D"));
        s.pass(3200, "e", "r", t("E"));
        let mut counted = false;
        for now in (3200..12_000).step_by(10) {
            s.advance(now);
            let held = s.heads().len();
            let fading = s.frame(now)[..held].iter().any(|c| c.left.is_none());
            if s.more() > 0 {
                counted = true;
                assert_eq!(held, MAX_CARDS, "+N under a third card at {now}");
                assert!(!fading, "+N while a card shown fades at {now}");
            }
        }
        assert!(counted);
    }

    #[test]
    fn without_animation_the_count_goes_down_on_time() {
        let mut s = Stack::new(false, HOUR);
        for (k, call) in ["a", "b", "c", "d"].into_iter().enumerate() {
            s.pass(100 * k as u64, call, "r", t(&call.to_uppercase()));
        }
        assert_eq!(s.more(), 1);
        assert_eq!(
            s.next_wake(300),
            Some(SHOW_MS),
            "A's four seconds end first"
        );
        s.advance(SHOW_MS);
        assert_eq!(s.more(), 0);
        assert_eq!(s.heads().len(), 3, "B, C and D are still shown");
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

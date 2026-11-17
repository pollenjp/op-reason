//! The windows of the relay, one at a time (design §5.5): a request takes a ticket in the queue; the thread whose
//! ticket's turn it is takes op-reason's lock (`Local\op-reason`, shared with `op-reason.exe` and the launcher, so that
//! op-reason's windows and 1Password's prompts come in pairs), writes `holder.json`, shows the window and its toast,
//! and passes the turn on when it closes. A request of a call that asks already waits for that answer.

use std::fs::File;
use std::path::PathBuf;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};

use op_reason_core::contract;
use op_reason_core::lang::Lang;
use op_reason_core::ssh::decide::{Answer, Decision, Join, Mark, ProcId};
use op_reason_core::time;
use windows::Win32::UI::WindowsAndMessaging::{
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, QS_ALLINPUT,
};

use super::{End, Shared, State, peer, pipe};
use crate::relay_ui::{self, Facts, Phase};
use crate::ui::Button;
use crate::win::{app, holder, mutex, toast, window};

/// How often a waiting thread looks at its client and the queue.
const POLL: Duration = Duration::from_millis(250);

/// A place in the queue of windows. Dropping it (also when a thread unwinds) leaves the queue and passes the turn on.
pub(super) struct Ticket<'a> {
    shared: &'a Shared,
    pub t: u64,
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        let mut st = self.shared.lock();
        st.queue.done(self.t);
        drop(st);
        self.shared.turn.notify_all();
    }
}

/// Following the ask of another request of the same call (counted, so that its window stays while the call lives).
struct Following<'a> {
    shared: &'a Shared,
    t: u64,
}

impl Following<'_> {
    fn leave(self, st: &mut State) {
        if let Some(n) = st.followers.get_mut(&self.t) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                st.followers.remove(&self.t);
            }
        }
        std::mem::forget(self);
    }
}

impl Drop for Following<'_> {
    fn drop(&mut self) {
        let mut st = self.shared.lock();
        if let Some(n) = st.followers.get_mut(&self.t) {
            *n = n.saturating_sub(1);
        }
    }
}

/// How a request ended before or in its window.
pub(super) enum Outcome<'a> {
    /// An earlier answer allowed its call (or its terminal).
    Passed(Decision),
    /// An earlier answer denied its call.
    Refused(Decision),
    /// A click; the window stays (saying that 1Password may ask now) until the request has gone through.
    Asked(Answer, Box<Shown<'a>>),
    /// No answer within the wait.
    Timeout,
    /// The window was closed without an answer.
    Dismissed,
    /// The client went away first.
    Gone,
    /// op-reason's lock was not free within `OPREASON_LOCK_WAIT`.
    LockTimeout,
    /// The window could not be shown.
    NoWindow,
}

/// A window shown by this thread, with op-reason's lock and the turn: on drop the holder file goes, the window closes,
/// the lock is released and the turn passes on, in this order.
pub(super) struct Shown<'a> {
    window: window::Window,
    /// Released when the window has closed (after the holder file is gone).
    _held: Option<mutex::Held>,
    ticket: Ticket<'a>,
    holder_dir: Option<PathBuf>,
    facts: Facts,
    phase: Phase,
    lang: Lang,
    last_look: Instant,
    toast: Option<toast::Pending>,
}

impl Drop for Shown<'_> {
    fn drop(&mut self) {
        if let Some(dir) = &self.holder_dir {
            holder::remove(dir);
        }
        // the toast's thread ends by itself; its result is not recorded
        self.toast = None;
    }
}

impl<'a> Shown<'a> {
    /// Shows the window of `facts` (and its toast); with `held`, writes the holder file.
    fn open(
        shared: &'a Shared,
        ticket: Ticket<'a>,
        held: Option<mutex::Held>,
        facts: Facts,
        phase: Phase,
    ) -> Result<Shown<'a>, String> {
        let lang = shared.lang();
        let texts = relay_ui::both(&facts, phase);
        let w = window::Window::create(&texts, lang, false, Box::new(app::remember))
            .map_err(|e| e.message())?;
        w.show();
        let t0 = Instant::now();
        while !w.painted() && t0.elapsed() < Duration::from_millis(500) {
            window::pump();
            std::thread::sleep(Duration::from_millis(2));
        }
        let holder_dir = held.as_ref().and(shared.state_dir.clone());
        if let Some(dir) = &holder_dir {
            holder::write(
                dir,
                &time::now().hms,
                &format!("SSH · {}", relay_ui::caller_short(&facts.caller, lang)),
                &facts.reason,
                &facts.command,
            );
        }
        let toast = Some(toast::start(texts.get(lang).toast.clone()));
        Ok(Shown {
            window: w,
            _held: held,
            ticket,
            holder_dir,
            facts,
            phase,
            lang,
            last_look: Instant::now(),
            toast,
        })
    }

    /// Serves the window's messages for up to `wait`, and keeps the count of waiting calls in its footer.
    pub fn pump(&mut self, wait: Duration) {
        let shared = self.ticket.shared;
        // SAFETY: no handles; waits for input to this thread or the time-out.
        unsafe {
            let _ = MsgWaitForMultipleObjectsEx(
                None,
                wait.as_millis() as u32,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            );
        }
        window::pump();
        if self.last_look.elapsed() >= POLL {
            self.last_look = Instant::now();
            let waiting = shared.lock().queue.waiting();
            if waiting != self.facts.waiting {
                self.facts.waiting = waiting;
                self.window
                    .set_texts(&relay_ui::both(&self.facts, self.phase));
            }
        }
    }

    /// The window without its buttons (after an Allow: 1Password may ask now).
    pub fn asking(&mut self) {
        self.phase = Phase::Asking;
        self.window
            .set_texts(&relay_ui::both(&self.facts, self.phase));
    }

    /// The language the window was opened in.
    pub fn lang(&self) -> Lang {
        self.lang
    }

    fn ticket(&self) -> u64 {
        self.ticket.t
    }
}

/// Waits on the queue's condition for a moment.
fn wait<'a>(shared: &'a Shared, st: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
    shared
        .turn
        .wait_timeout(st, POLL)
        .map(|(g, _)| g)
        .unwrap_or_else(|e| e.into_inner().0)
}

/// Whether a remembered process still runs: WSL roots cannot be seen and only time out.
fn alive(p: &ProcId) -> bool {
    !p.scope.is_empty() || peer::alive(p.pid, p.start)
}

/// op-reason's lock, waiting up to `OPREASON_LOCK_WAIT` seconds (none when the mutex cannot be used at all, as
/// `op-reason.exe` does).
fn lock(shared: &Shared, stop: &dyn Fn() -> bool) -> Result<Option<mutex::Held>, Outcome<'static>> {
    match mutex::acquire(
        contract::MUTEX_NAME,
        Duration::from_secs(shared.lock_wait),
        Duration::ZERO,
        || {},
        stop,
    ) {
        Ok(mutex::Acquired::Held(h)) => Ok(Some(h)),
        Ok(mutex::Acquired::TimedOut) => Err(Outcome::LockTimeout),
        Ok(mutex::Acquired::Interrupted) => Err(Outcome::Gone),
        Err(_) => Ok(None),
    }
}

/// per-call: the answer that covers the request, or its own window (design §5.4, §5.5).
pub(super) fn per_call<'a>(
    shared: &'a Shared,
    mark: &Mark,
    shell: Option<&ProcId>,
    client: &File,
    facts: Facts,
    id: &str,
) -> Outcome<'a> {
    let mut st = shared.lock();
    loop {
        // A request of this call asks already, in its window or with 1Password after the click: wait for it
        if let Some(t) = st.queue.asking(mark) {
            *st.followers.entry(t).or_default() += 1;
            let following = Following { shared, t };
            while st.queue.active(t) {
                st = wait(shared, st);
                if pipe::gone(client) {
                    following.leave(&mut st);
                    return Outcome::Gone;
                }
            }
            following.leave(&mut st);
            // that window ended without an answer: the requests that waited for it fail with it, unremembered
            match st.ended.get(&t).map(|(_, end)| *end) {
                Some(End::Timeout) => return Outcome::Timeout,
                Some(End::Dismissed) => return Outcome::Dismissed,
                None => continue,
            }
        }
        if let Some(d) = st.book.lookup(mark, shell, time::epoch_now(), &alive) {
            return if d.answer.allows() {
                Outcome::Passed(d)
            } else {
                Outcome::Refused(d)
            };
        }
        let Join::Ask(t) = st.queue.join(Some(mark)) else {
            continue;
        };
        let ticket = Ticket { shared, t };
        while !st.queue.turn(t) {
            st = wait(shared, st);
            if pipe::gone(client) {
                drop(st);
                drop(ticket);
                return Outcome::Gone;
            }
        }
        // an answer for the terminal may have come while it waited
        if let Some(d) = st.book.lookup(mark, shell, time::epoch_now(), &alive) {
            drop(st);
            drop(ticket);
            return if d.answer.allows() {
                Outcome::Passed(d)
            } else {
                Outcome::Refused(d)
            };
        }
        if pipe::gone(client) {
            drop(st);
            drop(ticket);
            return Outcome::Gone;
        }
        let waiting = st.queue.waiting();
        drop(st);
        return ask(shared, ticket, mark, shell, client, facts, waiting, id);
    }
}

/// Notes how a window ended without an answer, for the requests that waited for it (kept five minutes).
fn ended(shared: &Shared, t: u64, end: End) {
    let mut st = shared.lock();
    st.ended
        .retain(|_, (at, _)| at.elapsed() < Duration::from_secs(300));
    st.ended.insert(t, (Instant::now(), end));
}

/// The window with buttons, until a click, the answer wait, the window closed, or the client gone with nobody of its
/// call left waiting.
#[allow(clippy::too_many_arguments)]
fn ask<'a>(
    shared: &'a Shared,
    ticket: Ticket<'a>,
    mark: &Mark,
    shell: Option<&ProcId>,
    client: &File,
    mut facts: Facts,
    waiting: usize,
    id: &str,
) -> Outcome<'a> {
    let held = match lock(shared, &|| pipe::gone(client)) {
        Ok(h) => h,
        Err(o) => return o,
    };
    // the client may have gone while the lock was busy
    if pipe::gone(client) {
        return Outcome::Gone;
    }
    facts.waiting = waiting;
    let mut shown = match Shown::open(shared, ticket, held, facts, Phase::Ask) {
        Ok(s) => s,
        Err(_) => return Outcome::NoWindow,
    };
    let deadline = Instant::now() + Duration::from_secs(shared.opts.answer_secs);
    loop {
        shown.pump(Duration::from_millis(50));
        if let Some(b) = shown.window.answer() {
            let answer = match b {
                Button::Allow => Answer::Allow,
                Button::AllowTerminal => Answer::AllowTerminal,
                Button::Deny => Answer::Deny,
            };
            shared
                .lock()
                .book
                .remember(mark, shell, answer, id, time::epoch_now());
            shared.turn.notify_all();
            if answer.allows() {
                shown.asking();
            }
            return Outcome::Asked(answer, Box::new(shown));
        }
        if !shown.window.is_open() {
            ended(shared, shown.ticket(), End::Dismissed);
            return Outcome::Dismissed;
        }
        if Instant::now() >= deadline {
            ended(shared, shown.ticket(), End::Timeout);
            return Outcome::Timeout;
        }
        if pipe::gone(client) {
            let t = shown.ticket();
            if shared.lock().followers.get(&t).copied().unwrap_or(0) == 0 {
                return Outcome::Gone;
            }
        }
    }
}

/// A ticket for a window without buttons (show-only, or 1Password slow to answer).
pub(super) fn join(shared: &Shared) -> Ticket<'_> {
    let mut st = shared.lock();
    let t = match st.queue.join(None) {
        Join::Ask(t) | Join::Follow(t) => t,
    };
    drop(st);
    Ticket { shared, t }
}

/// Whether it is the ticket's turn now (it becomes current then).
pub(super) fn turn(ticket: &Ticket<'_>) -> bool {
    ticket.shared.lock().queue.turn(ticket.t)
}

/// show-only: waits for the turn and op-reason's lock, then shows the window without buttons.
pub(super) fn show_only<'a>(
    shared: &'a Shared,
    client: &File,
    facts: Facts,
) -> Result<Shown<'a>, Outcome<'static>> {
    let ticket = join(shared);
    while !turn(&ticket) {
        if pipe::gone(client) {
            return Err(Outcome::Gone);
        }
        let st = shared.lock();
        drop(wait(shared, st));
    }
    // the client may have gone in the last wait, or while the lock was busy
    if pipe::gone(client) {
        return Err(Outcome::Gone);
    }
    let held = lock(shared, &|| pipe::gone(client))?;
    if pipe::gone(client) {
        return Err(Outcome::Gone);
    }
    Shown::open(shared, ticket, held, facts, Phase::Asking).map_err(|_| Outcome::NoWindow)
}

/// The window without buttons for a request whose answer is late (its ticket's turn has come): no lock, the request is
/// with 1Password already.
pub(super) fn late<'a>(shared: &'a Shared, ticket: Ticket<'a>, facts: Facts) -> Option<Shown<'a>> {
    Shown::open(shared, ticket, None, facts, Phase::Asking).ok()
}

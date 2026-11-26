//! The windows of the relay, one at a time (design §5.5): a request takes a ticket in the queue; the thread whose
//! ticket's turn it is takes op-reason's lock (`Local\op-reason`, shared with `op-reason.exe` and the launcher, so that
//! op-reason's windows and 1Password's prompts come in pairs), writes `holder.json`, shows the window and its toast,
//! and passes the turn on when it closes. A request of a call that asks already waits for that answer. The window's
//! notification goes with it, except after a per-call time-out (design `2026-11-18-pass-toasts-design.md` §6). After a
//! click that allows, the window comes to the front until the request has gone through 1Password, unless the program
//! that started the relay is in front (design `2026-11-22-front-after-allow-design.md` §5), and watches for 1Password's
//! prompt: it says so when the prompt comes, and lights up and points at the tray when it does not (design
//! `2026-11-26-prompt-guide-design.md`).

use std::fs::File;
use std::path::PathBuf;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};

use op_reason_core::contract;
use op_reason_core::lang::Lang;
use op_reason_core::ssh::decide::{Answer, Decision, Join, Mark, ProcId};
use op_reason_core::ssh::record::PromptRec;
use op_reason_core::time;
use windows::Win32::UI::WindowsAndMessaging::{
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, QS_ALLINPUT,
};

use super::{End, Shared, State, peer, pipe};
use crate::relay_front;
use crate::relay_prompt::{self, Prompt};
use crate::relay_ui::{self, Facts, Phase};
use crate::ui::Button;
use crate::win::{app, holder, mutex, procs, prompt, theme, toast, tray_hint, window};

/// How often a waiting thread looks at its client and the queue.
const POLL: Duration = Duration::from_millis(250);

/// How often the window looks for 1Password's prompt after a click that allows.
const LOOK: Duration = Duration::from_millis(100);

/// The watch for 1Password's prompt after a click that allows (design `2026-11-26-prompt-guide-design.md` §5).
struct PromptWatch {
    looker: prompt::Looker,
    watch: relay_prompt::Watch,
    /// When the request went to 1Password.
    sent: Instant,
    last: Instant,
    /// The window came to the front at the click.
    front: bool,
    /// The pointer to the tray, while the prompt has not come.
    pointer: Option<tray_hint::Pointer>,
}

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
    /// Leave the notification when the window closes (it timed out unanswered).
    keep_notification: bool,
    /// The end of the answer wait (the window that asks).
    deadline: Option<Instant>,
    /// After a click that allows: the watch for 1Password's prompt.
    prompt: Option<PromptWatch>,
}

impl Drop for Shown<'_> {
    fn drop(&mut self) {
        if let Some(dir) = &self.holder_dir {
            holder::remove(dir);
        }
        // the notification goes with the window; its result is not recorded
        if let Some(mut t) = self.toast.take()
            && !self.keep_notification
        {
            t.hide();
        }
    }
}

impl<'a> Shown<'a> {
    /// Shows the window of `facts` (and its toast); with `held`, writes the holder file. `answer_wait`: how long the
    /// window that asks waits for a click, counted from when it is up (its countdown shows it).
    fn open(
        shared: &'a Shared,
        ticket: Ticket<'a>,
        held: Option<mutex::Held>,
        facts: Facts,
        phase: Phase,
        answer_wait: Option<Duration>,
    ) -> Result<Shown<'a>, String> {
        let lang = shared.lang();
        let texts = relay_ui::both(&facts, phase);
        let w = window::Window::create(&texts, lang, false, Box::new(app::remember))
            .map_err(|e| e.message())?;
        w.draw_without_gpu();
        w.show();
        let t0 = Instant::now();
        while !w.painted() && t0.elapsed() < Duration::from_millis(500) {
            window::pump();
            std::thread::sleep(Duration::from_millis(2));
        }
        let deadline = answer_wait.map(|wait| {
            let end = Instant::now() + wait;
            w.set_deadline(end, wait);
            end
        });
        let holder_dir = held.as_ref().and(shared.state_dir.clone());
        // the steps that may wait on Windows, for the watchdog of the windows (design
        // `2026-11-22-front-after-allow-design.md` §12)
        let busy = window::Busy::enter(window::Step::Holder);
        if let Some(dir) = &holder_dir {
            holder::write(
                dir,
                &time::now().hms,
                &format!("SSH · {}", relay_ui::caller_short(&facts.caller, lang)),
                &facts.reason,
                &facts.command,
            );
        }
        busy.next(window::Step::Toast);
        let toast = Some(toast::start_for_window(texts.get(lang).toast.clone()));
        drop(busy);
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
            keep_notification: false,
            deadline,
            prompt: None,
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
        self.look_for_prompt();
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

    /// The window came to the front after the click (it gives the front back when it closes).
    pub fn came_forward(&self) -> bool {
        self.window.came_forward()
    }

    /// The request goes to 1Password now, after a click that allows: watches for its prompt from here (`server`: the
    /// process that serves 1Password's pipe).
    pub fn watch_prompt(&mut self, server: Option<u32>) {
        let now = Instant::now();
        self.prompt = Some(PromptWatch {
            looker: prompt::Looker::start(server),
            watch: relay_prompt::Watch::new(),
            sent: now,
            last: now,
            front: self.window.came_forward(),
            pointer: None,
        });
    }

    /// What the relay saw of 1Password's prompt, for the request's record.
    pub fn prompt_record(&self) -> Option<PromptRec> {
        self.prompt.as_ref().map(|p| PromptRec {
            front: p.front,
            seen: p.watch.seen_secs(),
            hint: p.watch.hint_secs(),
        })
    }

    /// Looks for 1Password's prompt (every [`LOOK`]) and turns the window to what it saw: "approve in 1Password's
    /// window" when the prompt came; lit up, with the seconds until 1Password closes the request and the pointer to the
    /// tray, when it has not come within two seconds.
    fn look_for_prompt(&mut self) {
        let Some(p) = self.prompt.as_mut() else {
            return;
        };
        let open = self.window.is_open();
        if !open {
            // closed with its close button while 1Password asks: no pointer without the window
            p.pointer = None;
        }
        if let Some(pointer) = p.pointer.as_mut() {
            pointer.tick();
        }
        if p.last.elapsed() < LOOK {
            return;
        }
        p.last = Instant::now();
        let before = p.watch.state();
        let ms = u64::try_from(p.sent.elapsed().as_millis()).unwrap_or(u64::MAX);
        let saw = p.looker.look();
        let now = p.watch.look(ms, saw);
        if now == before {
            return;
        }
        let sent = p.sent;
        match now {
            Prompt::Seen => {
                p.pointer = None;
                self.phase = Phase::Seen;
                self.window.set_alert(false);
                self.window
                    .set_texts(&relay_ui::both(&self.facts, self.phase));
            }
            Prompt::Hidden => {
                let closes = Duration::from_secs(relay_prompt::CLOSES_AFTER_SECS);
                self.phase = Phase::Hidden;
                self.window.set_deadline(sent + closes, closes);
                self.window
                    .set_texts(&relay_ui::both(&self.facts, self.phase));
                self.window.set_alert(true);
                let pointer = open
                    .then(|| {
                        tray_hint::Pointer::show(
                            relay_ui::pointer_texts(self.lang),
                            theme::current(),
                            sent,
                        )
                    })
                    .flatten();
                if let Some(p) = self.prompt.as_mut() {
                    p.pointer = pointer;
                }
            }
            Prompt::Waiting => {}
        }
    }

    /// The language the window was opened in.
    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// Leaves the notification when the window closes: the sign of a request that went unanswered.
    fn keep_notification(&mut self) {
        self.keep_notification = true;
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

/// Whether the window in front belongs to the program that started the relay (a terminal program): 1Password then
/// sees the application of the relay's requests in front already, and the window stays behind
/// ([`relay_front::front_is_its_program`]).
fn its_program_in_front(shared: &Shared) -> bool {
    let pid = window::front_pid();
    pid != 0 && relay_front::front_is_its_program(&shared.ancestors, pid, procs::created(pid))
}

/// The request's client went away, or the relay was asked to stop (design `2026-11-23-settings-design.md` §8.2): a
/// request that waits for its turn or for op-reason's lock gives up, so that a stop does not wait for it.
fn quit(shared: &Shared, client: &File) -> bool {
    pipe::gone(client) || shared.stopping.load(std::sync::atomic::Ordering::SeqCst)
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
                if quit(shared, client) {
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
            if quit(shared, client) {
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
        if quit(shared, client) {
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
    let held = match lock(shared, &|| quit(shared, client)) {
        Ok(h) => h,
        Err(o) => return o,
    };
    // the client may have gone while the lock was busy
    if quit(shared, client) {
        return Outcome::Gone;
    }
    facts.waiting = waiting;
    let wait = Duration::from_secs(shared.opts.answer_secs);
    let mut shown = match Shown::open(shared, ticket, held, facts, Phase::Ask, Some(wait)) {
        Ok(s) => s,
        Err(_) => return Outcome::NoWindow,
    };
    let deadline = shown.deadline.unwrap_or_else(|| Instant::now() + wait);
    loop {
        shown.pump(Duration::from_millis(50));
        if let Some(b) = shown.window.answer() {
            let answer = match b {
                Button::Allow => Answer::Allow,
                Button::AllowTerminal => Answer::AllowTerminal,
                Button::Deny => Answer::Deny,
            };
            // At once, while the click is the last input: 1Password shows its prompt only when it sees the application
            // that asks in front (design `2026-11-22-front-after-allow-design.md` §5.1)
            if answer.allows() && !its_program_in_front(shared) {
                shown.window.come_forward();
            }
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
            shown.keep_notification();
            return Outcome::Timeout;
        }
        if quit(shared, client) {
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
        if quit(shared, client) {
            return Err(Outcome::Gone);
        }
        let st = shared.lock();
        drop(wait(shared, st));
    }
    // the client may have gone in the last wait, or while the lock was busy
    if quit(shared, client) {
        return Err(Outcome::Gone);
    }
    let held = lock(shared, &|| quit(shared, client))?;
    if quit(shared, client) {
        return Err(Outcome::Gone);
    }
    Shown::open(shared, ticket, held, facts, Phase::Asking, None).map_err(|_| Outcome::NoWindow)
}

/// The window without buttons for a request whose answer is late (its ticket's turn has come): no lock, the request is
/// with 1Password already.
pub(super) fn late<'a>(shared: &'a Shared, ticket: Ticket<'a>, facts: Facts) -> Option<Shown<'a>> {
    Shown::open(shared, ticket, None, facts, Phase::Asking, None).ok()
}

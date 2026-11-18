//! What per-call mode remembers (design §5.1, §5.4) and the order of the windows (§5.5), as plain data: the relay
//! wraps them in a lock, a clock and process checks.
//!
//! A request belongs to a call, identified by its mark: the root process (PID and start time), the key and the reason.
//! An answer covers the whole call until the root ends, the time limit passes or the relay exits. A person's command may
//! instead be covered by the terminal it was typed in (the interactive shell and the key).

use std::collections::{HashMap, VecDeque};

/// A process the relay can name: where it runs (`""` for Windows, `wsl:<distro>` for Linux), its PID and its start
/// time, so that a reused PID never matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcId {
    pub scope: String,
    pub pid: u32,
    pub start: u64,
}

/// Who a request belongs to: a root process, or nobody the relay can see (a forwarded request, a WSL `ssh.exe` run
/// without `op-reason-ssh`), and then every request is a call of its own.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Origin {
    Root(ProcId),
    Alone(u64),
}

/// The mark of a call.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mark {
    pub origin: Origin,
    /// The key's fingerprint.
    pub key: String,
    /// The normalized reason; empty when none was given.
    pub reason: String,
}

/// A click in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Allow,
    AllowTerminal,
    Deny,
}

impl Answer {
    /// The word of the records.
    pub fn as_str(self) -> &'static str {
        match self {
            Answer::Allow => "allow",
            Answer::AllowTerminal => "allow-terminal",
            Answer::Deny => "deny",
        }
    }

    /// Whether the request may go on.
    pub fn allows(self) -> bool {
        self != Answer::Deny
    }
}

/// A remembered answer: the record of the request it was given for, the answer and when (UNIX seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub id: String,
    pub answer: Answer,
    pub at: i64,
}

/// The answers the relay remembers, in memory only.
#[derive(Debug)]
pub struct Book {
    calls: HashMap<Mark, Decision>,
    terminals: HashMap<(ProcId, String), Decision>,
    limit: i64,
}

impl Book {
    /// An empty book whose answers last `limit_secs` seconds.
    pub fn new(limit_secs: i64) -> Book {
        Book {
            calls: HashMap::new(),
            terminals: HashMap::new(),
            limit: limit_secs,
        }
    }

    fn current(&self, d: &Decision, now: i64) -> bool {
        now - d.at < self.limit
    }

    /// The answer that covers a request: its call's, else (for a person's command: `shell` is the interactive shell it
    /// was typed in) its terminal's. What has run out is forgotten first: past the time limit, or a root or shell that
    /// `alive` says has ended.
    pub fn lookup(
        &mut self,
        mark: &Mark,
        shell: Option<&ProcId>,
        now: i64,
        alive: &dyn Fn(&ProcId) -> bool,
    ) -> Option<Decision> {
        if let Some(d) = self.calls.get(mark) {
            let root_alive = match &mark.origin {
                Origin::Root(p) => alive(p),
                Origin::Alone(_) => true,
            };
            if self.current(d, now) && root_alive {
                return Some(d.clone());
            }
            self.calls.remove(mark);
        }
        let shell = shell?;
        let key = (shell.clone(), mark.key.clone());
        let d = self.terminals.get(&key)?;
        if self.current(d, now) && alive(shell) {
            return Some(d.clone());
        }
        self.terminals.remove(&key);
        None
    }

    /// Remembers an answer for the call (never for an [`Origin::Alone`] request); `AllowTerminal` also for the terminal
    /// (`shell` and the key).
    pub fn remember(
        &mut self,
        mark: &Mark,
        shell: Option<&ProcId>,
        answer: Answer,
        id: &str,
        now: i64,
    ) {
        if matches!(mark.origin, Origin::Alone(_)) {
            return;
        }
        let d = Decision {
            id: id.to_owned(),
            answer,
            at: now,
        };
        if answer == Answer::AllowTerminal
            && let Some(shell) = shell
        {
            self.terminals
                .insert((shell.clone(), mark.key.clone()), d.clone());
        }
        self.calls.insert(mark.clone(), d);
    }

    /// Forgets every answer past the time limit or whose root (or shell) has ended.
    pub fn sweep(&mut self, now: i64, alive: &dyn Fn(&ProcId) -> bool) {
        let limit = self.limit;
        self.calls.retain(|m, d| {
            now - d.at < limit
                && match &m.origin {
                    Origin::Root(p) => alive(p),
                    Origin::Alone(_) => true,
                }
        });
        self.terminals
            .retain(|(shell, _), d| now - d.at < limit && alive(shell));
    }

    /// How many answers are remembered.
    pub fn len(&self) -> usize {
        self.calls.len() + self.terminals.len()
    }

    /// Whether nothing is remembered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How a request joins the queue of windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// It asks itself: wait for this ticket's turn.
    Ask(u64),
    /// A request of the same call asks already: wait until that ticket is done, then look again.
    Follow(u64),
}

/// The windows waiting to be shown, one at a time, first come first asked. A ticket without a mark is a window that
/// asks nothing (show-only, or 1Password slow to answer).
#[derive(Debug, Default)]
pub struct Queue {
    next: u64,
    waiting: VecDeque<(u64, Option<Mark>)>,
    current: Option<(u64, Option<Mark>)>,
}

impl Queue {
    /// Joins the queue: follows a waiting or current ticket of the same mark, else takes a new ticket at the end.
    pub fn join(&mut self, mark: Option<&Mark>) -> Join {
        if let Some(m) = mark
            && let Some((t, _)) = self
                .current
                .iter()
                .chain(self.waiting.iter())
                .find(|(_, other)| other.as_ref() == Some(m))
        {
            return Join::Follow(*t);
        }
        self.next += 1;
        self.waiting.push_back((self.next, mark.cloned()));
        Join::Ask(self.next)
    }

    /// Whether it is the ticket's turn: it is current, or nothing is and it is first (then it becomes current).
    pub fn turn(&mut self, ticket: u64) -> bool {
        if let Some((t, _)) = &self.current {
            return *t == ticket;
        }
        if self.waiting.front().is_some_and(|(t, _)| *t == ticket) {
            self.current = self.waiting.pop_front();
            return true;
        }
        false
    }

    /// The ticket is done (current), or leaves the queue (waiting).
    pub fn done(&mut self, ticket: u64) {
        if self.current.as_ref().is_some_and(|(t, _)| *t == ticket) {
            self.current = None;
        } else {
            self.waiting.retain(|(t, _)| *t != ticket);
        }
    }

    /// The ticket that asks for `mark` now (waiting or current), if any: a request of that call follows it.
    pub fn asking(&self, mark: &Mark) -> Option<u64> {
        self.current
            .iter()
            .chain(self.waiting.iter())
            .find(|(_, m)| m.as_ref() == Some(mark))
            .map(|(t, _)| *t)
    }

    /// Whether the ticket is waiting or current.
    pub fn active(&self, ticket: u64) -> bool {
        self.current
            .iter()
            .chain(self.waiting.iter())
            .any(|(t, _)| *t == ticket)
    }

    /// How many tickets wait behind the current one.
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3600;

    fn proc(pid: u32) -> ProcId {
        ProcId {
            scope: String::new(),
            pid,
            start: 1000 + u64::from(pid),
        }
    }

    fn mark(pid: u32, key: &str, reason: &str) -> Mark {
        Mark {
            origin: Origin::Root(proc(pid)),
            key: key.into(),
            reason: reason.into(),
        }
    }

    fn everyone(_: &ProcId) -> bool {
        true
    }

    #[test]
    fn an_answer_covers_its_call_and_nothing_else() {
        let mut b = Book::new(HOUR);
        let m = mark(10, "K", "deploy");
        assert_eq!(b.lookup(&m, None, 0, &everyone), None);
        b.remember(&m, None, Answer::Allow, "r1", 0);
        let d = b.lookup(&m, None, 5, &everyone).unwrap();
        assert_eq!((d.id.as_str(), d.answer), ("r1", Answer::Allow));
        assert_eq!(
            b.lookup(&mark(10, "K", "other reason"), None, 5, &everyone),
            None
        );
        assert_eq!(
            b.lookup(&mark(10, "K2", "deploy"), None, 5, &everyone),
            None
        );
        assert_eq!(b.lookup(&mark(11, "K", "deploy"), None, 5, &everyone), None);
        let reused = Mark {
            origin: Origin::Root(ProcId {
                start: 9,
                ..proc(10)
            }),
            ..m.clone()
        };
        assert_eq!(b.lookup(&reused, None, 5, &everyone), None, "a reused PID");
        b.remember(&mark(12, "K", ""), None, Answer::Deny, "r2", 7);
        assert_eq!(
            b.lookup(&mark(12, "K", ""), None, 8, &everyone)
                .map(|d| d.answer),
            Some(Answer::Deny),
            "a denial covers the call too"
        );
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn forgotten_at_the_time_limit_or_when_the_root_ends() {
        let mut b = Book::new(HOUR);
        let m = mark(10, "K", "r");
        b.remember(&m, None, Answer::Allow, "r1", 100);
        assert!(b.lookup(&m, None, 100 + HOUR - 1, &everyone).is_some());
        assert!(b.lookup(&m, None, 100 + HOUR, &everyone).is_none());
        assert!(b.is_empty(), "forgotten, not only hidden");
        b.remember(&m, None, Answer::Allow, "r2", 200);
        let ended = |p: &ProcId| p.pid != 10;
        assert!(b.lookup(&m, None, 201, &ended).is_none());
        assert!(b.is_empty());
    }

    #[test]
    fn alone_requests_are_never_remembered() {
        let mut b = Book::new(HOUR);
        let m = Mark {
            origin: Origin::Alone(7),
            key: "K".into(),
            reason: String::new(),
        };
        b.remember(&m, None, Answer::Allow, "r1", 0);
        assert!(b.is_empty());
        assert_eq!(b.lookup(&m, None, 1, &everyone), None);
    }

    #[test]
    fn the_terminal_answer_covers_the_commands_of_that_shell_and_key() {
        let mut b = Book::new(HOUR);
        let shell = proc(50);
        b.remember(
            &mark(60, "K", "pull"),
            Some(&shell),
            Answer::AllowTerminal,
            "r1",
            0,
        );
        let later = mark(61, "K", "");
        let d = b.lookup(&later, Some(&shell), 10, &everyone).unwrap();
        assert_eq!((d.id.as_str(), d.answer), ("r1", Answer::AllowTerminal));
        assert_eq!(
            b.lookup(&later, Some(&proc(51)), 10, &everyone),
            None,
            "another terminal"
        );
        assert_eq!(
            b.lookup(&mark(61, "K2", ""), Some(&shell), 10, &everyone),
            None,
            "another key"
        );
        assert_eq!(
            b.lookup(&later, None, 10, &everyone),
            None,
            "not a person's command"
        );
        b.remember(&mark(70, "K", ""), Some(&shell), Answer::Allow, "r2", 20);
        assert_eq!(
            b.lookup(&mark(71, "K", ""), Some(&shell), 21, &everyone)
                .map(|d| d.id),
            Some("r1".to_owned()),
            "a plain Allow covers its call only"
        );
        let shell_gone = |p: &ProcId| p.pid != 50;
        assert_eq!(b.lookup(&later, Some(&shell), 30, &shell_gone), None);
    }

    #[test]
    fn a_sweep_forgets_what_ran_out() {
        let mut b = Book::new(HOUR);
        b.remember(&mark(1, "K", ""), None, Answer::Allow, "a", 0);
        b.remember(&mark(2, "K", ""), None, Answer::Allow, "b", HOUR / 2);
        b.remember(
            &mark(3, "K", ""),
            Some(&proc(9)),
            Answer::AllowTerminal,
            "c",
            HOUR / 2,
        );
        assert_eq!(b.len(), 4, "two calls, a terminal and its call");
        b.sweep(HOUR, &|p| p.pid != 2);
        assert_eq!(b.len(), 2, "1 timed out, 2 ended; 3 and its terminal stay");
        b.sweep(HOUR, &|p| p.pid != 9);
        assert_eq!(b.len(), 1, "the terminal's shell ended");
    }

    #[test]
    fn one_window_at_a_time_in_order_and_one_per_call() {
        let mut q = Queue::default();
        let a = mark(1, "K", "a");
        let Join::Ask(ta) = q.join(Some(&a)) else {
            panic!("the first one asks")
        };
        let Join::Ask(tb) = q.join(Some(&mark(2, "K", "b"))) else {
            panic!("another call asks too")
        };
        assert_eq!(q.join(Some(&a)), Join::Follow(ta), "the same call follows");
        let Join::Ask(tc) = q.join(None) else {
            panic!("a window that asks nothing never follows")
        };
        assert_ne!(q.join(None), Join::Ask(tc));
        assert!(!q.turn(tb), "not first");
        assert!(q.turn(ta));
        assert!(q.turn(ta), "still its turn");
        assert_eq!(q.waiting(), 3, "b, c and the second window without a mark");
        assert_eq!(
            q.join(Some(&a)),
            Join::Follow(ta),
            "following the current one"
        );
        assert!(!q.turn(tb), "a's window is open");
        q.done(ta);
        assert!(!q.active(ta));
        assert!(q.active(tc));
        q.done(tc);
        assert!(!q.active(tc), "a waiting ticket can leave");
        assert!(q.turn(tb));
        assert_eq!(q.waiting(), 1);
    }

    #[test]
    fn the_ticket_that_asks_for_a_call_is_found() {
        let mut q = Queue::default();
        let a = mark(1, "K", "a");
        assert_eq!(q.asking(&a), None);
        let Join::Ask(t) = q.join(Some(&a)) else {
            panic!("asks")
        };
        assert_eq!(q.asking(&a), Some(t), "waiting");
        assert!(q.turn(t));
        assert_eq!(q.asking(&a), Some(t), "current");
        assert_eq!(q.asking(&mark(2, "K", "a")), None);
        q.done(t);
        assert_eq!(q.asking(&a), None);
    }
}

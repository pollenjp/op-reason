//! One connection to the relay (design §8): requests are read one at a time and passed on unchanged to the 1Password
//! agent, on a connection of their own opened at the first message (so a connection that closes without a message
//! leaves no trace). Sign requests are decided first (per-call) or shown while 1Password answers (show-only); a request
//! that 1Password takes more than a second to answer gets the window without buttons, so that its prompt never stands
//! alone. Every signature is recorded. A signature that an earlier answer let through without a window gets a toast
//! (design `2026-11-18-pass-toasts-design.md` §4.1).

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use op_reason_core::args;
use op_reason_core::contract::{Herdr, Place};
use op_reason_core::log::{self, Caller as LogCaller};
use op_reason_core::place;
use op_reason_core::ssh::call::{self, REASON_MAX};
use op_reason_core::ssh::cmdline;
use op_reason_core::ssh::decide::{Answer, Mark, Origin, ProcId};
use op_reason_core::ssh::record::{CallRec, ClientRec, HostRec, KeyRec, ProcRec, PurposeRec, Sign};
use op_reason_core::ssh::roots::{self, Node, Os, Rule};
use op_reason_core::ssh::wire::{self, Purpose, SessionBind, SignRequest};
use op_reason_core::time;

use super::ask::{self, Outcome, Shown, Ticket};
use super::{Shared, peer, pipe};
use crate::relay_opts::Mode;
use crate::relay_ui::{self, Caller, Facts, Forwarded, Request, Unknown};
use crate::win::{procs, toast};

/// How long 1Password may take before the window without buttons comes (§5.5).
const SLOW: Duration = Duration::from_secs(1);
/// A show-only signature faster than this had no prompt (§6).
const NO_PROMPT: f64 = 0.5;
/// How long the relay waits for 1Password's answer to one request: past this it gives up on the connection, so that a
/// stalled agent cannot hold the turn of the windows for ever.
const REPLY_LIMIT: Duration = Duration::from_secs(300);
/// The longest name, user, namespace or directory shown in the window and kept in a record.
const VALUE_MAX: usize = 200;

/// Serves one client until it closes the connection.
pub(super) fn serve(shared: Arc<Shared>, server: pipe::Server) {
    let client_pid = server.client_pid();
    let client = server.into_file();
    let mut c = Conn {
        shared: &shared,
        client,
        client_pid,
        upstream: Upstream::NotYet,
        binds: Binds::default(),
        who: None,
    };
    c.run();
}

/// The connection to the 1Password agent.
enum Upstream {
    NotYet,
    Up(File),
    Missing,
}

/// A request's exchange with the 1Password agent: what came back, in how many seconds, and whether the window without
/// buttons came meanwhile.
struct Exchanged {
    reply: Reply,
    secs: f64,
    late_window: bool,
}

/// What came back from the 1Password agent.
enum Reply {
    /// A message body.
    Body(Vec<u8>),
    /// The agent closed the connection without an answer.
    Closed,
    /// No agent (its pipe is missing).
    Missing,
}

/// What `session-bind@openssh.com` said on this connection: the host key, or for a forwarding the remote host's key
/// (the first bind with the flag) and the remote client's destination (a later bind without it; design §15, C4).
#[derive(Default)]
struct Binds {
    host: Option<String>,
    forwarded_from: Option<String>,
    onward: Option<String>,
}

impl Binds {
    fn note(&mut self, b: &SessionBind) {
        let fp = wire::fingerprint(&b.host_key);
        if b.forwarding {
            self.forwarded_from.get_or_insert(fp);
        } else if self.forwarded_from.is_some() {
            self.onward = Some(fp);
        } else {
            self.host = Some(fp);
        }
    }
}

/// Who asks: found once per connection from the client's process.
#[derive(Clone)]
struct Who {
    /// The root of the call; `None`: nobody the relay can see (every request is a call of its own).
    root: Option<ProcId>,
    /// Rule 2: the interactive shell.
    shell: Option<ProcId>,
    rule: &'static str,
    caller: Caller,
    caller_rec: Option<LogCaller>,
    session: Option<String>,
    root_line: Option<String>,
    root_rec: Option<ProcRec>,
    shell_rec: Option<ProcRec>,
    distro: Option<String>,
    herdr: Option<Herdr>,
    place: Option<Place>,
    reason: String,
    client: ClientRec,
    command: String,
    destination: Option<String>,
    /// The client is Windows OpenSSH's `ssh.exe` (it binds every connection it uses for a host, from OpenSSH 8.9 on).
    is_ssh: bool,
}

struct Conn<'a> {
    shared: &'a Shared,
    client: File,
    client_pid: Option<u32>,
    upstream: Upstream,
    binds: Binds,
    who: Option<Who>,
}

/// How to wait for 1Password's answer.
enum Watch<'s, 'a> {
    /// Quietly; the window without buttons after a second.
    Late(Box<Facts>),
    /// While this window stays open.
    Shown(&'s mut Shown<'a>),
}

/// A late window: none yet, waiting for its turn, or shown.
enum Late<'a> {
    Idle,
    Waiting(Ticket<'a>),
    Shown(Box<Shown<'a>>),
    /// The window could not be shown: not tried again for this request.
    Failed,
}

impl<'a> Conn<'a> {
    fn run(&mut self) {
        loop {
            let msg = match wire::read_message(&mut self.client) {
                Ok(Some(m)) => m,
                _ => return,
            };
            let reply = if msg[0] == wire::SIGN_REQUEST {
                self.sign(&msg)
            } else {
                if let Some(b) = wire::session_bind(&msg) {
                    self.binds.note(&b);
                }
                self.pass(&msg)
            };
            let Some(reply) = reply else { return };
            if self.client.write_all(&reply).is_err() {
                return;
            }
        }
    }

    /// The agent's connection, opened at the first request (each client connection has its own).
    fn upstream(&mut self) -> Option<&mut File> {
        if matches!(self.upstream, Upstream::NotYet) {
            self.upstream = match pipe::connect(&self.shared.opts.upstream) {
                Ok(f) => Upstream::Up(f),
                Err(_) => Upstream::Missing,
            };
        }
        match &mut self.upstream {
            Upstream::Up(f) => Some(f),
            _ => None,
        }
    }

    /// Sends a request to the agent and waits for its answer. Returns the answer and the seconds it took.
    /// Sends one message to the agent; a connection that 1Password has closed since is opened again once.
    fn send(&mut self, frame: &[u8]) -> Result<(), Reply> {
        for _ in 0..2 {
            let Some(up) = self.upstream() else {
                return Err(Reply::Missing);
            };
            if up.write_all(frame).is_ok() {
                return Ok(());
            }
            self.upstream = Upstream::NotYet;
        }
        Err(Reply::Closed)
    }

    fn exchange(&mut self, msg: &[u8], mut watch: Watch<'_, 'a>) -> Exchanged {
        let shared = self.shared;
        let at_once = |reply| Exchanged {
            reply,
            secs: 0.0,
            late_window: false,
        };
        if let Err(r) = self.send(&wire::frame(msg)) {
            return at_once(r);
        }
        let Upstream::Up(up) = &mut self.upstream else {
            return at_once(Reply::Missing);
        };
        let t0 = Instant::now();
        let mut late = Late::Idle;
        let mut stalled = false;
        loop {
            match pipe::available(up) {
                Ok(n) if n >= 4 => break,
                Ok(_) => {}
                Err(_) => break,
            }
            if t0.elapsed() >= REPLY_LIMIT {
                stalled = true;
                break;
            }
            match (&mut watch, &mut late) {
                (Watch::Shown(s), _) => s.pump(Duration::from_millis(10)),
                (Watch::Late(_), Late::Shown(s)) => s.pump(Duration::from_millis(10)),
                (Watch::Late(facts), state) => {
                    if let Late::Waiting(ticket) = state {
                        if ask::turn(ticket) {
                            let Late::Waiting(ticket) = std::mem::replace(state, Late::Idle) else {
                                unreachable!("matched above")
                            };
                            *state = match ask::late(shared, ticket, (**facts).clone()) {
                                Some(s) => Late::Shown(Box::new(s)),
                                None => Late::Failed,
                            };
                        }
                    } else if t0.elapsed() >= SLOW && matches!(state, Late::Idle) {
                        *state = Late::Waiting(ask::join(shared));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        let late_window = matches!(late, Late::Shown(_));
        drop(late);
        if stalled {
            self.upstream = Upstream::NotYet;
            return Exchanged {
                reply: Reply::Closed,
                secs: t0.elapsed().as_secs_f64(),
                late_window,
            };
        }
        let reply = match wire::read_message(up) {
            Ok(Some(body)) => Reply::Body(body),
            _ => {
                self.upstream = Upstream::NotYet;
                Reply::Closed
            }
        };
        Exchanged {
            reply,
            secs: t0.elapsed().as_secs_f64(),
            late_window,
        }
    }

    /// A request other than a signature: passed on unchanged (the keys' comments are read from the answer to the key
    /// list).
    fn pass(&mut self, msg: &[u8]) -> Option<Vec<u8>> {
        let request = match msg[0] {
            wire::REQUEST_IDENTITIES => Request::List,
            kind => Request::Other(kind),
        };
        let facts = self.facts(request);
        let reply = self.exchange(msg, Watch::Late(Box::new(facts))).reply;
        Some(match reply {
            Reply::Body(body) => {
                if let Some(keys) = wire::identities(&body) {
                    let mut st = self.shared.lock();
                    for (blob, comment) in keys {
                        st.comments.insert(wire::fingerprint(&blob), comment);
                    }
                }
                wire::frame(&body)
            }
            _ => wire::failure(),
        })
    }

    fn who(&mut self) -> Who {
        if self.who.is_none() {
            self.who = Some(identify(self.client_pid));
        }
        self.who
            .clone()
            .unwrap_or_else(|| unknown(Unknown::Unreadable))
    }

    /// The window's facts for a request on this connection.
    fn facts(&mut self, request: Request) -> Facts {
        let who = self.who();
        let o = &self.shared.opts;
        let forwarded = self.binds.forwarded_from.as_ref().map(|k| Forwarded {
            from: who.destination.clone().unwrap_or_else(|| "?".to_owned()),
            from_key: Some(k.clone()),
            onward_key: self.binds.onward.clone(),
        });
        let is_forwarded = forwarded.is_some();
        let single = !is_forwarded && who.root.is_none()
            || who.is_ssh && self.binds.host.is_none() && !is_forwarded;
        Facts {
            time: time::now().ymd_hms,
            mode: o.mode,
            caller: if is_forwarded {
                Caller::Unknown(Unknown::Forwarded)
            } else {
                who.caller.clone()
            },
            root: if is_forwarded {
                None
            } else {
                who.root_line.clone()
            },
            herdr: if is_forwarded {
                None
            } else {
                who.herdr.as_ref().map(|h| Herdr {
                    workspace: tidy(&h.workspace),
                    label: tidy(&h.label),
                    pane: tidy(&h.pane),
                })
            },
            distro: if is_forwarded {
                None
            } else {
                who.distro.as_deref().map(tidy)
            },
            place: if is_forwarded {
                None
            } else {
                who.place.as_ref().map(|p| Place {
                    repo: p.repo.as_deref().map(tidy),
                    branch: p.branch.as_deref().map(tidy),
                    cwd: tidy(&p.cwd),
                })
            },
            reason: if is_forwarded {
                String::new()
            } else {
                who.reason.clone()
            },
            command: who.command.clone(),
            destination: who.destination.as_deref().map(tidy),
            host_key: self.binds.host.clone(),
            forwarded,
            request,
            limit_minutes: o.limit_secs.div_ceil(60),
            waiting: 0,
            terminal: false,
            single,
        }
    }

    fn sign(&mut self, msg: &[u8]) -> Option<Vec<u8>> {
        let t0 = Instant::now();
        let id = self.shared.new_id();
        let Some(req) = wire::sign_request(msg) else {
            let who = self.who();
            self.record(&who, &id, None, "malformed", None, None, t0);
            return Some(wire::failure());
        };
        let fp = wire::fingerprint(&req.key);
        let comment = self.shared.lock().comments.get(&fp).map(|c| tidy(c));
        let mut facts = self.facts(Request::Sign {
            purpose: tidy_purpose(wire::purpose(&req.data)),
            comment,
            fingerprint: fp.clone(),
        });
        let who = self.who();
        // No 1Password agent: nothing to ask about (design §8)
        if self.upstream().is_none() {
            self.record(&who, &id, Some(&req), "no-upstream", None, None, t0);
            return Some(wire::failure());
        }
        if self.shared.opts.mode == Mode::ShowOnly {
            return self.show_only(msg, &who, facts, &req, &id, t0);
        }
        let forwarded = self.binds.forwarded_from.is_some();
        // An ssh.exe connection that signs before any session-bind@openssh.com: an OpenSSH older than 8.9, whose
        // forwarded requests cannot be told from its own (design §5.3)
        let unbound = self.unbound(&who);
        let origin = match (&who.root, forwarded || unbound) {
            (Some(root), false) => Origin::Root(root.clone()),
            _ => Origin::Alone(self.shared.unique()),
        };
        let shell = if forwarded || unbound {
            None
        } else {
            who.shell.clone()
        };
        facts.terminal = shell.is_some();
        let mark = Mark {
            origin,
            key: fp,
            reason: facts.reason.clone(),
        };
        let shared = self.shared;
        match ask::per_call(
            shared,
            &mark,
            shell.as_ref(),
            &self.client,
            facts.clone(),
            &id,
        ) {
            Outcome::Passed(d) => {
                let toast = relay_ui::pass_texts(&facts, d.answer, shared.lang());
                let ex = self.exchange(msg, Watch::Late(Box::new(facts)));
                // signed without a window: the toast (not after the window without buttons, which showed it)
                if signed(&ex.reply)
                    && !ex.late_window
                    && let Some(toasts) = shared.toasts()
                {
                    toasts.pass(mark.clone(), &d.id, toast);
                }
                self.finish(&who, &id, &req, ex.reply, "passed", None, Some(&d.id), t0)
            }
            Outcome::Refused(d) => {
                self.record(&who, &id, Some(&req), "denied", None, Some(&d.id), t0);
                Some(wire::failure())
            }
            Outcome::Asked(answer, mut shown) => {
                if answer.allows() {
                    let reply = self.exchange(msg, Watch::Shown(&mut shown)).reply;
                    drop(shown);
                    self.finish(&who, &id, &req, reply, "allowed", Some(answer), None, t0)
                } else {
                    drop(shown);
                    self.record(&who, &id, Some(&req), "denied", Some(answer), None, t0);
                    Some(wire::failure())
                }
            }
            other => self.unanswered(&who, &id, &req, other, t0),
        }
    }

    /// show-only: the window without buttons first, then the request to 1Password, which decides.
    fn show_only(
        &mut self,
        msg: &[u8],
        who: &Who,
        facts: Facts,
        req: &SignRequest,
        id: &str,
        t0: Instant,
    ) -> Option<Vec<u8>> {
        let shared = self.shared;
        let mut shown = match ask::show_only(shared, &self.client, facts) {
            Ok(s) => s,
            Err(o) => return self.unanswered(who, id, req, o, t0),
        };
        let lang = shown.lang();
        let ex = self.exchange(msg, Watch::Shown(&mut shown));
        drop(shown);
        let fast = signed(&ex.reply) && ex.secs < NO_PROMPT;
        let out = self.finish(who, id, req, ex.reply, "signed", None, None, t0);
        if fast && !shared.told_no_prompt.swap(true, Ordering::SeqCst) {
            let _ = toast::start(relay_ui::no_prompt_toast(lang));
            shared.relay_record("no-prompt", None);
        }
        out
    }

    /// The record and the reply of a request that went to 1Password: `ok` when it signed.
    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        who: &Who,
        id: &str,
        req: &SignRequest,
        reply: Reply,
        ok: &'static str,
        answer: Option<Answer>,
        decision: Option<&str>,
        t0: Instant,
    ) -> Option<Vec<u8>> {
        let (result, out) = match reply {
            Reply::Body(body) if body.first() == Some(&wire::SIGN_RESPONSE) => {
                (ok, wire::frame(&body))
            }
            Reply::Body(body) => ("failed", wire::frame(&body)),
            Reply::Closed => {
                self.no_answer();
                ("no-answer", wire::failure())
            }
            Reply::Missing => ("no-upstream", wire::failure()),
        };
        self.record(who, id, Some(req), result, answer, decision, t0);
        Some(out)
    }

    /// Whether a sign request comes from `ssh.exe` on a connection that carried no `session-bind@openssh.com`.
    fn unbound(&self, who: &Who) -> bool {
        who.is_ssh && self.binds.host.is_none() && self.binds.forwarded_from.is_none()
    }

    /// 1Password closed a request without an answer: a toast once per run, which names the likely cause when the
    /// relay's parent is gone (design §15).
    fn no_answer(&self) {
        let s = self.shared;
        if !s.told_no_answer.swap(true, Ordering::SeqCst) {
            let _ = toast::start(relay_ui::no_answer_toast(
                s.lang(),
                s.parent_gone.load(Ordering::SeqCst),
            ));
        }
    }

    /// A request that never reached 1Password.
    fn unanswered(
        &mut self,
        who: &Who,
        id: &str,
        req: &SignRequest,
        outcome: Outcome<'_>,
        t0: Instant,
    ) -> Option<Vec<u8>> {
        let result = match outcome {
            Outcome::Timeout => "timeout",
            Outcome::Dismissed => "dismissed",
            Outcome::Gone => "gone",
            Outcome::LockTimeout => "lock-timeout",
            Outcome::NoWindow => "no-window",
            Outcome::Passed(_) | Outcome::Refused(_) | Outcome::Asked(..) => "failed",
        };
        self.record(who, id, Some(req), result, None, None, t0);
        (result != "gone").then(wire::failure)
    }

    /// Writes the `ssh-sign` record of a request. One that an earlier answer covered refers to that answer's record.
    #[allow(clippy::too_many_arguments)]
    fn record(
        &self,
        who: &Who,
        id: &str,
        req: Option<&SignRequest>,
        result: &str,
        answer: Option<Answer>,
        decision: Option<&str>,
        t0: Instant,
    ) {
        let now = time::now();
        let forwarded = self.binds.forwarded_from.is_some();
        let full = decision.is_none();
        let (key, purpose) = match req {
            Some(r) => {
                let fp = wire::fingerprint(&r.key);
                let comment = self.shared.lock().comments.get(&fp).map(|c| tidy(c));
                let purpose = match tidy_purpose(wire::purpose(&r.data)) {
                    Purpose::UserAuth { user } => PurposeRec {
                        kind: "userauth".into(),
                        user: Some(user),
                        namespace: None,
                    },
                    Purpose::Sshsig { namespace } => PurposeRec {
                        kind: "sshsig".into(),
                        user: None,
                        namespace: Some(namespace),
                    },
                    Purpose::Other => PurposeRec {
                        kind: "other".into(),
                        ..PurposeRec::default()
                    },
                };
                (
                    KeyRec {
                        fp,
                        comment,
                        kind: wire::key_type(&r.key),
                    },
                    purpose,
                )
            }
            None => (KeyRec::default(), PurposeRec::default()),
        };
        let host = if forwarded {
            Some(HostRec {
                name: who.destination.as_deref().map(tidy),
                fp: self.binds.forwarded_from.clone(),
                onward: self.binds.onward.clone(),
            })
        } else if who.destination.is_some() || self.binds.host.is_some() {
            Some(HostRec {
                name: who.destination.as_deref().map(tidy),
                fp: self.binds.host.clone(),
                onward: None,
            })
        } else {
            None
        };
        let rule = if forwarded {
            "alone"
        } else if self.unbound(who) && req.is_some() {
            "unbound"
        } else {
            who.rule
        };
        let r = Sign {
            v: 1,
            id: id.to_owned(),
            event: "ssh-sign",
            time: now.iso,
            epoch: now.epoch,
            mode: self.shared.opts.mode.as_str().to_owned(),
            result: result.to_owned(),
            seconds: (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            answer: answer.map(|a| a.as_str().to_owned()),
            decision: decision.map(str::to_owned),
            call: full.then(|| CallRec {
                rule: rule.to_owned(),
                root: if forwarded {
                    None
                } else {
                    who.root_rec.clone()
                },
                shell: if forwarded {
                    None
                } else {
                    who.shell_rec.clone()
                },
                distro: who.distro.clone(),
            }),
            caller: if full && !forwarded {
                who.caller_rec.clone()
            } else {
                None
            },
            session: if full && !forwarded {
                who.session.clone()
            } else {
                None
            },
            reason: full.then(|| {
                if forwarded {
                    String::new()
                } else {
                    who.reason.clone()
                }
            }),
            client: who.client.clone(),
            key,
            purpose,
            host,
            forwarded,
        };
        self.shared.record(log::line(&r));
    }
}

/// Whether 1Password signed.
fn signed(reply: &Reply) -> bool {
    matches!(reply, Reply::Body(b) if b.first() == Some(&wire::SIGN_RESPONSE))
}

/// A caller the relay cannot see.
fn unknown(why: Unknown) -> Who {
    Who {
        root: None,
        shell: None,
        rule: "alone",
        caller: Caller::Unknown(why),
        caller_rec: None,
        session: None,
        root_line: None,
        root_rec: None,
        shell_rec: None,
        distro: None,
        herdr: None,
        place: None,
        reason: String::new(),
        client: ClientRec::default(),
        command: String::new(),
        destination: None,
        is_ssh: false,
    }
}

/// A value from another process for the window and the records: control characters as spaces, at most [`VALUE_MAX`]
/// characters.
fn tidy(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    args::clip(&flat, VALUE_MAX, "…")
}

fn tidy_purpose(p: Purpose) -> Purpose {
    match p {
        Purpose::UserAuth { user } => Purpose::UserAuth { user: tidy(&user) },
        Purpose::Sshsig { namespace } => Purpose::Sshsig {
            namespace: tidy(&namespace),
        },
        Purpose::Other => Purpose::Other,
    }
}

fn env_nonempty(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// A normalized reason, at most 500 characters.
fn reason_of(text: &str) -> String {
    args::clip(&args::normalize(text), REASON_MAX, "…")
}

/// Finds who asks: the chain of the client's process, the root of its call (§5.2) and what the client's environment
/// and directory tell (§7).
fn identify(pid: Option<u32>) -> Who {
    let Some(pid) = pid else {
        return unknown(Unknown::Unreadable);
    };
    let chain = peer::chain(pid);
    let Some(first) = chain.first() else {
        return Who {
            client: ClientRec {
                pid,
                ..ClientRec::default()
            },
            ..unknown(Unknown::Unreadable)
        };
    };
    let mut nodes: Vec<Node> = chain
        .iter()
        .map(|l| Node {
            name: l.name.clone(),
            args: l
                .cmd
                .as_deref()
                .map(cmdline::split_windows)
                .unwrap_or_default(),
        })
        .collect();
    // C:\Windows\System32\bash.exe starts WSL: it is an interop host, not a shell
    if let Some(parent) = chain.get(1)
        && parent.name.eq_ignore_ascii_case("bash.exe")
        && procs::image_path(parent.pid)
            .is_some_and(|p| p.to_ascii_lowercase().ends_with("\\system32\\bash.exe"))
    {
        nodes[1].name = "wsl.exe".to_owned();
    }
    let client_args = &nodes[0].args;
    let client = ClientRec {
        pid,
        exe: procs::image_path(pid).unwrap_or_else(|| first.name.clone()),
        cmd: cmdline::shown(client_args, call::CMD_MAX),
    };
    let command = cmdline::shown(client_args, 300);
    let destination = first
        .name
        .eq_ignore_ascii_case("ssh.exe")
        .then(|| cmdline::destination(client_args).map(|d| d.text()))
        .flatten();
    let found = roots::find(Os::Windows, &nodes);
    let inside = peer::inside(pid).unwrap_or_default();
    let base = Who {
        client,
        command,
        destination,
        is_ssh: first.name.eq_ignore_ascii_case("ssh.exe"),
        ..unknown(Unknown::Unreadable)
    };
    if found.rule == Rule::Wsl {
        return match inside.var(call::ENV_CALL).and_then(call::decode) {
            Some(c) => from_wsl(base, c),
            None => Who {
                caller: Caller::Unknown(Unknown::WslDirect),
                ..base
            },
        };
    }
    let Some(root) = chain.get(found.root) else {
        return base;
    };
    let proc_rec = |i: usize| {
        chain.get(i).map(|l| ProcRec {
            pid: l.pid,
            start: l.start,
            name: l.name.clone(),
            cmd: cmdline::shown(&nodes[i].args, call::CMD_MAX),
        })
    };
    // A process whose start time cannot be read could be a reused PID: never remembered by it
    let id_of = |i: usize| {
        chain.get(i).filter(|l| l.start != 0).map(|l| ProcId {
            scope: String::new(),
            pid: l.pid,
            start: l.start,
        })
    };
    // The answer for the terminal: a command a person types, under a shell whose command line could be read
    let terminal = found
        .shell
        .filter(|&i| roots::terminal_command(Os::Windows, &root.name) && !nodes[i].args.is_empty());
    let claude_pid = match found.rule {
        Rule::Claude => chain.get(found.root + 1).map(|l| l.pid),
        _ => None,
    };
    let (kind, caller) = match found.rule {
        Rule::Claude => ("claude", Caller::Claude { pid: claude_pid }),
        Rule::Shell => (
            "human",
            Caller::Person {
                shell: found
                    .shell
                    .and_then(|i| chain.get(i))
                    .map(|l| l.name.clone())
                    .unwrap_or_default(),
            },
        ),
        _ => (
            "program",
            Caller::Program {
                name: root.name.clone(),
            },
        ),
    };
    let herdr = (inside.var("HERDR_ENV") == Some("1")).then(|| Herdr {
        workspace: inside.var("HERDR_WORKSPACE_ID").unwrap_or("").to_owned(),
        label: String::new(),
        pane: inside.var("HERDR_PANE_ID").unwrap_or("").to_owned(),
    });
    let cwd = inside.cwd.clone().unwrap_or_default();
    let info = if cwd.is_empty() {
        place::PlaceInfo::default()
    } else {
        place::detect(Path::new(&cwd))
    };
    let shown_cwd = place::tilde(&cwd, env_nonempty("USERPROFILE").as_deref());
    Who {
        root: id_of(found.root),
        shell: terminal.and_then(id_of),
        rule: found.rule.as_str(),
        caller,
        caller_rec: Some(LogCaller {
            kind: kind.to_owned(),
            claude_pid,
            herdr: herdr.clone(),
            cwd: cwd.clone(),
            repo: info.repo.clone(),
            branch: info.branch.clone(),
        }),
        session: inside
            .var("CLAUDE_CODE_SESSION_ID")
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        root_line: Some(format!(
            "{} (pid {}) · {}",
            root.name,
            root.pid,
            cmdline::shown(&nodes[found.root].args, 200)
        )),
        root_rec: proc_rec(found.root),
        shell_rec: found.shell.and_then(proc_rec),
        distro: None,
        herdr,
        place: (!cwd.is_empty()).then_some(Place {
            repo: info.repo,
            branch: info.branch,
            cwd: shown_cwd,
        }),
        reason: reason_of(inside.var(call::ENV_REASON).unwrap_or("")),
        ..base
    }
}

/// The facts of a WSL call that `op-reason-ssh` collected (taken on trust, design §10).
fn from_wsl(base: Who, c: call::WslCall) -> Who {
    let scope = format!("wsl:{}", c.distro);
    let id_of = |p: &call::Proc| ProcId {
        scope: scope.clone(),
        pid: p.pid,
        start: p.start,
    };
    let rec_of = |p: &call::Proc| ProcRec {
        pid: p.pid,
        start: p.start,
        name: p.name.clone(),
        cmd: args::clip(&p.cmd, call::CMD_MAX, "…"),
    };
    let caller = match c.rule.as_str() {
        "claude" => Caller::Claude {
            pid: c.caller.claude_pid,
        },
        "shell" => Caller::Person {
            shell: c.shell.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
        },
        _ => Caller::Program {
            name: c.root.name.clone(),
        },
    };
    let rule = match c.rule.as_str() {
        "claude" => "claude",
        "shell" => "shell",
        _ => "parent",
    };
    Who {
        root: Some(id_of(&c.root)),
        shell: (rule == "shell" && roots::terminal_command(Os::Linux, &c.root.name))
            .then(|| c.shell.as_ref().filter(|s| s.start != 0).map(id_of))
            .flatten(),
        rule,
        caller,
        caller_rec: Some(c.caller.clone()),
        session: c.session.clone(),
        root_line: Some(format!(
            "{} (pid {}) · {}",
            c.root.name,
            c.root.pid,
            args::clip(&c.root.cmd, 200, "…")
        )),
        root_rec: Some(rec_of(&c.root)),
        shell_rec: c.shell.as_ref().map(rec_of),
        distro: Some(c.distro.clone()),
        herdr: c.caller.herdr.clone(),
        place: Some(Place {
            repo: c.caller.repo.clone(),
            branch: c.caller.branch.clone(),
            cwd: c.caller.cwd.clone(),
        }),
        reason: reason_of(&c.reason),
        ..base
    }
}

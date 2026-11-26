//! The texts of the SSH relay's window and toasts (design `2026-11-17-ssh-relay-design.md` §6), in English and
//! Japanese. The window is `op-reason.exe`'s ([`crate::ui::Texts`] with buttons), and draws the [`View`] built here
//! (design `2026-11-26-modern-window-design.md` §4.3): the reason first, chips for who and where, the root command in
//! the box, what the signature is for, and the band.
//!
//! - [`Phase::Ask`] (per-call): "SSH key: allow this call?" with who, what and why, the seconds left to answer, how long
//!   an answer lasts (tied to the root command, §4.8), and the buttons.
//! - [`Phase::Asking`]: the same facts without buttons while 1Password's prompt may be open (show-only, after a click on
//!   Allow, or a request that 1Password is slow to answer), with a band that says where 1Password keeps a prompt it did
//!   not show (design `2026-11-22-front-after-allow-design.md` §6).
//! - After a click that allows, the relay watches for 1Password's prompt (design `2026-11-26-prompt-guide-design.md`):
//!   [`Phase::Seen`] when it came ("Approve in 1Password's window", with a sketch of its question), [`Phase::Hidden`]
//!   when it has not come within two seconds (the band in the alert colors, the seconds until 1Password closes the
//!   request), with the pointer to the tray ([`pointer_texts`]).
//!
//! And the toast of a signature that an earlier answer let through without a window ([`pass_texts`]; design
//! `2026-11-18-pass-toasts-design.md` §4.3).

use op_reason_core::args;
use op_reason_core::contract::{Herdr, Place};
use op_reason_core::lang::Lang;
use op_reason_core::ssh::decide::Answer;
use op_reason_core::ssh::wire::Purpose;

use crate::look::{self, Band, Chip, ChipKind, Fact, FactKind, Icon, Reason, Sketch, View};
use crate::relay_opts::Mode;
use crate::ui::{self, Both, Button, Texts};

/// Who asks, as the relay found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    /// Claude Code (rule 1), with its PID when known.
    Claude { pid: Option<u32> },
    /// A person in a terminal (rule 2): the shell's name.
    Person { shell: String },
    /// A program that runs ssh itself (rule 3): its name.
    Program { name: String },
    /// Nobody the relay can see.
    Unknown(Unknown),
}

/// Why the caller is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unknown {
    /// The connection carries requests forwarded from a remote host.
    Forwarded,
    /// A WSL `ssh.exe` that was not started through `op-reason-ssh`.
    WslDirect,
    /// The client's process could not be read.
    Unreadable,
}

/// A forwarded connection: the remote host (the local ssh's destination) and the two host keys (design §15, C4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forwarded {
    pub from: String,
    pub from_key: Option<String>,
    pub onward_key: Option<String>,
}

/// What is asked of the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// A signature: what for, and the key (its comment in 1Password, and its fingerprint).
    Sign {
        purpose: Purpose,
        comment: Option<String>,
        fingerprint: String,
    },
    /// The key list.
    List,
    /// Another request, by its message number.
    Other(u8),
}

/// The root process of a call: its name, PID and command line (masked, at most 1000 characters, as the record keeps
/// it: the command that Claude Code's wrapper runs comes after some 300 characters of it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    pub name: String,
    pub pid: u32,
    pub cmd: String,
    /// The launcher of Claude Code's PowerShell tool, which holds no command (design §4.7, rule 3): the relay tells it
    /// from the root's own arguments (`look::claude_powershell`), for a root under Claude Code only.
    pub claude_powershell: bool,
}

/// How much of a root's command line the window shows when it shows the line itself.
const ROOT_SHOWN: usize = 200;

impl Root {
    /// The command line as the window shows it when nothing comes off it: at most 200 characters.
    pub fn shown(&self) -> String {
        args::clip(&self.cmd, ROOT_SHOWN, "…")
    }

    /// `<name> (pid <n>)`.
    pub fn process(&self) -> String {
        format!("{} (pid {})", self.name, self.pid)
    }
}

/// What the window shows of one request (masked and clipped already).
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    /// `YYYY-MM-DD HH:MM:SS`
    pub time: String,
    pub mode: Mode,
    pub caller: Caller,
    /// The root process.
    pub root: Option<Root>,
    pub herdr: Option<Herdr>,
    /// WSL calls: the distribution.
    pub distro: Option<String>,
    pub place: Option<Place>,
    /// The reason; empty when none was given.
    pub reason: String,
    /// The client's command line (masked, at most 300 characters).
    pub command: String,
    /// Where the client connects (from its command line).
    pub destination: Option<String>,
    /// The host key's fingerprint (from `session-bind@openssh.com`).
    pub host_key: Option<String>,
    pub forwarded: Option<Forwarded>,
    pub request: Request,
    /// How long an answer covers the call, in minutes.
    pub limit_minutes: u64,
    /// How many other calls wait for a window.
    pub waiting: usize,
    /// Offer the answer that allows the terminal (a person's command).
    pub terminal: bool,
    /// The answer covers this signature only (a connection that cannot be tied to a call).
    pub single: bool,
}

/// Which window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// per-call: the relay asks.
    Ask,
    /// 1Password asks (or may): no buttons.
    Asking,
    /// After a click that allows: 1Password's prompt came.
    Seen,
    /// After a click that allows: 1Password's prompt has not come to the front.
    Hidden,
}

/// The words of the window and the toasts.
struct Table {
    title_ask: &'static str,
    title_asking: &'static str,
    wsl: &'static str,
    reason: &'static str,
    no_reason: &'static str,
    command: &'static str,
    destination: &'static str,
    host_key: &'static str,
    forwarded_from: &'static str,
    onward: &'static str,
    userauth: &'static str,
    sshsig: &'static str,
    other_signature: &'static str,
    list: &'static str,
    other_request: &'static str,
    key: &'static str,
    claude_pid: &'static str,
    claude: &'static str,
    person: &'static str,
    program: &'static str,
    unknown_forwarded: &'static str,
    unknown_wsl: &'static str,
    unknown_unreadable: &'static str,
    allow: &'static str,
    allow_terminal: &'static str,
    deny: &'static str,
    subtitle_per_call: &'static str,
    subtitle_show_only: &'static str,
    root_command: &'static str,
    /// What the root of a call of Claude Code's PowerShell tool is (its launcher, which carries no command).
    claude_powershell: &'static str,
    /// The seconds left to answer (`{0}`).
    countdown: &'static str,
    /// How long an answer lasts (design `2026-11-26-modern-window-design.md` §4.8): the root command's program
    /// (`{0}`) and the time limit (`{1}`); without one program; a request that cannot be tied to a call.
    covers_name: &'static str,
    covers_call: &'static str,
    covers_alone: &'static str,
    waiting: &'static str,
    band_ask_title: &'static str,
    band_ask_line: &'static str,
    /// The band while 1Password asks: per-call after Allow, per-call while 1Password is slow, show-only.
    asking_per_call: &'static str,
    asking_slow: &'static str,
    asking_show_only: &'static str,
    footer_asking: &'static str,
    /// The band's last line while 1Password asks: where it keeps a prompt that it did not show (its label and value).
    tray: &'static str,
    tray_how: &'static str,
    /// The band when 1Password's prompt came (`{0}`: the key) and when it did not; the seconds until 1Password closes the
    /// request (`{0}`).
    seen_title: &'static str,
    seen_line: &'static str,
    hidden_title: &'static str,
    hidden_line: &'static str,
    closes: &'static str,
    /// The pointer to the tray: its title, its line, its seconds (`{0}`) and the item of 1Password's menu in its picture.
    pointer_title: &'static str,
    pointer_line: &'static str,
    pointer_closes: &'static str,
    pointer_item: &'static str,
    pass_word: &'static str,
    pass_call: &'static str,
    pass_terminal: &'static str,
}

const EN: Table = Table {
    title_ask: "SSH key: allow this call?",
    title_asking: "SSH key: 1Password is asking",
    wsl: "WSL",
    reason: "Reason",
    no_reason: "no reason given",
    command: "Command",
    destination: "Destination",
    host_key: "host key {0}",
    forwarded_from: "forwarded from {0}",
    onward: "onward to the host with key {0}",
    userauth: "logging in as {0}",
    sshsig: "a signature (SSHSIG, namespace {0})",
    other_signature: "a signature of another kind",
    list: "listing the keys",
    other_request: "agent request {0}",
    key: "Key",
    claude_pid: "Claude Code (pid {0})",
    claude: "Claude Code",
    person: "Human (terminal)",
    program: "Program: {0}",
    unknown_forwarded: "unknown (forwarded from a remote host)",
    unknown_wsl: "unknown (a WSL ssh.exe not started through op-reason-ssh)",
    unknown_unreadable: "unknown (its process could not be read)",
    allow: "Allow this call",
    allow_terminal: "Allow this terminal for {0} minutes",
    deny: "Deny",
    subtitle_per_call: "op-reason's relay · per-call mode",
    subtitle_show_only: "op-reason's relay · show-only mode",
    root_command: "Root command",
    claude_powershell: "Claude Code's PowerShell tool",
    countdown: "{0} s left to answer",
    covers_name: "Allowing covers this {0} until it ends (at most {1} minutes)",
    covers_call: "Allowing covers this call until it ends (at most {1} minutes)",
    covers_alone: "This answer covers this signature only",
    waiting: " · {0} more waiting",
    band_ask_title: "Allowing brings up 1Password's prompt",
    band_ask_line: "Usually in the ⟦middle of the screen⟧; if not, from the ⟪tray at the bottom right⟫",
    asking_per_call: "Allowed here; 1Password itself may ask now",
    asking_slow: "1Password itself may ask now (to unlock it, for example)",
    asking_show_only: "Decide in 1Password's prompt",
    footer_asking: "Deny any 1Password prompt that comes without this window",
    tray: "If 1Password's prompt does not come",
    tray_how: "the ⟪1Password icon at the right of the taskbar⟫ (under ^ when hidden) → SSH request waiting",
    seen_title: "Approve in 1Password's window",
    seen_line: "It asks whether the SSH key ⟦{0}⟧ may be used. If it went behind other windows: the ⟪tray at the bottom right⟫",
    hidden_title: "1Password's prompt did not come to the front",
    hidden_line: "Open it from the ⟪1Password icon at the right of the taskbar⟫ (under ^ when hidden) → SSH request waiting",
    closes: "1Password closes the request in about {0} s",
    pointer_title: "1Password's prompt is waiting in the tray",
    pointer_line: "1Password icon (under ^ when hidden) → SSH request waiting",
    pointer_closes: "closes in about {0} s",
    pointer_item: "SSH request waiting",
    pass_word: "Passed",
    pass_call: "allowed for this call",
    pass_terminal: "allowed for this terminal",
};

const JA: Table = Table {
    title_ask: "SSH の鍵: この呼び出しを許可しますか",
    title_asking: "SSH の鍵: 1Password が確認しています",
    wsl: "WSL",
    reason: "理由",
    no_reason: "理由なし",
    command: "コマンド",
    destination: "接続先",
    host_key: "ホストの鍵 {0}",
    forwarded_from: "{0} から転送",
    onward: "行き先のホストの鍵 {0}",
    userauth: "{0} としてログイン",
    sshsig: "署名 (SSHSIG、namespace {0})",
    other_signature: "その他の署名",
    list: "鍵の一覧",
    other_request: "agent への依頼 {0}",
    key: "鍵",
    claude_pid: "Claude Code (pid {0})",
    claude: "Claude Code",
    person: "人 (端末)",
    program: "プログラム: {0}",
    unknown_forwarded: "分からない (リモートのホストから転送)",
    unknown_wsl: "分からない (op-reason-ssh を通さずに起きた WSL の ssh.exe)",
    unknown_unreadable: "分からない (process を読めなかった)",
    allow: "この呼び出しを許可",
    allow_terminal: "このターミナルを {0} 分許可",
    deny: "拒否",
    subtitle_per_call: "op-reason の中継 · 呼び出しごとに決めるモード",
    subtitle_show_only: "op-reason の中継 · 表示だけのモード",
    root_command: "起点のコマンド",
    claude_powershell: "Claude Code の PowerShell",
    countdown: "答えを待つ残り {0} 秒",
    covers_name: "許可は、この {0} が終わるまで効きます (長くても {1} 分)",
    covers_call: "許可は、この呼び出しが終わるまで効きます (長くても {1} 分)",
    covers_alone: "この答えはこの署名だけに効きます",
    waiting: " · ほかに {0} 件待っています",
    band_ask_title: "許可すると 1Password の画面が出ます",
    band_ask_line: "ふつうは⟦画面の中央⟧。出ないときは⟪右下のトレイ⟫から",
    asking_per_call: "ここでは許可済み。いまは 1Password 自身が確認することがあります",
    asking_slow: "いまは 1Password 自身が確認することがあります (ロックの解除など)",
    asking_show_only: "決めるのは 1Password の画面です",
    footer_asking: "この窓が出ていない 1Password の確認は拒否してください",
    tray: "1Password の画面が出ないとき",
    tray_how: "⟪タスクバーの右の 1Password のアイコン⟫ (隠れていれば ^ の中) →「SSHリクエストを待機中」",
    seen_title: "1Password の画面で承認してください",
    seen_line: "SSH の鍵 ⟦{0}⟧ を使ってよいかを聞いています。ほかの窓の後ろに隠れたら⟪右下のトレイ⟫から",
    hidden_title: "1Password の画面が前に出ていません",
    hidden_line: "⟪タスクバーの右の 1Password のアイコン⟫ (隠れていれば ^ の中) →「SSHリクエストを待機中」から開いてください",
    closes: "1Password が依頼を閉じるまで 約 {0} 秒",
    pointer_title: "1Password の画面はトレイで待っています",
    pointer_line: "1Password のアイコン (隠れていれば ^ の中) →「SSHリクエストを待機中」",
    pointer_closes: "約 {0} 秒で閉じます",
    pointer_item: "SSHリクエストを待機中",
    pass_word: "通過",
    pass_call: "この呼び出しの許可",
    pass_terminal: "ターミナルの許可",
};

fn table(lang: Lang) -> &'static Table {
    match lang {
        Lang::En => &EN,
        Lang::Ja => &JA,
    }
}

fn fill(template: &str, value: &str) -> String {
    template.replace("{0}", value)
}

/// The texts of one window and its toast in `lang`: the [`View`] the window draws (design
/// `2026-11-26-modern-window-design.md` §4.3), the buttons and the toast. The relay reports no status, so its texts
/// keep no "label: value" lines.
pub fn build(f: &Facts, phase: Phase, lang: Lang) -> Texts {
    let t = table(lang);
    let title = match phase {
        Phase::Ask => t.title_ask,
        Phase::Asking | Phase::Seen | Phase::Hidden => t.title_asking,
    };
    let reason = if f.reason.is_empty() {
        t.no_reason.to_owned()
    } else {
        f.reason.clone()
    };
    let view = view(f, phase, lang);
    let footer = match phase {
        Phase::Ask => view.status.clone(),
        Phase::Asking | Phase::Seen | Phase::Hidden => view.footer.clone(),
    };
    let buttons = match phase {
        Phase::Asking | Phase::Seen | Phase::Hidden => Vec::new(),
        Phase::Ask => {
            let mut b = vec![(Button::Allow, t.allow.to_owned())];
            if f.terminal {
                b.push((
                    Button::AllowTerminal,
                    fill(t.allow_terminal, &f.limit_minutes.to_string()),
                ));
            }
            b.push((Button::Deny, t.deny.to_owned()));
            b
        }
    };
    let toast = vec![
        title.to_owned(),
        ui::table(lang)
            .toast_reason
            .replace("{0}", &ui::clip_utf16(&reason, 100)),
        format!(
            "{} · {}",
            caller_short(&f.caller, lang),
            ui::clip_utf16(&f.command, 80)
        ),
    ];
    Texts {
        title: title.to_owned(),
        lines: Vec::new(),
        refs: None,
        refs_at: 0,
        footer,
        others: String::new(),
        toast,
        buttons,
        view,
    }
}

/// The window's view of a request.
fn view(f: &Facts, phase: Phase, lang: Lang) -> View {
    let t = table(lang);
    let (time, date) = look::split_time(&f.time);
    let alone = f.single || matches!(f.caller, Caller::Unknown(_)) || f.forwarded.is_some();
    // the launcher of Claude Code's PowerShell tool carries no command (its script comes through the environment)
    let powershell = matches!(f.caller, Caller::Claude { .. })
        && f.root.as_ref().is_some_and(|r| r.claude_powershell);
    // the root command in the box, and under it the root process and the rest of its line: the two show all of it;
    // for the PowerShell tool, the ssh command, and under it the root process and what it is
    let (code_label, code, code_note) = match &f.root {
        Some(root) if powershell => (
            t.command,
            f.command.clone(),
            Some(format!("{} · {}", root.process(), t.claude_powershell)),
        ),
        Some(root) => match look::root_command(&root.cmd) {
            Some(u) => (
                t.root_command,
                u.command,
                Some(format!("{} · {}", root.process(), u.frame)),
            ),
            None => (t.root_command, root.shown(), Some(root.process())),
        },
        None => (t.command, f.command.clone(), None),
    };
    let mut facts = vec![Fact {
        kind: FactKind::Purpose,
        text: purpose_line(t, &f.request),
    }];
    let mut detail = Vec::new();
    if let Some(d) = destination_line(t, f) {
        detail.push(format!("{} {d}", t.destination));
    }
    if let Some(k) = key_line(&f.request) {
        detail.push(format!("{} {k}", t.key));
    }
    if !detail.is_empty() {
        facts.push(Fact {
            kind: FactKind::Detail,
            text: detail.join(" · "),
        });
    }
    if f.root.is_some() && !powershell {
        facts.push(Fact {
            kind: FactKind::Mono,
            text: format!("{}: {}", t.command, f.command),
        });
    }
    let waiting = if f.waiting > 0 {
        fill(t.waiting, &f.waiting.to_string())
    } else {
        String::new()
    };
    let limit = f.limit_minutes.to_string();
    let (countdown, status, band, footer) = match phase {
        Phase::Ask => {
            let covers = if alone {
                t.covers_alone.to_owned()
            } else {
                // the PowerShell tool's script is not seen: its call may run more than the ssh command
                let named = f.root.is_some() && !powershell;
                match named.then(|| look::program_name(&code)).flatten() {
                    Some(name) => t.covers_name.replace("{0}", &name).replace("{1}", &limit),
                    None => t.covers_call.replace("{1}", &limit),
                }
            };
            (
                Some(t.countdown.to_owned()),
                format!("{covers}{waiting}"),
                Band {
                    title: t.band_ask_title.to_owned(),
                    line: t.band_ask_line.to_owned(),
                    sketch: Sketch::Ask,
                    keep: false,
                    alert: false,
                },
                String::new(),
            )
        }
        Phase::Asking => {
            let now = match f.mode {
                Mode::PerCall if matches!(f.request, Request::Sign { .. }) => t.asking_per_call,
                Mode::PerCall => t.asking_slow,
                Mode::ShowOnly => t.asking_show_only,
            };
            (
                None,
                String::new(),
                // 1Password keeps the prompt of an application that is not in front behind its icon (design
                // `2026-11-22-front-after-allow-design.md` §6)
                Band {
                    title: now.to_owned(),
                    line: format!("{}: {}", t.tray, t.tray_how),
                    sketch: Sketch::Asking,
                    keep: true,
                    alert: false,
                },
                format!("{}{waiting}", t.footer_asking),
            )
        }
        // after a click that allows (design `2026-11-26-prompt-guide-design.md` §6)
        Phase::Seen => (
            None,
            String::new(),
            Band {
                title: t.seen_title.to_owned(),
                line: t.seen_line.replace("{0}", &key_name(&f.request)),
                sketch: Sketch::Prompt,
                keep: true,
                alert: false,
            },
            format!("{}{waiting}", t.footer_asking),
        ),
        Phase::Hidden => (
            Some(t.closes.to_owned()),
            String::new(),
            Band {
                title: t.hidden_title.to_owned(),
                line: t.hidden_line.to_owned(),
                sketch: Sketch::Tray,
                keep: true,
                alert: true,
            },
            format!("{}{waiting}", t.footer_asking),
        ),
    };
    View {
        icon: Icon::Key,
        subtitle: match f.mode {
            Mode::PerCall => t.subtitle_per_call,
            Mode::ShowOnly => t.subtitle_show_only,
        }
        .to_owned(),
        time,
        date,
        reason_label: t.reason.to_owned(),
        reason: if f.reason.is_empty() {
            Reason::Missing(t.no_reason.to_owned())
        } else {
            Reason::Given(f.reason.clone())
        },
        chips: chips(t, f, lang),
        code_label: code_label.to_owned(),
        code,
        code_note,
        facts,
        notes: Vec::new(),
        countdown,
        status,
        band,
        footer,
    }
}

/// The chips: the caller, the WSL distribution, herdr, the repository and its branch, the directory.
fn chips(t: &Table, f: &Facts, lang: Lang) -> Vec<Chip> {
    let kind = match f.caller {
        Caller::Claude { .. } => ChipKind::Claude,
        Caller::Person { .. } => ChipKind::Person,
        Caller::Program { .. } | Caller::Unknown(_) => ChipKind::Other,
    };
    let mut out = vec![Chip {
        kind,
        text: caller_line(t, &f.caller),
    }];
    if let Some(d) = &f.distro {
        out.push(Chip {
            kind: ChipKind::Wsl,
            text: format!("{} · {d}", t.wsl),
        });
    }
    if let Some(h) = &f.herdr {
        let label = if h.label.is_empty() { "?" } else { &h.label };
        let pane = if h.pane.is_empty() { "?" } else { &h.pane };
        out.push(Chip {
            kind: ChipKind::Herdr,
            text: format!("{} · {label} · {pane}", ui::table(lang).herdr),
        });
    }
    if let Some(p) = &f.place {
        if let Some(repo) = p.repo.as_deref().filter(|r| !r.is_empty()) {
            let branch = p.branch.as_deref().filter(|b| !b.is_empty()).unwrap_or("?");
            out.push(Chip {
                kind: ChipKind::Repo,
                text: format!("{repo} · {branch}"),
            });
        }
        if !p.cwd.is_empty() {
            out.push(Chip {
                kind: ChipKind::Folder,
                text: p.cwd.clone(),
            });
        }
    }
    out
}

/// What the request is for.
fn purpose_line(t: &Table, r: &Request) -> String {
    match r {
        Request::Sign { purpose, .. } => match purpose {
            Purpose::UserAuth { user } => fill(t.userauth, user),
            Purpose::Sshsig { namespace } => fill(t.sshsig, namespace),
            Purpose::Other => t.other_signature.to_owned(),
        },
        Request::List => t.list.to_owned(),
        Request::Other(n) => fill(t.other_request, &n.to_string()),
    }
}

/// The key's name in the band of a prompt that came: its comment in 1Password, else its fingerprint.
fn key_name(r: &Request) -> String {
    match r {
        Request::Sign {
            comment,
            fingerprint,
            ..
        } => comment
            .as_deref()
            .filter(|c| !c.is_empty())
            .unwrap_or(fingerprint)
            .to_owned(),
        _ => "?".to_owned(),
    }
}

/// The texts of the pointer to the tray (design `2026-11-26-prompt-guide-design.md` §6): `closes` has `{0}` for the
/// seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointerTexts {
    pub title: String,
    pub line: String,
    pub closes: String,
    /// The item of 1Password's menu in the pointer's picture.
    pub item: String,
}

pub fn pointer_texts(lang: Lang) -> PointerTexts {
    let t = table(lang);
    PointerTexts {
        title: t.pointer_title.to_owned(),
        line: t.pointer_line.to_owned(),
        closes: t.pointer_closes.to_owned(),
        item: t.pointer_item.to_owned(),
    }
}

/// The key of a signature: its comment in 1Password and its fingerprint.
fn key_line(r: &Request) -> Option<String> {
    match r {
        Request::Sign {
            comment,
            fingerprint,
            ..
        } => Some(match comment.as_deref().filter(|c| !c.is_empty()) {
            Some(c) => format!("{c} · {fingerprint}"),
            None => fingerprint.clone(),
        }),
        _ => None,
    }
}

fn caller_line(t: &Table, c: &Caller) -> String {
    match c {
        Caller::Claude { pid: Some(pid) } => fill(t.claude_pid, &pid.to_string()),
        Caller::Claude { pid: None } => t.claude.to_owned(),
        Caller::Person { shell } if shell.is_empty() => t.person.to_owned(),
        Caller::Person { shell } => format!("{} · {shell}", t.person),
        Caller::Program { name } => fill(t.program, name),
        Caller::Unknown(Unknown::Forwarded) => t.unknown_forwarded.to_owned(),
        Caller::Unknown(Unknown::WslDirect) => t.unknown_wsl.to_owned(),
        Caller::Unknown(Unknown::Unreadable) => t.unknown_unreadable.to_owned(),
    }
}

/// Where the request goes: the destination and its host key, or, forwarded, the remote host (with its host key) and
/// the host key of the remote client's destination.
fn destination_line(t: &Table, f: &Facts) -> Option<String> {
    if let Some(fw) = &f.forwarded {
        let from = match &fw.from_key {
            Some(k) => format!("{} ({})", fw.from, fill(t.host_key, k)),
            None => fw.from.clone(),
        };
        let mut line = fill(t.forwarded_from, &from);
        if let Some(k) = &fw.onward_key {
            line.push_str(" · ");
            line.push_str(&fill(t.onward, k));
        }
        return Some(line);
    }
    match (&f.destination, &f.host_key) {
        (Some(d), Some(k)) => Some(format!("{d} · {}", fill(t.host_key, k))),
        (Some(d), None) => Some(d.clone()),
        (None, Some(k)) => Some(fill(t.host_key, k)),
        (None, None) => None,
    }
}

/// [`build`] in both languages, for the window's switch.
pub fn both(f: &Facts, phase: Phase) -> Both {
    Both {
        en: build(f, phase, Lang::En),
        ja: build(f, phase, Lang::Ja),
    }
}

/// The caller in a few words (the holder file, the toast).
pub fn caller_short(c: &Caller, lang: Lang) -> String {
    match c {
        Caller::Claude { .. } => table(lang).claude.to_owned(),
        Caller::Person { .. } => ui::table(lang).human_short.to_owned(),
        Caller::Program { name } => name.clone(),
        Caller::Unknown(_) => "?".to_owned(),
    }
}

/// The toast of a signature that passed without a window: three lines (design `2026-11-18-pass-toasts-design.md` §4.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassTexts {
    /// "Passed": the first word of line 1.
    pub word: String,
    /// The caller in a few words ([`caller_short`]).
    pub caller: String,
    /// The client's command line, as the window's Command line shows it.
    pub command: String,
    /// Where the client connects, when its command line says.
    pub destination: Option<String>,
    /// Line 3: why it passed (the answer that covers it), then the reason.
    pub why: String,
}

impl PassTexts {
    /// Line 1: `Passed · <caller>`, then ` ×N` for a card of `count` signatures (two or more).
    pub fn head(&self, count: u32) -> String {
        let mut out = format!("{} · {}", self.word, self.caller);
        if count >= 2 {
            out.push_str(&format!(" ×{count}"));
        }
        out
    }

    /// Line 2: `<command> → <destination>`, the command alone without a destination.
    pub fn line2(&self) -> String {
        match &self.destination {
            Some(d) => format!("{} → {d}", self.command),
            None => self.command.clone(),
        }
    }
}

/// The toast's texts for a signature of the call of `f` that the earlier `answer` let through.
pub fn pass_texts(f: &Facts, answer: Answer, lang: Lang) -> PassTexts {
    let t = table(lang);
    let how = match answer {
        Answer::AllowTerminal => t.pass_terminal,
        Answer::Allow | Answer::Deny => t.pass_call,
    };
    let reason = if f.reason.is_empty() {
        t.no_reason
    } else {
        &f.reason
    };
    PassTexts {
        word: t.pass_word.to_owned(),
        caller: caller_short(&f.caller, lang),
        command: f.command.clone(),
        destination: f.destination.clone().filter(|d| !d.is_empty()),
        why: format!("{how} · {reason}"),
    }
}

/// show-only: a signature passed in under half a second, so 1Password most likely did not ask (§6).
pub fn no_prompt_toast(lang: Lang) -> Vec<String> {
    let lines: [&str; 2] = match lang {
        Lang::En => [
            "SSH key: 1Password did not ask",
            "A signature through the relay passed in under half a second, so 1Password's setting is most likely not \"request\": one approval lets every call through. Use per-call mode, or set 1Password to ask for every request.",
        ],
        Lang::Ja => [
            "SSH の鍵: 1Password が確認しませんでした",
            "中継を通った署名が 0.5 秒もかからずに通りました。1Password の設定がおそらく「リクエストごと」ではなく、1 度の承認ですべての呼び出しが通ります。per-call モードを使うか、1Password をリクエストごとに確認する設定にしてください。",
        ],
    };
    lines.map(str::to_owned).to_vec()
}

/// The relay's parent process is missing at start (`ended` false) or has ended (true) (§8). 1Password then most likely
/// takes the relay itself for the application that asks, which it sees in front only while op-reason's window is (design
/// `2026-11-22-front-after-allow-design.md` §6).
pub fn parent_toast(lang: Lang, ended: bool) -> Vec<String> {
    let (title, body) = match (lang, ended) {
        (Lang::En, false) => (
            "op-reason's SSH relay has no parent process",
            PARENT_BODY_EN,
        ),
        (Lang::En, true) => (
            "op-reason's SSH relay lost its parent process",
            PARENT_BODY_EN,
        ),
        (Lang::Ja, false) => (
            "op-reason の SSH の中継に親の process がありません",
            PARENT_BODY_JA,
        ),
        (Lang::Ja, true) => (
            "op-reason の SSH の中継の親の process が終わりました",
            PARENT_BODY_JA,
        ),
    };
    vec![title.to_owned(), body.to_owned()]
}

const PARENT_BODY_EN: &str = "1Password now most likely takes the relay itself for the application that asks: its prompt comes while op-reason's window is in front (after Allow), and otherwise waits behind the 1Password icon at the right of the taskbar (SSH request waiting).";
const PARENT_BODY_JA: &str = "1Password はこれから、おそらく中継そのものを求めたアプリと見ます。確認の画面は op-reason の窓が前面にある間 (許可を押した後) に出て、そうでないときはタスクバーの右の 1Password のアイコン (SSHリクエストを待機中) で待ちます。";

/// 1Password closed a request without an answer: most likely its prompt waited behind its icon, unseen, because the
/// application it sees was not in front (design `2026-11-22-front-after-allow-design.md` §3); `parent_gone` adds why that
/// application is the relay itself.
pub fn no_answer_toast(lang: Lang, parent_gone: bool) -> Vec<String> {
    let (title, body, cause) = match lang {
        Lang::En => (
            "SSH key: 1Password did not answer",
            "1Password closed the request without an answer. While the application that asks is not in front, 1Password keeps its prompt behind its icon at the right of the taskbar (SSH request waiting) and closes it after about 60 seconds.",
            " The relay's parent process has ended, so 1Password most likely takes the relay itself for that application.",
        ),
        Lang::Ja => (
            "SSH の鍵: 1Password が答えませんでした",
            "1Password が答えずに依頼を閉じました。求めたアプリが前面にないと、1Password は確認の画面をタスクバーの右のアイコン (SSHリクエストを待機中) に伏せ、約 60 秒で閉じます。",
            "中継の親の process が終わったので、1Password はおそらく中継そのものをそのアプリと見ています。",
        ),
    };
    let mut text = body.to_owned();
    if parent_gone {
        text.push_str(cause);
    }
    vec![title.to_owned(), text]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            time: "2026-11-18 09:00:00".into(),
            mode: Mode::PerCall,
            caller: Caller::Claude { pid: Some(4242) },
            root: Some(Root {
                name: "bash".into(),
                pid: 31337,
                cmd: r#"bash -c -l "git push origin fix/login""#.into(),
                claude_powershell: false,
            }),
            herdr: None,
            distro: None,
            place: Some(Place {
                repo: Some("me/r".into()),
                branch: Some("main".into()),
                cwd: r"~\src\r".into(),
            }),
            reason: "push the fix".into(),
            command: r#"ssh.exe git@github.com "git-receive-pack 'me/r.git'""#.into(),
            destination: Some("git@github.com".into()),
            host_key: Some("SHA256:h".into()),
            forwarded: None,
            request: Request::Sign {
                purpose: Purpose::UserAuth { user: "git".into() },
                comment: Some("GitHub".into()),
                fingerprint: "SHA256:k".into(),
            },
            limit_minutes: 60,
            waiting: 0,
            terminal: false,
            single: false,
        }
    }

    fn view(f: &Facts, phase: Phase, lang: Lang) -> View {
        build(f, phase, lang).view
    }

    fn chip(kind: ChipKind, text: &str) -> Chip {
        Chip {
            kind,
            text: text.to_owned(),
        }
    }

    fn fact(kind: FactKind, text: &str) -> Fact {
        Fact {
            kind,
            text: text.to_owned(),
        }
    }

    #[test]
    fn the_heads_and_buttons_of_both_phases() {
        let en = build(&facts(), Phase::Ask, Lang::En);
        assert_eq!(en.title, "SSH key: allow this call?");
        assert_eq!(
            en.buttons,
            vec![
                (Button::Allow, "Allow this call".to_owned()),
                (Button::Deny, "Deny".to_owned())
            ]
        );
        let ja = build(&facts(), Phase::Ask, Lang::Ja);
        assert_eq!(ja.title, "SSH の鍵: この呼び出しを許可しますか");
        assert_eq!(
            ja.buttons,
            vec![
                (Button::Allow, "この呼び出しを許可".to_owned()),
                (Button::Deny, "拒否".to_owned())
            ]
        );
        let asking = build(&facts(), Phase::Asking, Lang::En);
        assert_eq!(asking.title, "SSH key: 1Password is asking");
        assert!(asking.buttons.is_empty());
        assert_eq!(
            build(&facts(), Phase::Asking, Lang::Ja).title,
            "SSH の鍵: 1Password が確認しています"
        );
        assert_eq!(both(&facts(), Phase::Ask).ja, ja);
    }

    #[test]
    fn the_view_of_a_call_that_asks() {
        let v = view(&facts(), Phase::Ask, Lang::En);
        assert_eq!(v.icon, Icon::Key);
        assert_eq!(v.subtitle, "op-reason's relay · per-call mode");
        assert_eq!(
            (v.time.as_str(), v.date.as_str()),
            ("09:00:00", "2026-11-18")
        );
        assert_eq!(v.reason_label, "Reason");
        assert_eq!(v.reason, Reason::Given("push the fix".into()));
        assert_eq!(
            v.chips,
            vec![
                chip(ChipKind::Claude, "Claude Code (pid 4242)"),
                chip(ChipKind::Repo, "me/r · main"),
                chip(ChipKind::Folder, r"~\src\r"),
            ]
        );
        assert_eq!(v.code_label, "Root command");
        assert_eq!(v.code, "git push origin fix/login");
        assert_eq!(
            v.code_note.as_deref(),
            Some(r#"bash (pid 31337) · bash -c -l "…""#),
            "the root process, and its line with the box's command as …"
        );
        assert_eq!(
            v.facts,
            vec![
                fact(FactKind::Purpose, "logging in as git"),
                fact(
                    FactKind::Detail,
                    "Destination git@github.com · host key SHA256:h · Key GitHub · SHA256:k"
                ),
                fact(
                    FactKind::Mono,
                    r#"Command: ssh.exe git@github.com "git-receive-pack 'me/r.git'""#
                ),
            ]
        );
        assert_eq!(v.countdown.as_deref(), Some("{0} s left to answer"));
        assert_eq!(
            v.status,
            "Allowing covers this git push until it ends (at most 60 minutes)"
        );
        assert_eq!(v.band.sketch, Sketch::Ask);
        assert!(!v.band.keep);
        assert_eq!(v.band.title, "Allowing brings up 1Password's prompt");
        assert_eq!(
            look::marked(&v.band.line).text,
            "Usually in the middle of the screen; if not, from the tray at the bottom right"
        );
        assert_eq!(v.footer, "");
        assert!(v.notes.is_empty());
        let t = build(&facts(), Phase::Ask, Lang::En);
        assert!(t.lines.is_empty(), "the relay keeps no label-value lines");
        assert_eq!(t.footer, v.status);
        assert_eq!(t.toast[0], "SSH key: allow this call?");
        assert_eq!(t.toast[1], "Reason: push the fix");
        assert!(
            t.toast[2].starts_with("Claude Code · ssh.exe git@github.com"),
            "{:?}",
            t.toast
        );
        let ja = view(&facts(), Phase::Ask, Lang::Ja);
        assert_eq!(ja.subtitle, "op-reason の中継 · 呼び出しごとに決めるモード");
        assert_eq!(ja.reason_label, "理由");
        assert_eq!(ja.code_label, "起点のコマンド");
        assert_eq!(ja.countdown.as_deref(), Some("答えを待つ残り {0} 秒"));
        assert_eq!(
            ja.status,
            "許可は、この git push が終わるまで効きます (長くても 60 分)"
        );
        assert_eq!(ja.facts[0].text, "git としてログイン");
        assert_eq!(
            ja.facts[1].text,
            "接続先 git@github.com · ホストの鍵 SHA256:h · 鍵 GitHub · SHA256:k"
        );
        assert_eq!(ja.band.title, "許可すると 1Password の画面が出ます");
        assert_eq!(
            look::marked(&ja.band.line).text,
            "ふつうは画面の中央。出ないときは右下のトレイから"
        );
    }

    #[test]
    fn the_root_as_it_is_when_nothing_wraps_it() {
        let mut f = facts();
        f.root = Some(Root {
            name: "git".into(),
            pid: 2718,
            cmd: "git pull --rebase".into(),
            claude_powershell: false,
        });
        let v = view(&f, Phase::Ask, Lang::En);
        assert_eq!(v.code, "git pull --rebase");
        assert_eq!(
            v.code_note.as_deref(),
            Some("git (pid 2718)"),
            "the box shows the whole line; its process under it"
        );
        assert_eq!(
            v.status,
            "Allowing covers this git pull until it ends (at most 60 minutes)"
        );
    }

    #[test]
    fn a_long_root_line() {
        let mut f = facts();
        let eval = "git push origin main";
        let cmd = format!(
            r#"bash -c "source /home/u/.claude/shell-snapshots/snapshot-bash-1-x.sh 2>/dev/null || true && {pad} && eval '{eval}' && pwd -P >| /tmp/claude-f37c-cwd""#,
            pad = [r"\builtin shopt -u extglob 2>/dev/null || true"; 8].join(" && ")
        );
        assert!(
            cmd.chars().count() > 300,
            "the command comes late in the line"
        );
        f.root = Some(Root {
            name: "bash".into(),
            pid: 7,
            cmd: cmd.clone(),
            claude_powershell: false,
        });
        let v = view(&f, Phase::Ask, Lang::En);
        assert_eq!(
            v.code, eval,
            "taken from the record's line, past 200 characters"
        );
        let note = v.code_note.unwrap();
        assert_eq!(
            note,
            format!("bash (pid 7) · {}", cmd.replace(eval, "…")),
            "the whole rest of the line, not cut at 200 characters"
        );
        // a line that nothing comes off is shown cut at 200 characters, as before
        let long = format!("someprog {}", "x".repeat(400));
        f.root = Some(Root {
            name: "someprog".into(),
            pid: 8,
            cmd: long,
            claude_powershell: false,
        });
        let v = view(&f, Phase::Ask, Lang::En);
        assert_eq!(v.code.chars().count(), 201);
        assert!(v.code.ends_with('…'));
        assert_eq!(v.code_note.as_deref(), Some("someprog (pid 8)"));
    }

    #[test]
    fn a_shell_line_says_this_call() {
        let mut f = facts();
        f.root = Some(Root {
            name: "bash".into(),
            pid: 9,
            cmd: r#"bash -c "cd a && git push""#.into(),
            claude_powershell: false,
        });
        let v = view(&f, Phase::Ask, Lang::En);
        assert_eq!(v.code, "cd a && git push");
        assert_eq!(
            v.status,
            "Allowing covers this call until it ends (at most 60 minutes)"
        );
        assert_eq!(
            view(&f, Phase::Ask, Lang::Ja).status,
            "許可は、この呼び出しが終わるまで効きます (長くても 60 分)"
        );
        f.limit_minutes = 15;
        assert!(
            view(&f, Phase::Ask, Lang::En)
                .status
                .contains("(at most 15 minutes)")
        );
    }

    /// The root of a call of Claude Code's PowerShell tool: its launcher, with the line the record keeps of it (look.rs
    /// has how it comes about), told by the relay from its arguments.
    fn powershell_root() -> Root {
        Root {
            name: "cmd.exe".into(),
            pid: 2024,
            cmd: r#"cmd.exe /d /s /c "C:\WINDOWS\System32\chcp.com 65001 >nul & C:\Users\me\AppData\Local\Microsoft\WindowsApps\pwsh.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command $__claudeCodeScript" =*** "$env:CLAUDE_CODE_SHELL_LAUNCHER_SCRIPT;" "$env:CLAUDE_CODE_SHELL_LAUNCHER_SCRIPT" =*** "$null;" Invoke-Expression -Command "$__claudeCodeScript > C:\Users\me\AppData\Local\Temp\claude\shell-out-1.txt 2>&1""#.into(),
            claude_powershell: true,
        }
    }

    #[test]
    fn a_call_of_claude_codes_powershell_tool_shows_its_ssh_command() {
        let mut f = facts();
        f.root = Some(powershell_root());
        let v = view(&f, Phase::Ask, Lang::En);
        assert_eq!(v.code_label, "Command");
        assert_eq!(
            v.code, r#"ssh.exe git@github.com "git-receive-pack 'me/r.git'""#,
            "the script is not on the launcher's line: the box shows the ssh command"
        );
        assert_eq!(
            v.code_note.as_deref(),
            Some("cmd.exe (pid 2024) · Claude Code's PowerShell tool")
        );
        assert_eq!(
            v.facts,
            vec![
                fact(FactKind::Purpose, "logging in as git"),
                fact(
                    FactKind::Detail,
                    "Destination git@github.com · host key SHA256:h · Key GitHub · SHA256:k"
                ),
            ],
            "no second line of the ssh command"
        );
        assert_eq!(
            v.status, "Allowing covers this call until it ends (at most 60 minutes)",
            "the answer covers the tool's call, whose script is not seen"
        );
        let ja = view(&f, Phase::Ask, Lang::Ja);
        assert_eq!(ja.code_label, "コマンド");
        assert_eq!(
            ja.code_note.as_deref(),
            Some("cmd.exe (pid 2024) · Claude Code の PowerShell")
        );
        assert_eq!(
            ja.status,
            "許可は、この呼び出しが終わるまで効きます (長くても 60 分)"
        );
        let asking = view(&f, Phase::Asking, Lang::En);
        assert_eq!(
            (asking.code_label.as_str(), asking.code.as_str()),
            ("Command", v.code.as_str()),
            "after Allow, the same box"
        );
    }

    #[test]
    fn only_claude_codes_call_is_its_powershell_tool() {
        // a person or a program whose root has the launcher's line is shown as any root: never as Claude Code's tool
        for caller in [
            Caller::Person {
                shell: "pwsh.exe".into(),
            },
            Caller::Program {
                name: "cmd.exe".into(),
            },
        ] {
            let mut f = facts();
            f.caller = caller.clone();
            f.root = Some(powershell_root());
            let v = view(&f, Phase::Ask, Lang::En);
            assert_eq!(v.code_label, "Root command", "{caller:?}");
            assert_eq!(
                v.code_note
                    .as_deref()
                    .map(|n| n.contains("PowerShell tool")),
                Some(false)
            );
        }
    }

    #[test]
    fn no_reason_given() {
        let mut f = facts();
        f.reason = String::new();
        assert_eq!(
            view(&f, Phase::Ask, Lang::En).reason,
            Reason::Missing("no reason given".into())
        );
        assert_eq!(
            view(&f, Phase::Asking, Lang::Ja).reason,
            Reason::Missing("理由なし".into())
        );
    }

    #[test]
    fn while_1password_asks() {
        let mut show = facts();
        show.mode = Mode::ShowOnly;
        let mut list = facts();
        list.request = Request::List;
        assert_eq!(
            view(&show, Phase::Asking, Lang::En).subtitle,
            "op-reason's relay · show-only mode"
        );
        assert_eq!(
            view(&show, Phase::Asking, Lang::Ja).subtitle,
            "op-reason の中継 · 表示だけのモード"
        );
        for (f, en_title, ja_title) in [
            (
                facts(),
                "Allowed here; 1Password itself may ask now",
                "ここでは許可済み。いまは 1Password 自身が確認することがあります",
            ),
            (
                list,
                "1Password itself may ask now (to unlock it, for example)",
                "いまは 1Password 自身が確認することがあります (ロックの解除など)",
            ),
            (
                show,
                "Decide in 1Password's prompt",
                "決めるのは 1Password の画面です",
            ),
        ] {
            let en = view(&f, Phase::Asking, Lang::En);
            assert_eq!(en.band.title, en_title);
            assert_eq!(
                look::marked(&en.band.line).text,
                "If 1Password's prompt does not come: the 1Password icon at the right of the taskbar (under ^ when hidden) → SSH request waiting"
            );
            assert_eq!(en.band.sketch, Sketch::Asking);
            assert!(
                en.band.keep,
                "today's last line and the mode's words stay on a small screen"
            );
            assert_eq!(
                en.footer,
                "Deny any 1Password prompt that comes without this window"
            );
            assert_eq!(en.countdown, None);
            assert_eq!(en.status, "");
            let ja = view(&f, Phase::Asking, Lang::Ja);
            assert_eq!(ja.band.title, ja_title);
            assert_eq!(
                look::marked(&ja.band.line).text,
                "1Password の画面が出ないとき: タスクバーの右の 1Password のアイコン (隠れていれば ^ の中) →「SSHリクエストを待機中」"
            );
            assert_eq!(
                ja.footer,
                "この窓が出ていない 1Password の確認は拒否してください"
            );
            assert_eq!(build(&f, Phase::Asking, Lang::Ja).footer, ja.footer);
        }
    }

    #[test]
    fn after_a_click_that_allows() {
        let seen = build(&facts(), Phase::Seen, Lang::En);
        assert_eq!(
            seen.title, "SSH key: 1Password is asking",
            "the window keeps its title"
        );
        assert!(seen.buttons.is_empty());
        let v = &seen.view;
        assert_eq!(v.band.title, "Approve in 1Password's window");
        assert_eq!(
            look::marked(&v.band.line).text,
            "It asks whether the SSH key GitHub may be used. If it went behind other windows: the tray at the bottom right"
        );
        assert_eq!(
            (v.band.sketch, v.band.keep, v.band.alert),
            (Sketch::Prompt, true, false)
        );
        assert_eq!(v.countdown, None);
        assert_eq!(
            v.footer,
            "Deny any 1Password prompt that comes without this window"
        );
        let ja = view(&facts(), Phase::Seen, Lang::Ja);
        assert_eq!(ja.band.title, "1Password の画面で承認してください");
        assert_eq!(
            look::marked(&ja.band.line).text,
            "SSH の鍵 GitHub を使ってよいかを聞いています。ほかの窓の後ろに隠れたら右下のトレイから"
        );
        let mut nameless = facts();
        nameless.request = Request::Sign {
            purpose: Purpose::UserAuth { user: "git".into() },
            comment: None,
            fingerprint: "SHA256:k".into(),
        };
        assert!(
            view(&nameless, Phase::Seen, Lang::En)
                .band
                .line
                .contains("⟦SHA256:k⟧")
        );

        let hidden = view(&facts(), Phase::Hidden, Lang::En);
        assert_eq!(
            hidden.band.title,
            "1Password's prompt did not come to the front"
        );
        assert_eq!(
            look::marked(&hidden.band.line).text,
            "Open it from the 1Password icon at the right of the taskbar (under ^ when hidden) → SSH request waiting"
        );
        assert_eq!(
            (hidden.band.sketch, hidden.band.keep, hidden.band.alert),
            (Sketch::Tray, true, true)
        );
        assert_eq!(
            hidden.countdown.as_deref(),
            Some("1Password closes the request in about {0} s")
        );
        assert_eq!(hidden.status, "");
        let ja = view(&facts(), Phase::Hidden, Lang::Ja);
        assert_eq!(ja.band.title, "1Password の画面が前に出ていません");
        assert_eq!(
            ja.countdown.as_deref(),
            Some("1Password が依頼を閉じるまで 約 {0} 秒")
        );
        let mut waiting = facts();
        waiting.waiting = 1;
        assert!(
            view(&waiting, Phase::Hidden, Lang::En)
                .footer
                .ends_with(" · 1 more waiting")
        );
    }

    #[test]
    fn the_pointer_to_the_tray() {
        let en = pointer_texts(Lang::En);
        assert_eq!(en.title, "1Password's prompt is waiting in the tray");
        assert_eq!(
            en.line,
            "1Password icon (under ^ when hidden) → SSH request waiting"
        );
        assert_eq!(en.closes, "closes in about {0} s");
        assert_eq!(en.item, "SSH request waiting");
        let ja = pointer_texts(Lang::Ja);
        assert_eq!(ja.title, "1Password の画面はトレイで待っています");
        assert_eq!(
            ja.line,
            "1Password のアイコン (隠れていれば ^ の中) →「SSHリクエストを待機中」"
        );
        assert_eq!(ja.closes, "約 {0} 秒で閉じます");
        assert_eq!(
            ja.item, "SSHリクエストを待機中",
            "the words of 1Password's menu"
        );
    }

    #[test]
    fn the_terminal_button_only_when_offered() {
        let mut f = facts();
        f.caller = Caller::Person {
            shell: "pwsh.exe".into(),
        };
        f.terminal = true;
        let en = build(&f, Phase::Ask, Lang::En);
        assert_eq!(
            en.buttons.iter().map(|(b, _)| *b).collect::<Vec<_>>(),
            [Button::Allow, Button::AllowTerminal, Button::Deny]
        );
        assert_eq!(en.buttons[1].1, "Allow this terminal for 60 minutes");
        assert_eq!(
            build(&f, Phase::Ask, Lang::Ja).buttons[1].1,
            "このターミナルを 60 分許可"
        );
        assert_eq!(
            en.view.chips[0],
            chip(ChipKind::Person, "Human (terminal) · pwsh.exe")
        );
        assert!(build(&f, Phase::Asking, Lang::En).buttons.is_empty());
    }

    #[test]
    fn waiting_calls_and_a_signature_alone() {
        let mut f = facts();
        f.waiting = 2;
        assert!(
            view(&f, Phase::Ask, Lang::En)
                .status
                .ends_with(" · 2 more waiting")
        );
        assert!(
            view(&f, Phase::Ask, Lang::Ja)
                .status
                .ends_with(" · ほかに 2 件待っています")
        );
        assert!(
            view(&f, Phase::Asking, Lang::En)
                .footer
                .ends_with(" · 2 more waiting")
        );
        assert!(
            !view(&facts(), Phase::Ask, Lang::En)
                .status
                .contains("waiting")
        );
        let mut single = facts();
        single.single = true;
        assert_eq!(
            view(&single, Phase::Ask, Lang::En).status,
            "This answer covers this signature only"
        );
        assert_eq!(
            view(&single, Phase::Ask, Lang::Ja).status,
            "この答えはこの署名だけに効きます"
        );
    }

    #[test]
    fn a_request_without_a_root_shows_the_ssh_command() {
        let mut f = facts();
        f.caller = Caller::Unknown(Unknown::Forwarded);
        f.root = None;
        f.forwarded = Some(Forwarded {
            from: "jumphost".into(),
            from_key: Some("SHA256:a".into()),
            onward_key: Some("SHA256:b".into()),
        });
        let en = view(&f, Phase::Ask, Lang::En);
        assert_eq!(
            en.chips[0],
            chip(ChipKind::Other, "unknown (forwarded from a remote host)")
        );
        assert_eq!(en.code_label, "Command");
        assert_eq!(en.code, f.command);
        assert_eq!(en.code_note, None);
        assert!(en.facts.iter().all(|x| x.kind != FactKind::Mono));
        assert_eq!(
            en.facts[1].text,
            "Destination forwarded from jumphost (host key SHA256:a) · onward to the host with key SHA256:b · Key GitHub · SHA256:k"
        );
        assert_eq!(en.status, "This answer covers this signature only");
        let ja = view(&f, Phase::Ask, Lang::Ja);
        assert_eq!(ja.code_label, "コマンド");
        assert_eq!(
            ja.facts[1].text,
            "接続先 jumphost (ホストの鍵 SHA256:a) から転送 · 行き先のホストの鍵 SHA256:b · 鍵 GitHub · SHA256:k"
        );
    }

    #[test]
    fn wsl_calls_name_herdr_and_the_distribution() {
        let mut f = facts();
        f.herdr = Some(Herdr {
            workspace: "w1".into(),
            label: "デプロイ".into(),
            pane: "w1:p2".into(),
        });
        f.distro = Some("Ubuntu".into());
        let en = view(&f, Phase::Ask, Lang::En);
        assert_eq!(
            en.chips[1..3],
            [
                chip(ChipKind::Wsl, "WSL · Ubuntu"),
                chip(ChipKind::Herdr, "herdr · デプロイ · w1:p2"),
            ]
        );
        let mut direct = facts();
        direct.caller = Caller::Unknown(Unknown::WslDirect);
        assert_eq!(
            view(&direct, Phase::Ask, Lang::En).chips[0].text,
            "unknown (a WSL ssh.exe not started through op-reason-ssh)"
        );
    }

    #[test]
    fn other_purposes_and_requests() {
        let mut f = facts();
        f.request = Request::Sign {
            purpose: Purpose::Sshsig {
                namespace: "git".into(),
            },
            comment: None,
            fingerprint: "SHA256:k".into(),
        };
        let v = view(&f, Phase::Asking, Lang::En);
        assert_eq!(v.facts[0].text, "a signature (SSHSIG, namespace git)");
        assert!(
            v.facts[1].text.ends_with(" · Key SHA256:k"),
            "{:?}",
            v.facts
        );
        f.request = Request::List;
        let v = view(&f, Phase::Asking, Lang::Ja);
        assert_eq!(v.facts[0].text, "鍵の一覧");
        assert_eq!(
            v.facts[1].text, "接続先 git@github.com · ホストの鍵 SHA256:h",
            "no key for the key list"
        );
        f.request = Request::Other(17);
        assert_eq!(
            view(&f, Phase::Asking, Lang::En).facts[0].text,
            "agent request 17"
        );
    }

    #[test]
    fn the_pass_toast_says_who_what_and_why() {
        let en = pass_texts(&facts(), Answer::Allow, Lang::En);
        assert_eq!(en.word, "Passed");
        assert_eq!(en.caller, "Claude Code");
        assert_eq!(
            en.command,
            r#"ssh.exe git@github.com "git-receive-pack 'me/r.git'""#
        );
        assert_eq!(en.destination.as_deref(), Some("git@github.com"));
        assert_eq!(en.why, "allowed for this call · push the fix");
        assert_eq!(en.head(1), "Passed · Claude Code");
        assert_eq!(en.head(3), "Passed · Claude Code ×3");
        assert_eq!(
            en.line2(),
            r#"ssh.exe git@github.com "git-receive-pack 'me/r.git'" → git@github.com"#
        );
        let ja = pass_texts(&facts(), Answer::Allow, Lang::Ja);
        assert_eq!(ja.word, "通過");
        assert_eq!(ja.head(2), "通過 · Claude Code ×2");
        assert_eq!(ja.why, "この呼び出しの許可 · push the fix");
    }

    #[test]
    fn the_terminal_answer_and_no_reason() {
        let mut f = facts();
        f.caller = Caller::Person {
            shell: "pwsh.exe".into(),
        };
        f.reason = String::new();
        f.destination = None;
        let en = pass_texts(&f, Answer::AllowTerminal, Lang::En);
        assert_eq!(en.why, "allowed for this terminal · no reason given");
        assert_eq!(en.caller, "Human");
        assert_eq!(en.destination, None);
        assert_eq!(en.line2(), f.command, "the command alone");
        let ja = pass_texts(&f, Answer::AllowTerminal, Lang::Ja);
        assert_eq!(ja.why, "ターミナルの許可 · 理由なし");
        assert_eq!(ja.caller, "人");
        f.destination = Some(String::new());
        assert_eq!(pass_texts(&f, Answer::Allow, Lang::En).destination, None);
    }

    #[test]
    fn the_other_toasts() {
        for lang in [Lang::En, Lang::Ja] {
            assert_eq!(no_prompt_toast(lang).len(), 2);
            assert_ne!(parent_toast(lang, false), parent_toast(lang, true));
            assert_ne!(no_answer_toast(lang, false), no_answer_toast(lang, true));
        }
        assert!(no_prompt_toast(Lang::En)[1].contains("request"));
        assert!(no_answer_toast(Lang::En, true)[1].contains("parent"));
        assert!(!no_answer_toast(Lang::En, false)[1].contains("parent"));
        assert!(parent_toast(Lang::Ja, true)[0].contains("親"));
        // where 1Password keeps a prompt it did not show, not "start it from a task" (a task's relay is in the same
        // place: M1b in design 2026-11-22 §3)
        for parent_gone in [false, true] {
            let en = &no_answer_toast(Lang::En, parent_gone)[1];
            assert!(en.contains("(SSH request waiting)"), "{en}");
            assert!(!en.contains("scheduled task"), "{en}");
            let ja = &no_answer_toast(Lang::Ja, parent_gone)[1];
            assert!(ja.contains("(SSHリクエストを待機中)"), "{ja}");
            assert!(!ja.contains("タスク スケジューラー"), "{ja}");
        }
        for ended in [false, true] {
            assert!(parent_toast(Lang::En, ended)[1].contains("(SSH request waiting)"));
            assert!(parent_toast(Lang::Ja, ended)[1].contains("(SSHリクエストを待機中)"));
        }
        assert_eq!(
            caller_short(&Caller::Claude { pid: None }, Lang::En),
            "Claude Code"
        );
        assert_eq!(
            caller_short(
                &Caller::Person {
                    shell: "fish".into()
                },
                Lang::Ja
            ),
            "人"
        );
    }
}

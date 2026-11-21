//! The texts of the SSH relay's window and toasts (design `2026-11-17-ssh-relay-design.md` §6), in English and
//! Japanese. The window is `op-reason.exe`'s ([`crate::ui::Texts`] with buttons); its look and behavior stay the same.
//!
//! - [`Phase::Ask`] (per-call): "SSH key: allow this call?" with who, what and why, and the buttons.
//! - [`Phase::Asking`]: the same facts without buttons while 1Password's prompt may be open (show-only, after a click on
//!   Allow, or a request that 1Password is slow to answer), and a last line that says where 1Password keeps a prompt it
//!   did not show (design `2026-11-22-front-after-allow-design.md` §6).
//!
//! And the toast of a signature that an earlier answer let through without a window ([`pass_texts`]; design
//! `2026-11-18-pass-toasts-design.md` §4.3).

use op_reason_core::contract::{Herdr, Place};
use op_reason_core::lang::Lang;
use op_reason_core::ssh::decide::Answer;
use op_reason_core::ssh::wire::Purpose;

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

/// What the window shows of one request (masked and clipped already).
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    /// `YYYY-MM-DD HH:MM:SS`
    pub time: String,
    pub mode: Mode,
    pub caller: Caller,
    /// The root process: `<name> (pid <n>) · <command line>`.
    pub root: Option<String>,
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
}

/// The words of the window and the toasts.
struct Table {
    title_ask: &'static str,
    title_asking: &'static str,
    time: &'static str,
    caller: &'static str,
    root: &'static str,
    wsl: &'static str,
    place: &'static str,
    reason: &'static str,
    no_reason: &'static str,
    command: &'static str,
    destination: &'static str,
    host_key: &'static str,
    forwarded_from: &'static str,
    onward: &'static str,
    purpose: &'static str,
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
    footer_ask: &'static str,
    footer_ask_alone: &'static str,
    waiting: &'static str,
    footer_asking_per_call: &'static str,
    footer_asking_slow: &'static str,
    footer_asking_show_only: &'static str,
    /// The last line while 1Password asks: where it keeps a prompt that it did not show (its label and value).
    tray: &'static str,
    tray_how: &'static str,
    pass_word: &'static str,
    pass_call: &'static str,
    pass_terminal: &'static str,
}

const EN: Table = Table {
    title_ask: "SSH key: allow this call?",
    title_asking: "SSH key: 1Password is asking",
    time: "Time",
    caller: "Caller",
    root: "Root",
    wsl: "WSL",
    place: "Place",
    reason: "Reason",
    no_reason: "no reason given",
    command: "Command",
    destination: "Destination",
    host_key: "host key {0}",
    forwarded_from: "forwarded from {0}",
    onward: "onward to the host with key {0}",
    purpose: "For",
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
    footer_ask: "per-call mode · an answer covers this call for {0} minutes",
    footer_ask_alone: "per-call mode · this answer covers this signature only",
    waiting: " · {0} more waiting",
    footer_asking_per_call: "per-call mode · allowed here; 1Password itself may ask now. Deny any 1Password prompt that comes without this window",
    footer_asking_slow: "per-call mode · 1Password itself may ask now (to unlock it, for example). Deny any 1Password prompt that comes without this window",
    footer_asking_show_only: "show-only mode: decide in 1Password's prompt. Deny any 1Password prompt that comes without this window",
    tray: "If 1Password's prompt does not come",
    tray_how: "the 1Password icon at the right of the taskbar (under ^ when hidden) → SSH request waiting",
    pass_word: "Passed",
    pass_call: "allowed for this call",
    pass_terminal: "allowed for this terminal",
};

const JA: Table = Table {
    title_ask: "SSH の鍵: この呼び出しを許可しますか",
    title_asking: "SSH の鍵: 1Password が確認しています",
    time: "時刻",
    caller: "呼び手",
    root: "起点",
    wsl: "WSL",
    place: "場所",
    reason: "理由",
    no_reason: "理由なし",
    command: "コマンド",
    destination: "接続先",
    host_key: "ホストの鍵 {0}",
    forwarded_from: "{0} から転送",
    onward: "行き先のホストの鍵 {0}",
    purpose: "用途",
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
    footer_ask: "呼び出しごとに決めるモード · 答えはこの呼び出しに {0} 分効きます",
    footer_ask_alone: "呼び出しごとに決めるモード · この答えはこの署名だけに効きます",
    waiting: " · ほかに {0} 件待っています",
    footer_asking_per_call: "呼び出しごとに決めるモード · ここでは許可済み。いまは 1Password 自身が確認することがあります。この窓が出ていない 1Password の確認は拒否してください",
    footer_asking_slow: "呼び出しごとに決めるモード · いまは 1Password 自身が確認することがあります (ロックの解除など)。この窓が出ていない 1Password の確認は拒否してください",
    footer_asking_show_only: "表示だけのモード: 決めるのは 1Password の画面です。この窓が出ていない 1Password の確認は拒否してください",
    tray: "1Password の画面が出ないとき",
    tray_how: "タスクバーの右の 1Password のアイコン (隠れていれば ^ の中) →「SSHリクエストを待機中」",
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

/// The texts of one window and its toast in `lang`.
pub fn build(f: &Facts, phase: Phase, lang: Lang) -> Texts {
    let t = table(lang);
    let title = match phase {
        Phase::Ask => t.title_ask,
        Phase::Asking => t.title_asking,
    };
    let mut lines = vec![
        (t.time.to_owned(), f.time.clone()),
        (t.caller.to_owned(), caller_line(t, &f.caller)),
    ];
    if let Some(root) = &f.root {
        lines.push((t.root.to_owned(), root.clone()));
    }
    if let Some(h) = &f.herdr {
        let label = if h.label.is_empty() { "?" } else { &h.label };
        let pane = if h.pane.is_empty() { "?" } else { &h.pane };
        let ui_t = ui::table(lang);
        lines.push((
            ui_t.herdr.to_owned(),
            ui_t.herdr_line.replace("{0}", label).replace("{1}", pane),
        ));
    }
    if let Some(d) = &f.distro {
        lines.push((t.wsl.to_owned(), d.clone()));
    }
    if let Some(p) = &f.place {
        let mut place = p.cwd.clone();
        if let Some(repo) = p.repo.as_deref().filter(|r| !r.is_empty()) {
            let branch = p.branch.as_deref().filter(|b| !b.is_empty()).unwrap_or("?");
            place = format!("{repo} ({branch}) · {place}");
        }
        lines.push((t.place.to_owned(), place));
    }
    let reason = if f.reason.is_empty() {
        t.no_reason.to_owned()
    } else {
        f.reason.clone()
    };
    lines.push((t.reason.to_owned(), reason.clone()));
    lines.push((t.command.to_owned(), f.command.clone()));
    if let Some(d) = destination_line(t, f) {
        lines.push((t.destination.to_owned(), d));
    }
    let (purpose, key) = match &f.request {
        Request::Sign {
            purpose,
            comment,
            fingerprint,
        } => {
            let p = match purpose {
                Purpose::UserAuth { user } => fill(t.userauth, user),
                Purpose::Sshsig { namespace } => fill(t.sshsig, namespace),
                Purpose::Other => t.other_signature.to_owned(),
            };
            let k = match comment.as_deref().filter(|c| !c.is_empty()) {
                Some(c) => format!("{c} · {fingerprint}"),
                None => fingerprint.clone(),
            };
            (p, Some(k))
        }
        Request::List => (t.list.to_owned(), None),
        Request::Other(n) => (fill(t.other_request, &n.to_string()), None),
    };
    lines.push((t.purpose.to_owned(), purpose));
    if let Some(k) = key {
        lines.push((t.key.to_owned(), k));
    }
    // 1Password keeps the prompt of an application that is not in front behind its icon (design
    // `2026-11-22-front-after-allow-design.md` §6)
    if phase == Phase::Asking {
        lines.push((t.tray.to_owned(), t.tray_how.to_owned()));
    }
    let alone = f.single || matches!(f.caller, Caller::Unknown(_)) || f.forwarded.is_some();
    let mut footer = match (phase, f.mode) {
        (Phase::Ask, _) if alone => t.footer_ask_alone.to_owned(),
        (Phase::Ask, _) => fill(t.footer_ask, &f.limit_minutes.to_string()),
        (Phase::Asking, Mode::PerCall) if matches!(f.request, Request::Sign { .. }) => {
            t.footer_asking_per_call.to_owned()
        }
        (Phase::Asking, Mode::PerCall) => t.footer_asking_slow.to_owned(),
        (Phase::Asking, Mode::ShowOnly) => t.footer_asking_show_only.to_owned(),
    };
    if f.waiting > 0 {
        footer.push_str(&fill(t.waiting, &f.waiting.to_string()));
    }
    let buttons = match phase {
        Phase::Asking => Vec::new(),
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
    let refs_at = lines.len();
    Texts {
        title: title.to_owned(),
        lines,
        refs: None,
        refs_at,
        footer,
        others: String::new(),
        toast,
        buttons,
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
            root: Some("cmd.exe (pid 77) · cmd.exe /d /s /c x".into()),
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

    fn line<'a>(t: &'a Texts, label: &str) -> Option<&'a str> {
        t.lines
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, v)| v.as_str())
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
    fn while_1password_asks_the_last_line_says_where_a_hidden_prompt_waits() {
        let mut show = facts();
        show.mode = Mode::ShowOnly;
        let mut list = facts();
        list.request = Request::List;
        for f in [facts(), show, list] {
            let en = build(&f, Phase::Asking, Lang::En);
            assert_eq!(
                en.lines.last().map(|(l, v)| (l.as_str(), v.as_str())),
                Some((
                    "If 1Password's prompt does not come",
                    "the 1Password icon at the right of the taskbar (under ^ when hidden) → SSH request waiting"
                ))
            );
            let ja = build(&f, Phase::Asking, Lang::Ja);
            assert_eq!(
                ja.lines.last().map(|(l, v)| (l.as_str(), v.as_str())),
                Some((
                    "1Password の画面が出ないとき",
                    "タスクバーの右の 1Password のアイコン (隠れていれば ^ の中) →「SSHリクエストを待機中」"
                ))
            );
        }
        // the window that asks has its buttons instead
        let ask = build(&facts(), Phase::Ask, Lang::En);
        assert_eq!(ask.lines.last().map(|(l, _)| l.as_str()), Some("Key"));
    }

    #[test]
    fn the_lines_of_a_call() {
        let t = build(&facts(), Phase::Ask, Lang::En);
        let labels: Vec<&str> = t.lines.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Time",
                "Caller",
                "Root",
                "Place",
                "Reason",
                "Command",
                "Destination",
                "For",
                "Key"
            ]
        );
        assert_eq!(line(&t, "Caller"), Some("Claude Code (pid 4242)"));
        assert_eq!(
            line(&t, "Root"),
            Some("cmd.exe (pid 77) · cmd.exe /d /s /c x")
        );
        assert_eq!(line(&t, "Place"), Some(r"me/r (main) · ~\src\r"));
        assert_eq!(
            line(&t, "Destination"),
            Some("git@github.com · host key SHA256:h")
        );
        assert_eq!(line(&t, "For"), Some("logging in as git"));
        assert_eq!(line(&t, "Key"), Some("GitHub · SHA256:k"));
        let ja = build(&facts(), Phase::Ask, Lang::Ja);
        let labels: Vec<&str> = ja.lines.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            labels,
            [
                "時刻",
                "呼び手",
                "起点",
                "場所",
                "理由",
                "コマンド",
                "接続先",
                "用途",
                "鍵"
            ]
        );
        assert_eq!(
            line(&ja, "接続先"),
            Some("git@github.com · ホストの鍵 SHA256:h")
        );
        assert_eq!(line(&ja, "用途"), Some("git としてログイン"));
        assert_eq!(t.toast[0], "SSH key: allow this call?");
        assert_eq!(t.toast[1], "Reason: push the fix");
        assert!(
            t.toast[2].starts_with("Claude Code · ssh.exe git@github.com"),
            "{:?}",
            t.toast
        );
    }

    #[test]
    fn no_reason_given() {
        let mut f = facts();
        f.reason = String::new();
        assert_eq!(
            line(&build(&f, Phase::Ask, Lang::En), "Reason"),
            Some("no reason given")
        );
        assert_eq!(
            line(&build(&f, Phase::Ask, Lang::Ja), "理由"),
            Some("理由なし")
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
        assert_eq!(line(&en, "Caller"), Some("Human (terminal) · pwsh.exe"));
        assert!(build(&f, Phase::Asking, Lang::En).buttons.is_empty());
    }

    #[test]
    fn the_footer_says_the_mode_the_limit_and_who_waits() {
        let f = facts();
        let en = build(&f, Phase::Ask, Lang::En);
        assert!(
            en.footer.contains("per-call") && en.footer.contains("60 minutes"),
            "{}",
            en.footer
        );
        assert!(!en.footer.contains("waiting"), "{}", en.footer);
        let mut single = facts();
        single.single = true;
        assert!(
            build(&single, Phase::Ask, Lang::En)
                .footer
                .contains("this signature only")
        );
        let mut waiting = facts();
        waiting.waiting = 2;
        assert!(
            build(&waiting, Phase::Ask, Lang::En)
                .footer
                .contains("2 more waiting")
        );
        assert!(
            build(&waiting, Phase::Ask, Lang::Ja)
                .footer
                .contains("ほかに 2 件待っています")
        );
        let mut show = facts();
        show.mode = Mode::ShowOnly;
        let s = build(&show, Phase::Asking, Lang::En);
        assert!(
            s.footer.contains("show-only") && s.footer.contains("Deny any 1Password prompt"),
            "{}",
            s.footer
        );
        assert!(
            build(&show, Phase::Asking, Lang::Ja)
                .footer
                .contains("この窓が出ていない 1Password の確認は拒否")
        );
        // 1Password slow to answer the key list: nothing was allowed here
        let mut list = facts();
        list.request = Request::List;
        let l = build(&list, Phase::Asking, Lang::En);
        assert!(!l.footer.contains("allowed"), "{}", l.footer);
        assert!(
            l.footer.contains("Deny any 1Password prompt"),
            "{}",
            l.footer
        );
        assert!(
            !build(&list, Phase::Asking, Lang::Ja)
                .footer
                .contains("許可済み")
        );
    }

    #[test]
    fn a_forwarded_signature_shows_both_host_keys() {
        let mut f = facts();
        f.caller = Caller::Unknown(Unknown::Forwarded);
        f.root = None;
        f.forwarded = Some(Forwarded {
            from: "jumphost".into(),
            from_key: Some("SHA256:a".into()),
            onward_key: Some("SHA256:b".into()),
        });
        let en = build(&f, Phase::Ask, Lang::En);
        assert_eq!(
            line(&en, "Caller"),
            Some("unknown (forwarded from a remote host)")
        );
        assert_eq!(
            line(&en, "Destination"),
            Some(
                "forwarded from jumphost (host key SHA256:a) · onward to the host with key SHA256:b"
            )
        );
        assert!(en.footer.contains("this signature only"), "{}", en.footer);
        let ja = build(&f, Phase::Ask, Lang::Ja);
        assert_eq!(
            line(&ja, "接続先"),
            Some("jumphost (ホストの鍵 SHA256:a) から転送 · 行き先のホストの鍵 SHA256:b")
        );
        assert!(line(&en, "Root").is_none());
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
        let en = build(&f, Phase::Ask, Lang::En);
        assert_eq!(
            line(&en, "herdr"),
            Some("workspace \"デプロイ\" · pane w1:p2")
        );
        assert_eq!(line(&en, "WSL"), Some("Ubuntu"));
        let mut direct = facts();
        direct.caller = Caller::Unknown(Unknown::WslDirect);
        assert_eq!(
            line(&build(&direct, Phase::Ask, Lang::En), "Caller"),
            Some("unknown (a WSL ssh.exe not started through op-reason-ssh)")
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
        let t = build(&f, Phase::Asking, Lang::En);
        assert_eq!(line(&t, "For"), Some("a signature (SSHSIG, namespace git)"));
        assert_eq!(line(&t, "Key"), Some("SHA256:k"));
        f.request = Request::List;
        let t = build(&f, Phase::Asking, Lang::Ja);
        assert_eq!(line(&t, "用途"), Some("鍵の一覧"));
        assert_eq!(line(&t, "鍵"), None);
        f.request = Request::Other(17);
        assert_eq!(
            line(&build(&f, Phase::Asking, Lang::En), "For"),
            Some("agent request 17")
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

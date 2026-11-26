//! The window and toast texts, built from the same facts and the same table as the A' launcher (`Build-Ui`,
//! `Build-NotifyUi` and `Get-Texts` in `launcher/launcher.ps1`). A test reads that table out of the launcher and fails
//! when the two drift apart, so the PowerShell window and this one keep saying the same thing.
//!
//! The window of `op-reason.exe` also switches language ([`Both`], [`SWITCH`]; design
//! `2026-11-15-language-toggle-design.md` §4). The launcher's window has no switch.
//!
//! The window of `op-reason.exe` draws a [`look::View`] built next to the launcher's lines (design
//! `2026-11-26-modern-window-design.md` §4.2): the lines stay what the status `ui` reports, and the view adds the words of
//! that window alone ([`Words`]).
//!
//! Both windows list the references the call reads after the command line, and link those whose vault and item are
//! IDs to the 1Password app ([`RefLines`]; design `2026-11-17-refs-in-window-design.md` §4 and §5).

use op_reason_core::contract::{Facts, Notify, Refs};
use op_reason_core::lang::Lang;
use op_reason_core::refs;
use serde_json::{Value, json};

use crate::account;
use crate::look::{self, Band, Chip, ChipKind, Icon, Reason, Sketch, View};

/// The texts of one window and its toast (the launcher's status `ui`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texts {
    pub title: String,
    /// `(label, value)` per line.
    pub lines: Vec<(String, String)>,
    /// The references the call reads, shown before `lines[refs_at]` (after the command line, before the notes).
    pub refs: Option<RefLines>,
    pub refs_at: usize,
    pub footer: String,
    /// Shown only while another op.exe runs.
    pub others: String,
    pub toast: Vec<String>,
    /// The buttons under the footer (the SSH relay's window, [`crate::relay_ui`]); none for `op` calls.
    pub buttons: Vec<(Button, String)>,
    /// What the window of `op-reason.exe` draws ([`look::View`]).
    pub view: View,
}

/// A button of the SSH relay's window: only a press and a release on it answers (design
/// `2026-11-17-ssh-relay-design.md` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// Allow this call.
    Allow,
    /// The relay: allow this ssh of a call of Claude Code's PowerShell tool (design
    /// `2026-11-27-relay-process-tree-design.md` §4).
    AllowSsh,
    /// Allow the terminal (a person's commands) for the time limit.
    AllowTerminal,
    /// Deny.
    Deny,
}

/// The block of references: a head, the references shown (each with the URL that opens its item, when it has one),
/// "and N more", and the line that says why there are no links yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefLines {
    pub head: String,
    pub items: Vec<(String, Option<String>)>,
    pub more: Option<String>,
    pub hint: Option<String>,
}

impl Texts {
    /// The status `ui` of a desktop call, in the launcher's shape (`lines` as pairs of label and value; `refs` only
    /// when the block is shown, with `linked` instead of the URL).
    pub fn json(&self) -> Value {
        let mut ui = json!({
            "title": self.title,
            "lines": self.lines.iter().map(|(l, v)| json!([l, v])).collect::<Vec<_>>(),
            "footer": self.footer,
            "others": self.others,
            "toast": self.toast,
        });
        if let Some(r) = &self.refs {
            ui["refs"] = json!({
                "head": r.head,
                "items": r.items.iter().map(|(text, url)| json!({"text": text, "linked": url.is_some()})).collect::<Vec<_>>(),
                "more": r.more,
                "hint": r.hint,
            });
        }
        ui
    }
}

/// The texts of one desktop call in both languages: the window's switch moves between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Both {
    pub en: Texts,
    pub ja: Texts,
}

impl Both {
    /// [`build`] in English and in Japanese. `account` is the account ID for the links ([`account::pick`]).
    pub fn build(f: &Facts, account: Option<&str>) -> Both {
        Both {
            en: build(f, Lang::En, account),
            ja: build(f, Lang::Ja, account),
        }
    }

    /// The texts in `lang`.
    pub fn get(&self, lang: Lang) -> &Texts {
        match lang {
            Lang::En => &self.en,
            Lang::Ja => &self.ja,
        }
    }
}

/// The language switch: each language by its own name, side by side in a pill; the language shown is drawn bold on the
/// shown part, the other one is the half that switches ([`look::pill`]).
pub const SWITCH: [(&str, Lang); 2] = [("English", Lang::En), ("日本語", Lang::Ja)];

/// A rectangle in DIPs of the client area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    /// Whether the point is inside (the right and bottom edges excluded).
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// The toast of a `notify` call (a shared approval found by the WSL watchdog).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyTexts {
    pub title: String,
    /// The time line: `(label, time)`.
    pub time: (String, String),
    /// Title and body.
    pub toast: Vec<String>,
}

impl NotifyTexts {
    /// The status `ui` of a notify call, in the launcher's shape.
    pub fn json(&self) -> Value {
        json!({
            "title": self.title,
            "lines": [[self.time.0, self.time.1]],
            "toast": self.toast,
        })
    }
}

/// The entries of the launcher's `Get-Texts` that the window and the toast use (same keys).
#[derive(Debug)]
pub struct Table {
    pub title: &'static str,
    pub time: &'static str,
    pub caller: &'static str,
    pub herdr: &'static str,
    pub place: &'static str,
    pub reason: &'static str,
    pub command: &'static str,
    pub note: &'static str,
    pub human: &'static str,
    pub human_short: &'static str,
    pub herdr_line: &'static str,
    pub footer: &'static str,
    pub others: &'static str,
    pub toast_reason: &'static str,
    pub shared_cleared: &'static str,
    pub lock_skipped: &'static str,
    pub cleared_title: &'static str,
    pub cleared_body: &'static str,
    pub failed_title: &'static str,
    pub failed_body: &'static str,
    pub when_start: &'static str,
    pub when_end: &'static str,
    pub refs: &'static str,
    pub refs_args: &'static str,
    pub refs_stdin: &'static str,
    pub refs_sep: &'static str,
    pub refs_more: &'static str,
    pub refs_hint: &'static str,
}

pub const EN: Table = Table {
    title: "1Password CLI approval request",
    time: "Time",
    caller: "Caller",
    herdr: "herdr",
    place: "Place",
    reason: "Reason",
    command: "Command",
    note: "Note",
    human: "Human (terminal)",
    human_short: "Human",
    herdr_line: "workspace \"{0}\" · pane {1}",
    footer: "Deny any 1Password approval prompt that comes without this window",
    others: "Other op.exe processes are running (without this window). If you cannot tell which approval this is, deny it",
    toast_reason: "Reason: {0}",
    shared_cleared: "Before this call, an approval usable from every WSL pane was left; it was cleared",
    lock_skipped: "The op call after this one in the pipe (reason \"{0}\") holds the lock and waits for this output. This call runs without the lock, so that call's window is open at the same time",
    cleared_title: "Cleared a shared 1Password CLI approval",
    cleared_body: "An approval usable from every WSL pane was left, so op-reason ran op.exe signout --all ({0}). Someone may have allowed a prompt without the op-reason window, or a tool ran op.exe directly.",
    failed_title: "Could not clear a shared 1Password CLI approval",
    failed_body: "An approval usable from every WSL pane is left ({0}). Run op.exe signout --all in WSL, or lock 1Password.",
    when_start: "before the call",
    when_end: "after the call",
    refs: "References to read ({0}, from {1})",
    refs_args: "arguments",
    refs_stdin: "stdin",
    refs_sep: ", ",
    refs_more: "and {0} more",
    refs_hint: "Links to the items appear once an approved call has finished (op-reason then learns the ID of the account op uses by default)",
};

pub const JA: Table = Table {
    title: "1Password CLI の承認の依頼",
    time: "時刻",
    caller: "呼び手",
    herdr: "herdr",
    place: "場所",
    reason: "理由",
    command: "コマンド",
    note: "注意",
    human: "人 (端末)",
    human_short: "人",
    herdr_line: "workspace「{0}」 · pane {1}",
    footer: "この窓が出ていない 1Password の承認は拒否してください",
    others: "ほかにも op.exe が動いています (この窓の無いもの)。どれの承認か分からなければ拒否してください",
    toast_reason: "理由: {0}",
    shared_cleared: "打つ前に、WSL のどの pane からも使える承認が残っていたので消しました",
    lock_skipped: "パイプの先の op (理由「{0}」) が lock を持ったまま、この op の出力を待っています。lock を待たずに打つので、その op の窓も同時に出ています",
    cleared_title: "共有の 1Password CLI の承認を消しました",
    cleared_body: "WSL のどの pane からも使える承認が残っていたので、op.exe signout --all で消しました ({0})。窓の出ていない承認の画面を許可したか、道具が op.exe を直に打ったのかもしれません。",
    failed_title: "共有の 1Password CLI の承認を消せませんでした",
    failed_body: "WSL のどの pane からも使える承認が残っています ({0})。WSL で op.exe signout --all を打つか、1Password をロックしてください。",
    when_start: "op を打つ前",
    when_end: "op を打った後",
    refs: "読む参照 ({0} 件・{1} から)",
    refs_args: "引数",
    refs_stdin: "標準入力",
    refs_sep: "、",
    refs_more: "ほか {0} 件",
    refs_hint: "項目へのリンクは、承認が通った呼び出しを 1 回終えると付きます (そのとき、op が既定で使うアカウントの ID を覚えます)",
};

pub fn table(lang: Lang) -> &'static Table {
    match lang {
        Lang::En => &EN,
        Lang::Ja => &JA,
    }
}

/// The words of `op-reason.exe`'s window that the launcher's window does not have (design
/// `2026-11-26-modern-window-design.md` §5). The band's words in `⟦…⟧` and `⟪…⟫` are drawn in the accent and the caution
/// color ([`look::marked`]).
#[derive(Debug)]
pub struct Words {
    pub subtitle: &'static str,
    pub band_title: &'static str,
    pub band_line: &'static str,
    pub no_reason: &'static str,
}

pub const WORDS_EN: Words = Words {
    subtitle: "op-reason · asking 1Password for approval",
    band_title: "1Password's prompt usually opens in the ⟦middle of the screen⟧",
    band_line: "If it does not, open it from 1Password in the ⟪tray at the bottom right⟫ (under ^)",
    no_reason: "no reason given",
};

pub const WORDS_JA: Words = Words {
    subtitle: "op-reason · 1Password に承認を求めています",
    band_title: "1Password の画面は、ふつう⟦画面の中央⟧に出ます",
    band_line: "出ないときは⟪右下のトレイ⟫の 1Password (^ の中) から",
    no_reason: "理由なし",
};

pub fn words(lang: Lang) -> &'static Words {
    match lang {
        Lang::En => &WORDS_EN,
        Lang::Ja => &WORDS_JA,
    }
}

/// The launcher's `Get-Clip`: text longer than `n` UTF-16 units is cut (never inside a surrogate pair) and gets `…`.
pub fn clip_utf16(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    if units.len() <= n {
        return s.to_owned();
    }
    let mut cut = n;
    if cut > 0 && (0xD800..0xDC00).contains(&units[cut - 1]) {
        cut -= 1;
    }
    format!("{}…", String::from_utf16_lossy(&units[..cut]))
}

/// PowerShell's `-f` for the `{0}` and `{1}` of the table.
fn fill(template: &str, args: &[&str]) -> String {
    let mut out = template.to_owned();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), a);
    }
    out
}

/// The launcher's `Build-Ui`. `account` is the account ID for the links ([`account::pick`]), when one is known.
pub fn build(f: &Facts, lang: Lang, account: Option<&str>) -> Texts {
    let t = table(lang);
    let view = view(f, lang);
    let mut lines = vec![(t.time.to_owned(), f.time.clone())];
    let (mut desc, mut short) = if f.caller.kind == "claude" {
        let desc = match f.caller.pid {
            Some(pid) => format!("Claude Code (pid {pid})"),
            None => "Claude Code".to_owned(),
        };
        (desc, "Claude Code".to_owned())
    } else {
        (t.human.to_owned(), t.human_short.to_owned())
    };
    if let Some(d) = f.caller.detail.as_deref().filter(|d| !d.is_empty()) {
        desc = format!("{desc} · {d}");
    }
    lines.push((t.caller.to_owned(), desc));
    if let Some(h) = &f.herdr {
        let label = if h.label.is_empty() { "?" } else { &h.label };
        let pane = if h.pane.is_empty() { "?" } else { &h.pane };
        lines.push((t.herdr.to_owned(), fill(t.herdr_line, &[label, pane])));
        if !h.label.is_empty() {
            short = format!("{short} · {}", h.label);
        }
    }
    let mut place = f.place.cwd.clone();
    if let Some(repo) = f.place.repo.as_deref().filter(|r| !r.is_empty()) {
        let branch = f
            .place
            .branch
            .as_deref()
            .filter(|b| !b.is_empty())
            .unwrap_or("?");
        place = format!("{repo} ({branch}) · {place}");
    }
    lines.push((t.place.to_owned(), place));
    lines.push((t.reason.to_owned(), f.reason.clone()));
    lines.push((t.command.to_owned(), f.command.clone()));
    let refs_at = lines.len();
    let refs = f
        .refs
        .as_ref()
        .filter(|r| !r.list.is_empty())
        .map(|r| ref_lines(t, r, account));
    for n in &f.notes {
        match n.kind.as_str() {
            "shared-cleared" => lines.push((t.note.to_owned(), t.shared_cleared.to_owned())),
            "lock-skipped" => lines.push((
                t.note.to_owned(),
                fill(t.lock_skipped, &[n.holder_reason.as_deref().unwrap_or("")]),
            )),
            _ => {}
        }
    }
    let toast = vec![
        t.title.to_owned(),
        fill(t.toast_reason, &[&clip_utf16(&f.reason, 100)]),
        format!("{short} · {}", clip_utf16(&f.command, 80)),
    ];
    Texts {
        title: t.title.to_owned(),
        lines,
        refs,
        refs_at,
        footer: t.footer.to_owned(),
        others: t.others.to_owned(),
        toast,
        buttons: Vec::new(),
        view,
    }
}

/// The window's view of a call (design `2026-11-26-modern-window-design.md` §4.2): the reason first, the caller,
/// herdr, the repository and the directory as chips, the command in the box, the notes, the band and the footer.
fn view(f: &Facts, lang: Lang) -> View {
    let t = table(lang);
    let w = words(lang);
    let (time, date) = look::split_time(&f.time);
    let mut chips = Vec::new();
    let (kind, mut caller) = if f.caller.kind == "claude" {
        let c = match f.caller.pid {
            Some(pid) => format!("Claude Code (pid {pid})"),
            None => "Claude Code".to_owned(),
        };
        (ChipKind::Claude, c)
    } else {
        (ChipKind::Person, t.human.to_owned())
    };
    if let Some(d) = f.caller.detail.as_deref().filter(|d| !d.is_empty()) {
        caller = format!("{caller} · {d}");
    }
    chips.push(Chip { kind, text: caller });
    if let Some(h) = &f.herdr {
        let label = if h.label.is_empty() { "?" } else { &h.label };
        let pane = if h.pane.is_empty() { "?" } else { &h.pane };
        chips.push(Chip {
            kind: ChipKind::Herdr,
            text: format!("herdr · {label} · {pane}"),
        });
    }
    if let Some(repo) = f.place.repo.as_deref().filter(|r| !r.is_empty()) {
        let branch = f
            .place
            .branch
            .as_deref()
            .filter(|b| !b.is_empty())
            .unwrap_or("?");
        chips.push(Chip {
            kind: ChipKind::Repo,
            text: format!("{repo} · {branch}"),
        });
    }
    if !f.place.cwd.is_empty() {
        chips.push(Chip {
            kind: ChipKind::Folder,
            text: f.place.cwd.clone(),
        });
    }
    let notes = f
        .notes
        .iter()
        .filter_map(|n| match n.kind.as_str() {
            "shared-cleared" => Some(t.shared_cleared.to_owned()),
            "lock-skipped" => Some(fill(
                t.lock_skipped,
                &[n.holder_reason.as_deref().unwrap_or("")],
            )),
            _ => None,
        })
        .collect();
    View {
        icon: Icon::Lock,
        subtitle: w.subtitle.to_owned(),
        time,
        date,
        reason_label: t.reason.to_owned(),
        reason: if f.reason.is_empty() {
            Reason::Missing(w.no_reason.to_owned())
        } else {
            Reason::Given(f.reason.clone())
        },
        chips,
        code_label: t.command.to_owned(),
        code: f.command.clone(),
        code_note: None,
        tree: None,
        tree_short: None,
        facts: Vec::new(),
        notes,
        countdown: None,
        status: String::new(),
        band: Band {
            title: w.band_title.to_owned(),
            line: w.band_line.to_owned(),
            sketch: Sketch::Op,
            keep: false,
            alert: false,
        },
        footer: t.footer.to_owned(),
    }
}

/// The launcher's `Build-RefLines`: the head with the count and the sources, at most [`refs::SHOWN`] references (an ID
/// reference gets the URL of its item when the account is known), "and N more", and the hint when a reference shown
/// could be a link but no account is known.
fn ref_lines(t: &Table, r: &Refs, account: Option<&str>) -> RefLines {
    let from: Vec<String> = r
        .from
        .iter()
        .filter_map(|s| match s.kind.as_str() {
            "args" => Some(t.refs_args.to_owned()),
            "stdin" => Some(t.refs_stdin.to_owned()),
            "file" | "env-file" => s.name.clone().filter(|n| !n.is_empty()),
            _ => None,
        })
        .collect();
    let from = if from.is_empty() {
        "?".to_owned()
    } else {
        from.join(t.refs_sep)
    };
    let total = r.total.max(r.list.len());
    let shown = &r.list[..r.list.len().min(refs::SHOWN)];
    let items: Vec<(String, Option<String>)> = shown
        .iter()
        .map(|text| {
            let url = account.and_then(|a| {
                refs::link_ids(text).and_then(|(vault, item)| account::item_url(a, vault, item))
            });
            (text.clone(), url)
        })
        .collect();
    let could_link = shown.iter().any(|text| refs::link_ids(text).is_some());
    RefLines {
        head: fill(t.refs, &[&total.to_string(), &from]),
        more: (total > items.len())
            .then(|| fill(t.refs_more, &[&(total - items.len()).to_string()])),
        hint: (could_link && items.iter().all(|(_, url)| url.is_none()))
            .then(|| t.refs_hint.to_owned()),
        items,
    }
}

/// The launcher's `Get-CallerShort`: Claude Code or a person, and the herdr label (for the holder file).
pub fn caller_short(f: Option<&Facts>, lang: Lang) -> String {
    let t = table(lang);
    let mut short = match f {
        Some(f) if f.caller.kind == "claude" => "Claude Code".to_owned(),
        _ => t.human_short.to_owned(),
    };
    if let Some(label) = f
        .and_then(|f| f.herdr.as_ref())
        .map(|h| h.label.as_str())
        .filter(|l| !l.is_empty())
    {
        short = format!("{short} · {label}");
    }
    short
}

/// The launcher's `Build-NotifyUi`.
pub fn build_notify(n: &Notify, lang: Lang) -> NotifyTexts {
    let t = table(lang);
    let when = if n.when == "start" {
        t.when_start
    } else {
        t.when_end
    };
    let (title, body) = if n.cleared {
        (t.cleared_title, fill(t.cleared_body, &[when]))
    } else {
        (t.failed_title, fill(t.failed_body, &[when]))
    };
    NotifyTexts {
        title: title.to_owned(),
        time: (t.time.to_owned(), n.time.clone()),
        toast: vec![title.to_owned(), body],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_reason_core::contract::{FactsCaller, Herdr, Note, Place, RefSource};
    use std::collections::HashMap;

    /// `key = 'value'` lines of `Get-Texts`, Japanese block first, then English.
    fn launcher_tables() -> (HashMap<String, String>, HashMap<String, String>) {
        let src = include_str!("../../../launcher/launcher.ps1");
        let start = src
            .find("function Get-Texts")
            .expect("Get-Texts in launcher.ps1");
        let body = &src[start..];
        let body = &body[..body.find("\n}\n").expect("end of Get-Texts")];
        let mut ja = HashMap::new();
        let mut en = HashMap::new();
        let mut seen_title = 0;
        for line in body.lines() {
            let Some((key, rest)) = line.trim().split_once('=') else {
                continue;
            };
            let key = key.trim();
            let rest = rest.trim();
            let Some(value) = rest.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) else {
                continue;
            };
            if key == "title" {
                seen_title += 1;
            }
            let map = if seen_title <= 1 { &mut ja } else { &mut en };
            map.insert(key.to_owned(), value.replace("''", "'"));
        }
        (ja, en)
    }

    fn pairs(t: &Table) -> Vec<(&'static str, &'static str)> {
        vec![
            ("title", t.title),
            ("time", t.time),
            ("caller", t.caller),
            ("herdr", t.herdr),
            ("place", t.place),
            ("reason", t.reason),
            ("command", t.command),
            ("note", t.note),
            ("human", t.human),
            ("humanShort", t.human_short),
            ("herdrLine", t.herdr_line),
            ("footer", t.footer),
            ("others", t.others),
            ("toastReason", t.toast_reason),
            ("sharedCleared", t.shared_cleared),
            ("lockSkipped", t.lock_skipped),
            ("clearedTitle", t.cleared_title),
            ("clearedBody", t.cleared_body),
            ("failedTitle", t.failed_title),
            ("failedBody", t.failed_body),
            ("whenStart", t.when_start),
            ("whenEnd", t.when_end),
            ("refs", t.refs),
            ("refsArgs", t.refs_args),
            ("refsStdin", t.refs_stdin),
            ("refsSep", t.refs_sep),
            ("refsMore", t.refs_more),
            ("refsHint", t.refs_hint),
        ]
    }

    #[test]
    fn table_matches_the_launcher() {
        let (ja, en) = launcher_tables();
        for (lang, ours, theirs) in [("ja", &JA, &ja), ("en", &EN, &en)] {
            for (key, value) in pairs(ours) {
                assert_eq!(
                    theirs.get(key).map(String::as_str),
                    Some(value),
                    "{lang}.{key} differs from launcher/launcher.ps1"
                );
            }
        }
    }

    fn facts() -> Facts {
        Facts {
            time: "2026-11-15 06:05:00".into(),
            caller: FactsCaller {
                kind: "claude".into(),
                pid: Some(42),
                session: Some("s".into()),
                detail: Some("Windows Terminal".into()),
            },
            herdr: None,
            place: Place {
                repo: Some("me/r".into()),
                branch: None,
                cwd: "~\\src\\r".into(),
            },
            reason: "r".repeat(150),
            command: "op vault list".into(),
            notes: vec![Note {
                kind: "lock-skipped".into(),
                holder_reason: Some("why".into()),
            }],
            refs: None,
        }
    }

    #[test]
    fn builds_like_the_launcher() {
        let t = build(&facts(), Lang::Ja, None);
        assert_eq!(t.title, "1Password CLI の承認の依頼");
        let labels: Vec<&str> = t.lines.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            labels,
            ["時刻", "呼び手", "場所", "理由", "コマンド", "注意"]
        );
        assert_eq!(t.lines[1].1, "Claude Code (pid 42) · Windows Terminal");
        assert_eq!(t.lines[2].1, "me/r (?) · ~\\src\\r");
        assert!(t.lines[5].1.contains("理由「why」"));
        assert_eq!(t.toast[1].chars().count(), "理由: ".chars().count() + 101);
        assert_eq!(t.toast[2], "Claude Code · op vault list");
    }

    #[test]
    fn people_and_herdr() {
        let mut f = facts();
        f.caller = FactsCaller {
            kind: "human".into(),
            ..FactsCaller::default()
        };
        f.herdr = Some(Herdr {
            workspace: "w".into(),
            label: "lab".into(),
            pane: String::new(),
        });
        let t = build(&f, Lang::En, None);
        assert_eq!(t.lines[1].1, "Human (terminal)");
        assert_eq!(
            t.lines[2],
            ("herdr".into(), "workspace \"lab\" · pane ?".into())
        );
        assert!(t.toast[2].starts_with("Human · lab · "));
    }

    #[test]
    fn notify_texts_and_the_status_shapes() {
        let n = Notify {
            kind: "shared-approval".into(),
            when: "start".into(),
            cleared: true,
            time: "12:34:56".into(),
        };
        let t = build_notify(&n, Lang::En);
        assert_eq!(t.title, "Cleared a shared 1Password CLI approval");
        assert!(t.toast[1].contains("(before the call)"), "{:?}", t.toast);
        assert_eq!(
            t.json(),
            json!({"title": t.title, "lines": [["Time", "12:34:56"]], "toast": t.toast})
        );
        let failed = build_notify(
            &Notify {
                when: "end".into(),
                cleared: false,
                ..n
            },
            Lang::Ja,
        );
        assert_eq!(
            failed.title,
            "共有の 1Password CLI の承認を消せませんでした"
        );
        assert!(failed.toast[1].contains("(op を打った後)"));
        let ui = build(&facts(), Lang::En, None).json();
        assert_eq!(ui["lines"][0], json!(["Time", "2026-11-15 06:05:00"]));
        assert_eq!(ui["toast"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn caller_short_like_the_launcher() {
        let mut f = facts();
        assert_eq!(caller_short(Some(&f), Lang::Ja), "Claude Code");
        f.caller.kind = "human".into();
        f.herdr = Some(Herdr {
            workspace: "w".into(),
            label: "lab".into(),
            pane: "p".into(),
        });
        assert_eq!(caller_short(Some(&f), Lang::Ja), "人 · lab");
        assert_eq!(caller_short(None, Lang::En), "Human");
    }

    const ACCOUNT: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const ID_REF: &str = "op://abcdefghijklmnopqrstuvwxyz/0123456789abcdefghijklmnop/password";

    fn with_refs(list: &[&str], total: usize, from: Vec<RefSource>) -> Facts {
        let mut f = facts();
        f.refs = Some(Refs {
            total,
            from,
            list: list.iter().map(|r| r.to_string()).collect(),
            account: None,
        });
        f
    }

    fn src(kind: &str, name: Option<&str>) -> RefSource {
        RefSource {
            kind: kind.into(),
            name: name.map(str::to_owned),
        }
    }

    #[test]
    fn the_references_after_the_command_with_links_once_the_account_is_known() {
        let mut list = vec![
            ID_REF,
            "op://Private/proxmox/token",
            "op://$ENV/db/password",
        ];
        list.extend(["op://v/i/f"; 5]);
        let f = with_refs(
            &list,
            11,
            vec![
                src("args", None),
                src("file", Some("app.tpl")),
                src("stdin", None),
            ],
        );
        let t = build(&f, Lang::Ja, Some(ACCOUNT));
        assert_eq!(
            t.lines[t.refs_at - 1].0,
            "コマンド",
            "right after the command line"
        );
        assert_eq!(t.lines[t.refs_at].0, "注意", "before the notes");
        let r = t.refs.as_ref().unwrap();
        assert_eq!(r.head, "読む参照 (11 件・引数、app.tpl、標準入力 から)");
        assert_eq!(r.items.len(), 8);
        assert_eq!(
            r.items[0].1.as_deref(),
            Some(
                "onepassword://view-item?a=ABCDEFGHIJKLMNOPQRSTUVWXYZ&v=abcdefghijklmnopqrstuvwxyz&i=0123456789abcdefghijklmnop"
            )
        );
        assert!(
            r.items[1..].iter().all(|(_, url)| url.is_none()),
            "names and variables stay plain"
        );
        assert_eq!(r.more.as_deref(), Some("ほか 3 件"));
        assert_eq!(r.hint, None);
        let en = build(&f, Lang::En, Some(ACCOUNT));
        let r = en.refs.as_ref().unwrap();
        assert_eq!(
            r.head,
            "References to read (11, from arguments, app.tpl, stdin)"
        );
        assert_eq!(r.more.as_deref(), Some("and 3 more"));
        assert_eq!(
            en.json()["refs"],
            json!({
                "head": r.head,
                "items": r.items.iter().map(|(text, url)| json!({"text": text, "linked": url.is_some()})).collect::<Vec<_>>(),
                "more": "and 3 more",
                "hint": null,
            })
        );
        assert_eq!(en.json()["refs"]["items"][0]["linked"], true);
    }

    #[test]
    fn the_hint_only_when_a_reference_could_be_a_link() {
        let f = with_refs(
            &[ID_REF, "op://Private/proxmox/token"],
            2,
            vec![src("env-file", Some(".env"))],
        );
        let t = build(&f, Lang::En, None);
        let r = t.refs.as_ref().unwrap();
        assert_eq!(r.head, "References to read (2, from .env)");
        assert!(r.items.iter().all(|(_, url)| url.is_none()));
        assert_eq!(r.more, None);
        assert_eq!(r.hint.as_deref(), Some(EN.refs_hint));
        assert_eq!(
            build(&f, Lang::Ja, None).refs.unwrap().hint.as_deref(),
            Some(JA.refs_hint)
        );
        let names = with_refs(&["op://Private/proxmox/token"], 1, vec![src("args", None)]);
        assert_eq!(build(&names, Lang::En, None).refs.unwrap().hint, None);
        assert_eq!(
            build(&f, Lang::En, Some("not-an-account"))
                .refs
                .unwrap()
                .hint
                .as_deref(),
            Some(EN.refs_hint),
            "an account that cannot build a URL gives no links"
        );
    }

    #[test]
    fn no_block_without_references() {
        let t = build(&facts(), Lang::En, Some(ACCOUNT));
        assert_eq!(t.refs, None);
        assert!(t.json().get("refs").is_none());
        let empty = with_refs(&[], 0, vec![]);
        assert_eq!(build(&empty, Lang::En, None).refs, None);
    }

    #[test]
    fn the_shared_link_table_through_the_texts() {
        let table: Value =
            serde_json::from_str(include_str!("../../../tests/cases/refs.json")).unwrap();
        for l in table["links"].as_array().unwrap() {
            let r = l["ref"].as_str().unwrap();
            let f = with_refs(&[r], 1, vec![src("args", None)]);
            let linked = build(&f, Lang::En, Some(ACCOUNT)).refs.unwrap().items[0]
                .1
                .is_some();
            assert_eq!(linked, l["vault"].is_string(), "{r}");
        }
    }

    #[test]
    fn both_languages_for_the_switch() {
        let both = Both::build(&facts(), None);
        assert_eq!(both.get(Lang::En).title, "1Password CLI approval request");
        assert_eq!(both.get(Lang::Ja).title, "1Password CLI の承認の依頼");
        assert_eq!(both.get(Lang::Ja), &build(&facts(), Lang::Ja, None));
        assert_eq!(both.en.lines.len(), both.ja.lines.len());
        let labels: Vec<&str> = SWITCH.iter().map(|(t, _)| *t).collect();
        assert_eq!(labels, ["English", "日本語"]);
    }

    #[test]
    fn the_view_of_a_call() {
        let v = build(&facts(), Lang::Ja, None).view;
        assert_eq!(v.icon, Icon::Lock);
        assert_eq!(v.subtitle, "op-reason · 1Password に承認を求めています");
        assert_eq!(
            (v.time.as_str(), v.date.as_str()),
            ("06:05:00", "2026-11-15")
        );
        assert_eq!(v.reason_label, "理由");
        assert_eq!(v.reason, Reason::Given("r".repeat(150)));
        assert_eq!(
            v.chips,
            vec![
                Chip {
                    kind: ChipKind::Claude,
                    text: "Claude Code (pid 42) · Windows Terminal".into()
                },
                Chip {
                    kind: ChipKind::Repo,
                    text: "me/r · ?".into()
                },
                Chip {
                    kind: ChipKind::Folder,
                    text: "~\\src\\r".into()
                },
            ]
        );
        assert_eq!(v.code_label, "コマンド");
        assert_eq!(v.code, "op vault list");
        assert_eq!(v.code_note, None);
        assert!(v.facts.is_empty());
        assert_eq!(v.notes.len(), 1);
        assert!(v.notes[0].contains("理由「why」"), "{:?}", v.notes);
        assert_eq!(v.countdown, None);
        assert_eq!(v.status, "");
        assert_eq!(v.band.sketch, Sketch::Op);
        assert_eq!(
            look::marked(&v.band.title).text,
            "1Password の画面は、ふつう画面の中央に出ます"
        );
        assert_eq!(
            look::marked(&v.band.line).text,
            "出ないときは右下のトレイの 1Password (^ の中) から"
        );
        assert_eq!(v.footer, JA.footer);
        let en = build(&facts(), Lang::En, None).view;
        assert_eq!(en.subtitle, "op-reason · asking 1Password for approval");
        assert_eq!(
            look::marked(&en.band.title).text,
            "1Password's prompt usually opens in the middle of the screen"
        );
        assert_eq!(en.footer, EN.footer);
    }

    #[test]
    fn herdr_and_a_person_in_chips() {
        let mut f = facts();
        f.caller = FactsCaller {
            kind: "human".into(),
            ..FactsCaller::default()
        };
        f.herdr = Some(Herdr {
            workspace: "w".into(),
            label: "lab".into(),
            pane: String::new(),
        });
        let v = build(&f, Lang::En, None).view;
        assert_eq!(
            v.chips[..2],
            [
                Chip {
                    kind: ChipKind::Person,
                    text: "Human (terminal)".into()
                },
                Chip {
                    kind: ChipKind::Herdr,
                    text: "herdr · lab · ?".into()
                },
            ]
        );
        f.reason = String::new();
        assert_eq!(
            build(&f, Lang::Ja, None).view.reason,
            Reason::Missing("理由なし".into())
        );
    }

    #[test]
    fn the_status_ui_keeps_the_launchers_shape() {
        let t = build(&facts(), Lang::En, None);
        let ui = t.json();
        let keys: Vec<&String> = ui.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["footer", "lines", "others", "title", "toast"]);
        assert_eq!(ui["lines"].as_array().map(Vec::len), Some(t.lines.len()));
    }

    #[test]
    fn clip_keeps_surrogate_pairs_whole() {
        assert_eq!(clip_utf16("abc", 3), "abc");
        assert_eq!(clip_utf16("abcd", 3), "abc…");
        // 😀 is two units: cutting after one would split it
        assert_eq!(clip_utf16("a😀b", 2), "a…");
    }
}

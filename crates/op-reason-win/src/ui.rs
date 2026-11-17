//! The window and toast texts, built from the same facts and the same table as the A' launcher (`Build-Ui`,
//! `Build-NotifyUi` and `Get-Texts` in `launcher/launcher.ps1`). A test reads that table out of the launcher and fails
//! when the two drift apart, so the PowerShell window and this one keep saying the same thing.
//!
//! The window of `op-reason.exe` also switches language ([`Both`], [`SWITCH_LABELS`], [`switch_hit`]; design
//! `2026-11-15-language-toggle-design.md` §4). The launcher's window has no switch.

use op_reason_core::contract::{Facts, Notify};
use op_reason_core::lang::Lang;
use serde_json::{Value, json};

/// The texts of one window and its toast (the launcher's status `ui`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texts {
    pub title: String,
    /// `(label, value)` per line.
    pub lines: Vec<(String, String)>,
    pub footer: String,
    /// Shown only while another op.exe runs.
    pub others: String,
    pub toast: Vec<String>,
}

impl Texts {
    /// The status `ui` of a desktop call, in the launcher's shape (`lines` as pairs of label and value).
    pub fn json(&self) -> Value {
        json!({
            "title": self.title,
            "lines": self.lines.iter().map(|(l, v)| json!([l, v])).collect::<Vec<_>>(),
            "footer": self.footer,
            "others": self.others,
            "toast": self.toast,
        })
    }
}

/// The texts of one desktop call in both languages: the window's switch moves between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Both {
    pub en: Texts,
    pub ja: Texts,
}

impl Both {
    /// [`build`] in English and in Japanese.
    pub fn build(f: &Facts) -> Both {
        Both {
            en: build(f, Lang::En),
            ja: build(f, Lang::Ja),
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

/// The language switch at the right of the footer: each language by its own name, with a bar between them. The
/// language shown is drawn bold; the other one is the link that switches.
pub const SWITCH_LABELS: [(&str, Option<Lang>); 3] = [
    ("English", Some(Lang::En)),
    (" | ", None),
    ("日本語", Some(Lang::Ja)),
];

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

/// One part of the switch as laid out: its width, its line height and baseline (DIPs, from DirectWrite), and the
/// language it stands for (none for the bar).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchPart {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub lang: Option<Lang>,
}

/// Where each part of the switch goes: the switch ends at `right`, its parts on the baseline `baseline` (all in DIPs).
/// Returns the top left corner of each part's line box.
pub fn switch_origins(parts: &[SwitchPart], right: f32, baseline: f32) -> Vec<(f32, f32)> {
    let mut x = right - parts.iter().map(|p| p.width).sum::<f32>();
    parts
        .iter()
        .map(|p| {
            let at = (x, baseline - p.baseline);
            x += p.width;
            at
        })
        .collect()
}

/// The language a click at (`x`, `y`) chooses: the part of the language not `shown`, its line box widened by
/// `pad_x` on each side and `pad_y` above and below. `None` elsewhere, and on the language shown.
pub fn switch_hit(
    parts: &[SwitchPart],
    right: f32,
    baseline: f32,
    shown: Lang,
    (pad_x, pad_y): (f32, f32),
    (x, y): (f32, f32),
) -> Option<Lang> {
    parts
        .iter()
        .zip(switch_origins(parts, right, baseline))
        .find_map(|(p, (left, top))| {
            let lang = p.lang.filter(|l| *l != shown)?;
            let r = Rect {
                left: left - pad_x,
                top: top - pad_y,
                right: left + p.width + pad_x,
                bottom: top + p.height + pad_y,
            };
            r.contains(x, y).then_some(lang)
        })
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
};

pub fn table(lang: Lang) -> &'static Table {
    match lang {
        Lang::En => &EN,
        Lang::Ja => &JA,
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

/// The launcher's `Build-Ui`.
pub fn build(f: &Facts, lang: Lang) -> Texts {
    let t = table(lang);
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
        footer: t.footer.to_owned(),
        others: t.others.to_owned(),
        toast,
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
    use op_reason_core::contract::{FactsCaller, Herdr, Note, Place};
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
        let t = build(&facts(), Lang::Ja);
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
        let t = build(&f, Lang::En);
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
        let ui = build(&facts(), Lang::En).json();
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

    #[test]
    fn both_languages_for_the_switch() {
        let both = Both::build(&facts());
        assert_eq!(both.get(Lang::En).title, "1Password CLI approval request");
        assert_eq!(both.get(Lang::Ja).title, "1Password CLI の承認の依頼");
        assert_eq!(both.get(Lang::Ja), &build(&facts(), Lang::Ja));
        assert_eq!(both.en.lines.len(), both.ja.lines.len());
        let labels: Vec<&str> = SWITCH_LABELS.iter().map(|(t, _)| *t).collect();
        assert_eq!(labels.concat(), "English | 日本語");
    }

    fn parts() -> Vec<SwitchPart> {
        // English 40 wide, the bar 12, 日本語 34 (a taller line from a fallback font, its baseline lower)
        vec![
            SwitchPart {
                width: 40.0,
                height: 15.0,
                baseline: 12.0,
                lang: Some(Lang::En),
            },
            SwitchPart {
                width: 12.0,
                height: 15.0,
                baseline: 12.0,
                lang: None,
            },
            SwitchPart {
                width: 34.0,
                height: 18.0,
                baseline: 14.0,
                lang: Some(Lang::Ja),
            },
        ]
    }

    #[test]
    fn the_switch_ends_at_the_right_edge_on_one_baseline() {
        let at = switch_origins(&parts(), 500.0, 100.0);
        assert_eq!(at, vec![(414.0, 88.0), (454.0, 88.0), (466.0, 86.0)]);
    }

    #[test]
    fn only_the_language_not_shown_is_clickable_with_some_room_around_it() {
        let hit = |shown, x, y| switch_hit(&parts(), 500.0, 100.0, shown, (4.0, 3.0), (x, y));
        // 日本語 spans x 466..500, y 86..104 (+4 / +3 around it)
        assert_eq!(hit(Lang::En, 480.0, 95.0), Some(Lang::Ja));
        assert_eq!(
            hit(Lang::En, 462.5, 83.5),
            Some(Lang::Ja),
            "inside the room around it"
        );
        assert_eq!(hit(Lang::En, 503.9, 106.9), Some(Lang::Ja));
        assert_eq!(
            hit(Lang::En, 504.0, 95.0),
            None,
            "past the room on the right"
        );
        assert_eq!(hit(Lang::En, 480.0, 107.0), None, "below the room");
        assert_eq!(
            hit(Lang::En, 420.0, 95.0),
            None,
            "English is shown: not a link"
        );
        assert_eq!(hit(Lang::En, 459.0, 95.0), None, "the bar");
        assert_eq!(hit(Lang::Ja, 420.0, 95.0), Some(Lang::En));
        assert_eq!(
            hit(Lang::Ja, 480.0, 95.0),
            None,
            "日本語 is shown: not a link"
        );
    }

    #[test]
    fn clip_keeps_surrogate_pairs_whole() {
        assert_eq!(clip_utf16("abc", 3), "abc");
        assert_eq!(clip_utf16("abcd", 3), "abc…");
        // 😀 is two units: cutting after one would split it
        assert_eq!(clip_utf16("a😀b", 2), "a…");
    }
}

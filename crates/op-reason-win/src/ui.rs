//! The window and toast texts, built from the same facts and the same table as the A' launcher (`Build-Ui` and
//! `Get-Texts` in `launcher/launcher.ps1`). A test reads that table out of the launcher and fails when the two drift
//! apart, so the PowerShell window and this one keep saying the same thing.

use op_reason_core::contract::Facts;
use op_reason_core::lang::Lang;

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
    fn clip_keeps_surrogate_pairs_whole() {
        assert_eq!(clip_utf16("abc", 3), "abc");
        assert_eq!(clip_utf16("abcd", 3), "abc…");
        // 😀 is two units: cutting after one would split it
        assert_eq!(clip_utf16("a😀b", 2), "a…");
    }
}

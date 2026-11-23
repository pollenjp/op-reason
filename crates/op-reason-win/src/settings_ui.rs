//! The settings window's texts and lines (design `2026-11-23-settings-design.md` §11): portable, so that Linux tests
//! cover them. The window itself is `win::settings::window`.

use op_reason_core::lang::Lang;
use op_reason_core::settings::catalog::Name;
use op_reason_core::settings::engine::{Change, Item, State};
use op_reason_core::settings::skill::SkillState;
use op_reason_core::settings::text;

/// The window's fixed texts in one language.
pub struct Texts {
    pub title: &'static str,
    pub language: &'static str,
    /// English, Japanese, and "from OPREASON_LANG" (in this order: the language combo box's items).
    pub lang_choices: [&'static str; 3],
    pub relay: &'static str,
    pub start: &'static str,
    pub stop: &'static str,
    pub restart: &'static str,
    pub autostart: &'static str,
    pub tray_icon: &'static str,
    pub mode: &'static str,
    pub time_limit: &'static str,
    pub answer_wait: &'static str,
    pub apply: &'static str,
    pub apply_note: &'static str,
    pub skill: &'static str,
    /// The skill list's columns: place, state, where.
    pub columns: [&'static str; 3],
    pub place_or_update: &'static str,
    pub remove: &'static str,
    pub check: &'static str,
    pub add_folder: &'static str,
    pub start_menu_group: &'static str,
    pub start_menu: &'static str,
    pub records: &'static str,
    pub open: &'static str,
    pub forget: &'static str,
    pub refresh: &'static str,
    pub close: &'static str,
    pub working: &'static str,
    pub confirm_apply: &'static str,
    pub confirm_forget: &'static str,
}

/// The texts of `lang`.
pub fn texts(lang: Lang) -> Texts {
    match lang {
        Lang::En => Texts {
            title: "op-reason settings",
            language: "Language",
            lang_choices: ["English", "日本語", "From OPREASON_LANG (else English)"],
            relay: "SSH relay",
            start: "Start",
            stop: "Stop",
            restart: "Restart",
            autostart: "Start at logon, and again within a minute when it stops (op-reason's task)",
            tray_icon: "Show its icon in the notification area",
            mode: "Mode",
            time_limit: "Time limit (min)",
            answer_wait: "Answer wait (s)",
            apply: "Apply",
            apply_note: "The mode and the times apply when the relay starts again.",
            skill: "Skill for AI agents (SKILL.md)",
            columns: ["Place", "State", "Where"],
            place_or_update: "Place or update",
            remove: "Remove",
            check: "Check",
            add_folder: "Add a folder…",
            start_menu_group: "Start menu and records",
            start_menu: "op-reason settings in the Start menu",
            records: "Records",
            open: "Open",
            forget: "Forget",
            refresh: "Refresh",
            close: "Close",
            working: "Working…",
            confirm_apply: "The relay starts again with these values: 1Password asks once more at the next signature, and the answers it remembered are forgotten. Apply?",
            confirm_forget: "Forget the accounts remembered for the links of references? The next call remembers its account again.",
        },
        Lang::Ja => Texts {
            title: "op-reason の設定",
            language: "言語",
            lang_choices: ["English", "日本語", "OPREASON_LANG に従う (無ければ英語)"],
            relay: "SSH の中継",
            start: "起こす",
            stop: "止める",
            restart: "起こし直す",
            autostart: "ログオンで起こし、止まれば 1 分以内に起こし直す (op-reason のタスク)",
            tray_icon: "通知領域にアイコンを出す",
            mode: "モード",
            time_limit: "答えの有効な時間 (分)",
            answer_wait: "窓が待つ時間 (秒)",
            apply: "適用",
            apply_note: "モードと時間は、中継を起こし直したときに効きます。",
            skill: "AI エージェント向けの skill (SKILL.md)",
            columns: ["置き場", "状態", "場所"],
            place_or_update: "置く・更新する",
            remove: "消す",
            check: "確かめる",
            add_folder: "フォルダを足す…",
            start_menu_group: "スタートメニューと記録",
            start_menu: "スタートメニューに op-reason の設定を出す",
            records: "記録",
            open: "開く",
            forget: "忘れる",
            refresh: "読み直す",
            close: "閉じる",
            working: "処理しています…",
            confirm_apply: "この値で中継を起こし直します。次の署名で 1Password がもう一度尋ね、覚えていた答えは忘れます。適用しますか?",
            confirm_forget: "参照のリンクのために覚えたアカウントを忘れますか? 次の呼び出しがまたアカウントを覚えます。",
        },
    }
}

/// `op-reason --settings` could not start the window's process.
pub fn not_started(lang: Lang, why: &str) -> String {
    match lang {
        Lang::En => format!("the settings window could not be started: {why}"),
        Lang::Ja => format!("設定の窓を起こせませんでした: {why}"),
    }
}

/// The line of the accounts remembered for links.
pub fn accounts(lang: Lang, n: usize) -> String {
    match lang {
        Lang::En => format!("Accounts remembered for links: {n}"),
        Lang::Ja => format!("リンクのために覚えたアカウント: {n} 件"),
    }
}

fn find<'a>(items: &'a [Item], name: &Name) -> Option<&'a Item> {
    items.iter().find(|i| i.name == *name)
}

fn started_by(lang: Lang, ours: bool, by: Option<&str>) -> String {
    match (lang, ours, by) {
        (Lang::En, true, _) => "started by op-reason's task".into(),
        (Lang::Ja, true, _) => "op-reason のタスクが起こしたもの".into(),
        (Lang::En, false, Some(t)) => format!("started by another task, {t}"),
        (Lang::Ja, false, Some(t)) => format!("別の task {t} が起こしたもの"),
        (Lang::En, false, None) => "started outside op-reason's task".into(),
        (Lang::Ja, false, None) => "op-reason のタスクの外で起こしたもの".into(),
    }
}

/// The relay's line: whether it runs, its process, its mode and who started it.
pub fn relay_line(lang: Lang, items: &[Item]) -> String {
    let ja = lang == Lang::Ja;
    let mode = find(items, &Name::RelayMode).map_or_else(String::new, |i| match &i.state {
        State::RelayDiffers { relay, .. } => relay.clone(),
        _ => i.value.clone().unwrap_or_default(),
    });
    let autostart =
        find(items, &Name::RelayAutostart).and_then(|i| i.value.as_deref()) == Some("on");
    let Some(running) = find(items, &Name::RelayRunning) else {
        return String::new();
    };
    match &running.state {
        State::Running { pid, ours, by } => {
            let who = started_by(lang, *ours, by.as_deref());
            if ja {
                format!("動いています · pid {pid} · {mode} · {who}")
            } else {
                format!("Running · pid {pid} · {mode} · {who}")
            }
        }
        State::TaskOff => {
            if ja {
                "止まっています: 起こすまで op-reason のタスクは止めてあります".into()
            } else {
                "Stopped: op-reason's task is turned off until Start".into()
            }
        }
        State::TaskStarting => {
            if ja {
                "まだ答えません: op-reason のタスクが 1 分以内に起こします".into()
            } else {
                "Not answering yet: op-reason's task starts the relay within a minute".into()
            }
        }
        State::Unreadable(e) => {
            if ja {
                format!("分かりません: {e}")
            } else {
                format!("Unknown: {e}")
            }
        }
        _ => match (ja, autostart) {
            (false, false) => {
                "Not running · turn on the line below to start it from op-reason's task".into()
            }
            (true, false) => {
                "動いていません · 下の行を入れると op-reason のタスクから起こします".into()
            }
            (false, true) => "Not running".into(),
            (true, true) => "動いていません".into(),
        },
    }
}

/// Which of Start, Stop and Restart may act now: Start from op-reason's task when no relay answers, Stop and Restart
/// only for the relay of op-reason's task.
pub fn relay_buttons(items: &[Item]) -> (bool, bool, bool) {
    let task = find(items, &Name::RelayAutostart).and_then(|i| i.value.as_deref()) == Some("on");
    match find(items, &Name::RelayRunning).map(|i| &i.state) {
        Some(State::Running { ours: true, .. }) => (false, true, true),
        Some(State::Running { .. }) => (false, false, false),
        Some(_) if task => (true, false, false),
        _ => (false, false, false),
    }
}

/// A skill place as the list names it.
pub fn place_label(lang: Lang, name: &Name) -> String {
    match (name, lang) {
        (Name::SkillWindows, _) => "Windows (Claude Code)".into(),
        (Name::SkillWsl(d), _) => format!("WSL: {d}"),
        (Name::SkillFolder(f), Lang::En) => format!("Folder: {f}"),
        (Name::SkillFolder(f), Lang::Ja) => format!("フォルダ: {f}"),
        (other, _) => other.text(),
    }
}

/// A skill place's state, short, for the list.
pub fn skill_state(lang: Lang, item: &Item, version: &str) -> String {
    let ja = lang == Lang::Ja;
    match &item.state {
        State::Skill { state, link } => {
            let mut s = match (state, ja) {
                (SkillState::Missing, false) => "not placed".to_owned(),
                (SkillState::Missing, true) => "置いていません".to_owned(),
                (SkillState::Ok, false) => format!("placed ({version})"),
                (SkillState::Ok, true) => format!("置いてあります ({version})"),
                (SkillState::Old(v), false) => format!("placed by {v}: update it"),
                (SkillState::Old(v), true) => format!("{v} が置いたもの: 更新できます"),
                (SkillState::Changed, false) => "edited by hand".to_owned(),
                (SkillState::Changed, true) => "手で直してあります".to_owned(),
                (SkillState::Foreign, false) => "not op-reason's: left alone".to_owned(),
                (SkillState::Foreign, true) => "op-reason のものではありません".to_owned(),
            };
            if link.is_some() {
                s.push_str(if ja {
                    " · リンクの先には書きません"
                } else {
                    " · a link: nothing is written there"
                });
            }
            s
        }
        State::NotRunning => {
            if ja {
                "見ていません: distro が止まっています".into()
            } else {
                "not checked: the distribution does not run".into()
            }
        }
        State::Unreadable(e) => {
            if ja {
                format!("読めません: {e}")
            } else {
                format!("could not be read: {e}")
            }
        }
        _ => item.value.clone().unwrap_or_default(),
    }
}

/// What an action did, for the window's last line (empty when it did nothing).
pub fn result_line(lang: Lang, changes: &[Change]) -> String {
    changes
        .iter()
        .map(|c| text::change(lang, c).replace('\n', " "))
        .collect::<Vec<_>>()
        .join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_reason_core::lang::Lang;
    use op_reason_core::settings::catalog::Name;
    use op_reason_core::settings::engine::{Change, Done, Item, State, Why};
    use op_reason_core::settings::skill::SkillState;

    fn item(name: Name, value: Option<&str>, state: State) -> Item {
        Item {
            name,
            value: value.map(str::to_owned),
            default: None,
            state,
            where_: String::new(),
        }
    }

    fn relay(running: State, mode: &str) -> Vec<Item> {
        vec![
            item(Name::RelayMode, Some(mode), State::Ok),
            item(Name::RelayAutostart, Some("on"), State::Ok),
            item(
                Name::RelayRunning,
                Some(if matches!(running, State::Running { .. }) {
                    "on"
                } else {
                    "off"
                }),
                running,
            ),
        ]
    }

    #[test]
    fn both_languages_have_every_text() {
        for lang in [Lang::En, Lang::Ja] {
            let t = texts(lang);
            for s in [
                t.title,
                t.language,
                t.relay,
                t.start,
                t.stop,
                t.restart,
                t.autostart,
                t.tray_icon,
                t.mode,
                t.time_limit,
                t.answer_wait,
                t.apply,
                t.apply_note,
                t.skill,
                t.place_or_update,
                t.remove,
                t.check,
                t.add_folder,
                t.start_menu_group,
                t.start_menu,
                t.records,
                t.open,
                t.forget,
                t.refresh,
                t.close,
                t.working,
                t.confirm_apply,
                t.confirm_forget,
            ] {
                assert!(!s.is_empty());
            }
            assert_eq!(t.lang_choices.len(), 3);
            assert_eq!(t.columns.len(), 3);
        }
        assert_ne!(texts(Lang::En).title, texts(Lang::Ja).title);
    }

    #[test]
    fn a_window_that_could_not_start_is_named_with_the_reason() {
        let en = not_started(Lang::En, "access denied");
        assert!(
            en.contains("settings window") && en.ends_with("access denied"),
            "{en}"
        );
        let ja = not_started(Lang::Ja, "access denied");
        assert!(
            ja.contains("設定の窓") && ja.ends_with("access denied"),
            "{ja}"
        );
    }

    #[test]
    fn the_relay_line_says_who_runs_it() {
        let ours = relay(
            State::Running {
                pid: 42,
                ours: true,
                by: None,
            },
            "per-call",
        );
        assert_eq!(
            relay_line(Lang::En, &ours),
            "Running · pid 42 · per-call · started by op-reason's task"
        );
        let other = relay(
            State::Running {
                pid: 7,
                ours: false,
                by: Some(r"\pjp\op-reason-ssh-relay".into()),
            },
            "show-only",
        );
        assert!(relay_line(Lang::En, &other).contains(r"another task, \pjp\op-reason-ssh-relay"));
        assert!(relay_line(Lang::Ja, &other).contains("別の task"));
        assert!(relay_line(Lang::En, &relay(State::TaskOff, "per-call")).starts_with("Stopped"));
        assert!(
            relay_line(Lang::En, &relay(State::TaskStarting, "per-call"))
                .contains("within a minute")
        );
        assert!(relay_line(Lang::En, &relay(State::Ok, "per-call")).starts_with("Not running"));
    }

    #[test]
    fn what_the_relay_buttons_may_do() {
        let ours = relay(
            State::Running {
                pid: 42,
                ours: true,
                by: None,
            },
            "per-call",
        );
        assert_eq!(
            relay_buttons(&ours),
            (false, true, true),
            "stop and restart"
        );
        let other = relay(
            State::Running {
                pid: 7,
                ours: false,
                by: None,
            },
            "per-call",
        );
        assert_eq!(
            relay_buttons(&other),
            (false, false, false),
            "not op-reason's: nothing"
        );
        let stopped = relay(State::TaskOff, "per-call");
        assert_eq!(relay_buttons(&stopped), (true, false, false));
        let mut none = relay(State::Ok, "per-call");
        none[1] = item(Name::RelayAutostart, Some("off"), State::Ok);
        assert_eq!(
            relay_buttons(&none),
            (false, false, false),
            "no task: nothing to start from"
        );
    }

    #[test]
    fn places_and_their_states() {
        assert_eq!(
            place_label(Lang::En, &Name::SkillWindows),
            "Windows (Claude Code)"
        );
        assert_eq!(
            place_label(Lang::En, &Name::SkillWsl("Ubuntu".into())),
            "WSL: Ubuntu"
        );
        assert_eq!(
            place_label(Lang::Ja, &Name::SkillFolder(r"D:\a".into())),
            r"フォルダ: D:\a"
        );
        let placed = item(
            Name::SkillWindows,
            Some("on"),
            State::Skill {
                state: SkillState::Ok,
                link: None,
            },
        );
        assert_eq!(skill_state(Lang::En, &placed, "0.7.0"), "placed (0.7.0)");
        let old = item(
            Name::SkillWindows,
            Some("on"),
            State::Skill {
                state: SkillState::Old("0.6.0".into()),
                link: None,
            },
        );
        assert!(skill_state(Lang::En, &old, "0.7.0").contains("update"));
        let stopped = item(Name::SkillWsl("Debian".into()), None, State::NotRunning);
        assert!(skill_state(Lang::Ja, &stopped, "0.7.0").contains("止まって"));
    }

    #[test]
    fn the_result_line_of_an_action() {
        let done = vec![Change {
            name: "relay.mode".into(),
            from: Some("per-call".into()),
            to: "show-only".into(),
            done: Done::Done,
            note: None,
        }];
        assert_eq!(
            result_line(Lang::En, &done),
            "relay.mode: per-call → show-only"
        );
        let failed = vec![Change {
            name: "relay.running".into(),
            from: Some("on".into()),
            to: "off".into(),
            done: Done::Failed(Why::Locked),
            note: None,
        }];
        assert!(result_line(Lang::En, &failed).contains("window open"));
        assert_eq!(result_line(Lang::En, &[]), "");
    }
}

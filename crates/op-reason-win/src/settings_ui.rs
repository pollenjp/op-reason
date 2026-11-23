//! The settings window's texts and lines (design `2026-11-23-settings-design.md` §11): portable, so that Linux tests
//! cover them. The window itself is `win::settings::window`.

use op_reason_core::lang::Lang;
use op_reason_core::settings::catalog::{self, Name, Want};
use op_reason_core::settings::engine::{Change, Item, State};
use op_reason_core::settings::skill::SkillState;
use op_reason_core::settings::text;
use op_reason_core::settings::values::{self, RelayMode};

/// The IDs of the window's controls (the Windows tests find them by these).
pub mod ids {
    /// The language combo box.
    pub const LANG: u16 = 101;
    /// The relay's line.
    pub const RELAY_LINE: u16 = 110;
    /// Start.
    pub const START: u16 = 111;
    /// Stop.
    pub const STOP: u16 = 112;
    /// Restart.
    pub const RESTART: u16 = 113;
    /// `relay.autostart`.
    pub const AUTOSTART: u16 = 114;
    /// `relay.tray-icon`.
    pub const TRAY_ICON: u16 = 115;
    /// `relay.mode`.
    pub const MODE: u16 = 116;
    /// `relay.time-limit` (minutes).
    pub const TIME_LIMIT: u16 = 117;
    /// Its up-down.
    pub const TIME_LIMIT_SPIN: u16 = 118;
    /// `relay.answer-wait` (seconds).
    pub const ANSWER_WAIT: u16 = 119;
    /// Its up-down.
    pub const ANSWER_WAIT_SPIN: u16 = 120;
    /// The note by Apply.
    pub const APPLY_NOTE: u16 = 121;
    /// Apply.
    pub const APPLY: u16 = 122;
    /// The skill places.
    pub const SKILLS: u16 = 131;
    /// Place or update.
    pub const PLACE: u16 = 132;
    /// Remove.
    pub const REMOVE: u16 = 133;
    /// Check.
    pub const CHECK: u16 = 134;
    /// Add a folder.
    pub const ADD_FOLDER: u16 = 135;
    /// `start-menu`.
    pub const START_MENU: u16 = 141;
    /// The records folder.
    pub const RECORDS: u16 = 142;
    /// Open the records folder.
    pub const OPEN_RECORDS: u16 = 143;
    /// The accounts remembered for links.
    pub const ACCOUNTS: u16 = 144;
    /// Forget them.
    pub const FORGET: u16 = 145;
    /// What the last action did.
    pub const STATUS: u16 = 151;
    /// Refresh.
    pub const REFRESH: u16 = 152;
    /// Close: `IDCANCEL`, so that Esc closes the window too.
    pub const CLOSE: u16 = 2;
}

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

/// The note by Apply: when the relay runs with other values than the file's, it says so.
pub fn apply_note(lang: Lang, items: &[Item]) -> &'static str {
    let differs = items.iter().any(|i| {
        matches!(
            i.name,
            Name::RelayMode | Name::RelayTimeLimit | Name::RelayAnswerWait
        ) && matches!(i.state, State::RelayDiffers { .. })
    });
    match (differs, lang) {
        (false, _) => texts(lang).apply_note,
        (true, Lang::En) => "The relay runs with other values until Apply starts it again.",
        (true, Lang::Ja) => "中継は別の値で動いています。適用で起こし直すと、この値になります。",
    }
}

/// The language combo box's item of the file's `lang` (English, Japanese, from `OPREASON_LANG`).
pub fn lang_index(items: &[Item]) -> usize {
    match find(items, &Name::Lang).and_then(|i| i.value.as_deref()) {
        Some("en") => 0,
        Some("ja") => 1,
        _ => 2,
    }
}

/// What the language combo box's item `index` writes.
pub fn lang_want(index: usize) -> Want {
    match index {
        0 => Want::Lang(Some(Lang::En)),
        1 => Want::Lang(Some(Lang::Ja)),
        _ => Want::Lang(None),
    }
}

/// The mode combo box's item (`per-call`, `show-only`).
pub fn mode_index(items: &[Item]) -> usize {
    match find(items, &Name::RelayMode).and_then(|i| i.value.as_deref()) {
        Some("show-only") => 1,
        _ => 0,
    }
}

/// What the mode combo box's item `index` writes.
pub fn mode_want(index: usize) -> Want {
    match index {
        1 => Want::Mode(RelayMode::ShowOnly),
        _ => Want::Mode(RelayMode::PerCall),
    }
}

/// An on-or-off setting is on (one that is not in the list is off).
pub fn is_on(items: &[Item], name: &Name) -> bool {
    find(items, name).and_then(|i| i.value.as_deref()) == Some("on")
}

/// The time limit in whole minutes, for its field (seconds round up).
pub fn time_limit_text(items: &[Item]) -> String {
    find(items, &Name::RelayTimeLimit)
        .and_then(|i| i.value.as_deref())
        .and_then(values::time_limit)
        .map_or_else(String::new, |s| s.div_ceil(60).to_string())
}

/// The answer wait in seconds, for its field.
pub fn answer_wait_text(items: &[Item]) -> String {
    find(items, &Name::RelayAnswerWait)
        .and_then(|i| i.value.as_deref())
        .and_then(values::answer_wait)
        .map_or_else(String::new, |s| s.to_string())
}

/// What the time limit's field writes (minutes, 1 to 1440), when it holds such a number.
pub fn time_limit_want(text: &str) -> Option<Want> {
    let n = text.trim();
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    catalog::want(&Name::RelayTimeLimit, &format!("{n}m")).ok()
}

/// What the answer wait's field writes (seconds, 1 to 3600), when it holds such a number.
pub fn answer_wait_want(text: &str) -> Option<Want> {
    let n = text.trim();
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    catalog::want(&Name::RelayAnswerWait, &format!("{n}s")).ok()
}

/// A time field that holds no number within its bounds.
pub fn out_of_range(lang: Lang, name: &Name) -> String {
    let max_minutes = values::MAX_TIME_LIMIT / 60;
    let max_seconds = values::MAX_ANSWER_WAIT;
    match (name, lang) {
        (Name::RelayTimeLimit, Lang::En) => {
            format!("Time limit: a whole number of minutes from 1 to {max_minutes}")
        }
        (Name::RelayTimeLimit, Lang::Ja) => {
            format!("答えの有効な時間: 1 から {max_minutes} までの分")
        }
        (_, Lang::En) => format!("Answer wait: a whole number of seconds from 1 to {max_seconds}"),
        (_, Lang::Ja) => format!("窓が待つ時間: 1 から {max_seconds} までの秒"),
    }
}

/// A row of the skill list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRow {
    /// The setting it changes.
    pub name: Name,
    /// The place, as the list names it.
    pub place: String,
    /// Its state, short.
    pub state: String,
    /// The folder.
    pub where_: String,
}

/// The skill places of the list, in its order.
pub fn skill_rows(lang: Lang, items: &[Item], version: &str) -> Vec<SkillRow> {
    items
        .iter()
        .filter(|i| {
            matches!(
                i.name,
                Name::SkillWindows | Name::SkillWsl(_) | Name::SkillFolder(_)
            )
        })
        .map(|i| SkillRow {
            name: i.name.clone(),
            place: place_label(lang, &i.name),
            state: skill_state(lang, i, version),
            where_: i.where_.clone(),
        })
        .collect()
}

/// Which of Place or update, Remove and Check may act on the chosen skill place: nothing is done to a `SKILL.md` that
/// op-reason did not place or behind a link; a place not looked at yet is looked at by the action first.
pub fn skill_buttons(item: Option<&Item>) -> (bool, bool, bool) {
    let Some(item) = item else {
        return (false, false, false);
    };
    match &item.state {
        State::Skill { link: Some(_), .. }
        | State::Skill {
            state: SkillState::Foreign,
            ..
        } => (false, false, true),
        State::Skill {
            state: SkillState::Ok,
            ..
        } => (false, true, true),
        State::Skill {
            state: SkillState::Missing,
            ..
        } => (true, false, true),
        _ => (true, true, true),
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
    use op_reason_core::settings::catalog::Want;
    use op_reason_core::settings::engine::{Change, Done, Item, State, Why};
    use op_reason_core::settings::skill::SkillState;
    use op_reason_core::settings::values::RelayMode;

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

    fn file_values(lang: &str, mode: &str, limit: &str, wait: &str) -> Vec<Item> {
        vec![
            item(Name::Lang, Some(lang), State::Ok),
            item(Name::RelayMode, Some(mode), State::Ok),
            item(Name::RelayTimeLimit, Some(limit), State::Ok),
            item(Name::RelayAnswerWait, Some(wait), State::Ok),
            item(Name::RelayTrayIcon, Some("off"), State::Ok),
            item(Name::StartMenu, Some("on"), State::Ok),
        ]
    }

    #[test]
    fn the_choices_follow_the_file_and_write_what_they_show() {
        let items = file_values("ja", "show-only", "90m", "45s");
        assert_eq!(lang_index(&items), 1);
        assert_eq!(lang_index(&file_values("en", "per-call", "60m", "60s")), 0);
        assert_eq!(
            lang_index(&file_values("default", "per-call", "60m", "60s")),
            2
        );
        assert_eq!(lang_want(0), Want::Lang(Some(Lang::En)));
        assert_eq!(lang_want(1), Want::Lang(Some(Lang::Ja)));
        assert_eq!(lang_want(2), Want::Lang(None));
        assert_eq!(mode_index(&items), 1);
        assert_eq!(mode_want(0), Want::Mode(RelayMode::PerCall));
        assert_eq!(mode_want(1), Want::Mode(RelayMode::ShowOnly));
        assert!(!is_on(&items, &Name::RelayTrayIcon));
        assert!(is_on(&items, &Name::StartMenu));
        assert!(
            !is_on(&items, &Name::RelayAutostart),
            "not in the list: off"
        );
    }

    #[test]
    fn the_times_are_minutes_and_seconds_within_their_bounds() {
        let items = file_values("en", "per-call", "90m", "45s");
        assert_eq!(time_limit_text(&items), "90");
        assert_eq!(answer_wait_text(&items), "45");
        assert_eq!(
            time_limit_text(&file_values("en", "per-call", "90s", "60s")),
            "2",
            "seconds round up to whole minutes"
        );
        assert_eq!(time_limit_want("30"), Some(Want::Secs(1800)));
        assert_eq!(time_limit_want("1440"), Some(Want::Secs(86_400)));
        assert_eq!(time_limit_want("1441"), None, "at most a day");
        assert_eq!(time_limit_want("0"), None);
        assert_eq!(time_limit_want(""), None);
        assert_eq!(answer_wait_want("90"), Some(Want::Secs(90)));
        assert_eq!(answer_wait_want("3601"), None, "at most an hour");
        assert!(out_of_range(Lang::En, &Name::RelayTimeLimit).contains("1440"));
        assert!(out_of_range(Lang::Ja, &Name::RelayAnswerWait).contains("3600"));
    }

    #[test]
    fn the_skill_rows_and_what_their_buttons_may_do() {
        let placed = |state: SkillState, link: Option<&str>| {
            item(
                Name::SkillWindows,
                Some("on"),
                State::Skill {
                    state,
                    link: link.map(str::to_owned),
                },
            )
        };
        let mut items = file_values("en", "per-call", "60m", "60s");
        items.push(Item {
            where_: r"C:\Users\<you>\.claude\skills".into(),
            ..placed(SkillState::Ok, None)
        });
        items.push(item(
            Name::SkillWsl("Debian".into()),
            None,
            State::NotRunning,
        ));
        let rows = skill_rows(Lang::En, &items, "0.7.0");
        assert_eq!(rows.len(), 2, "the skill places only, in the list's order");
        assert_eq!(rows[0].name, Name::SkillWindows);
        assert_eq!(rows[0].place, "Windows (Claude Code)");
        assert_eq!(rows[0].state, "placed (0.7.0)");
        assert_eq!(rows[0].where_, r"C:\Users\<you>\.claude\skills");
        assert_eq!(rows[1].place, "WSL: Debian");

        assert_eq!(skill_buttons(None), (false, false, false), "no row chosen");
        assert_eq!(
            skill_buttons(Some(&placed(SkillState::Ok, None))),
            (false, true, true)
        );
        assert_eq!(
            skill_buttons(Some(&placed(SkillState::Missing, None))),
            (true, false, true)
        );
        assert_eq!(
            skill_buttons(Some(&placed(SkillState::Old("0.6.0".into()), None))),
            (true, true, true)
        );
        assert_eq!(
            skill_buttons(Some(&placed(SkillState::Changed, None))),
            (true, true, true)
        );
        assert_eq!(
            skill_buttons(Some(&placed(SkillState::Foreign, None))),
            (false, false, true),
            "never touched"
        );
        assert_eq!(
            skill_buttons(Some(&placed(SkillState::Ok, Some(r"C:\x")))),
            (false, false, true),
            "a link on the way: refused"
        );
        assert_eq!(
            skill_buttons(Some(&item(
                Name::SkillWsl("Debian".into()),
                None,
                State::NotRunning
            ))),
            (true, true, true),
            "not looked at: the engine looks first"
        );
    }

    #[test]
    fn the_note_by_apply_says_when_the_relay_runs_with_other_values() {
        let items = file_values("en", "show-only", "60m", "60s");
        assert_eq!(apply_note(Lang::En, &items), texts(Lang::En).apply_note);
        let mut differs = items.clone();
        differs[1].state = State::RelayDiffers {
            relay: "per-call".into(),
            pid: 42,
        };
        let note = apply_note(Lang::En, &differs);
        assert_ne!(note, texts(Lang::En).apply_note);
        assert!(note.contains("other values"), "{note}");
        assert!(apply_note(Lang::Ja, &differs).contains("適用"));
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

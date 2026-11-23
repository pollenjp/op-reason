//! The texts of `op-reason --settings` in English and Japanese (design `2026-11-23-settings-design.md` §7.2).

use crate::lang::Lang;
use crate::settings::catalog::Usage;
use crate::settings::engine::{Change, Checked, Done, Item, Note, ShortcutState, State, Why};
use crate::settings::skill::SkillState;

/// The usage of `--settings`.
pub fn usage(lang: Lang) -> String {
    match lang {
        Lang::En => r"Usage: op-reason --settings <word> …

  op-reason --settings                              the settings window, on the Windows desktop
  op-reason --settings help                         this text
  op-reason --settings list [--json]                every setting: its value now, its default,
                                                    where it is kept
  op-reason --settings get <name> [--json]          the value of one setting now
  op-reason --settings set <name> <value>           make it so; also <name>=<value> …
  op-reason --settings check <name>=<value> … [--json]
                                                    compare: exit 0 the same, 1 different, 2 failed
  op-reason --settings relay start|stop|restart     the SSH relay of op-reason's scheduled task
  op-reason --settings forget-accounts              forget the accounts remembered for links
  op-reason --settings open-records                 open the folder of the records

Settings
  lang                      en | ja | default
  relay.mode                per-call | show-only | default
  relay.time-limit          <n>[s|m|h] | default (60m)
  relay.answer-wait         <n>[s|m] | default (60s)
  relay.autostart           on | off   op-reason's task: the relay at logon, again
                                       within a minute when it stops
  relay.running             on | off   stop keeps the relay stopped until a start
  relay.tray-icon           on | off | default (on)   the relay's icon in the notification area
  skill.windows             on | off   %USERPROFILE%\.claude\skills\op-reason\SKILL.md
  skill.wsl.<distribution>  on | off   ~/.claude/skills of a WSL distribution
  skill.folder.<folder>     on | off   <folder>\op-reason\SKILL.md
  start-menu                on | off   op-reason settings in the Start menu

Every change is recorded in the log (op-reason --log).
"
        .to_owned(),
        Lang::Ja => r"使い方: op-reason --settings <語> …

  op-reason --settings                              設定の窓 (Windows のデスクトップに開く)
  op-reason --settings help                         この文
  op-reason --settings list [--json]                すべての設定: 今の値・既定・置き場
  op-reason --settings get <名前> [--json]          1 つの設定の今の値
  op-reason --settings set <名前> <値>              そうする。<名前>=<値> … も
  op-reason --settings check <名前>=<値> … [--json]
                                                    比べる: 終了コード 0 同じ・1 違う・2 失敗
  op-reason --settings relay start|stop|restart     op-reason の task の SSH の中継
  op-reason --settings forget-accounts              リンクのために覚えたアカウントを忘れる
  op-reason --settings open-records                 記録のフォルダを開く

設定
  lang                      en | ja | default
  relay.mode                per-call | show-only | default
  relay.time-limit          <n>[s|m|h] | default (60m)
  relay.answer-wait         <n>[s|m] | default (60s)
  relay.autostart           on | off   op-reason の task: logon で中継を起こし、止まれば
                                       1 分以内に起こし直す
  relay.running             on | off   止めた中継は、起こすまで止まったまま
  relay.tray-icon           on | off | default (on)   中継の通知領域のアイコン
  skill.windows             on | off   %USERPROFILE%\.claude\skills\op-reason\SKILL.md
  skill.wsl.<distribution>  on | off   WSL の distro の ~/.claude/skills
  skill.folder.<folder>     on | off   <folder>\op-reason\SKILL.md
  start-menu                on | off   スタートメニューの op-reason の設定

変えたことはすべてログに残ります (op-reason --log)。
"
        .to_owned(),
    }
}

/// A mistake on the command line.
pub fn usage_error(lang: Lang, u: &Usage) -> String {
    match (lang, u) {
        (Lang::En, Usage::UnknownWord(w)) => format!("--settings does not know {w}"),
        (Lang::Ja, Usage::UnknownWord(w)) => format!("--settings は {w} を知りません"),
        (Lang::En, Usage::UnknownName(n)) => {
            format!("there is no setting {n} (op-reason --settings list)")
        }
        (Lang::Ja, Usage::UnknownName(n)) => {
            format!("{n} という設定はありません (op-reason --settings list)")
        }
        (Lang::En, Usage::BadValue { name, value }) => format!("{name} does not take {value}"),
        (Lang::Ja, Usage::BadValue { name, value }) => {
            format!("{name} は {value} を受け付けません")
        }
        (Lang::En, Usage::BadDistro(d)) => format!("{d:?} is not the name of a WSL distribution"),
        (Lang::Ja, Usage::BadDistro(d)) => format!("{d:?} は WSL の distro の名前ではありません"),
        (Lang::En, Usage::RelativeFolder(f)) => format!("{f} is not an absolute folder"),
        (Lang::Ja, Usage::RelativeFolder(f)) => format!("{f} は絶対パスのフォルダではありません"),
        (Lang::En, Usage::WslShare(f)) => format!(
            "{f} is in a WSL distribution's share: op-reason cannot tell links there; use skill.wsl.<distribution>"
        ),
        (Lang::Ja, Usage::WslShare(f)) => format!(
            "{f} は WSL の distro の共有の中です。そこではリンクを見分けられないので、skill.wsl.<distro> を使ってください"
        ),
        (Lang::En, Usage::NoValue(w)) => {
            format!("{w} needs more: a name and a value (name=value for check)")
        }
        (Lang::Ja, Usage::NoValue(w)) => {
            format!("{w} の後ろが足りません: 名前と値 (check は 名前=値)")
        }
        (Lang::En, Usage::Twice(n)) => format!("{n} is named twice"),
        (Lang::Ja, Usage::Twice(n)) => format!("{n} を 2 回書いています"),
        (Lang::En, Usage::Extra(a)) => format!("{a} is one argument too many"),
        (Lang::Ja, Usage::Extra(a)) => format!("{a} は余分な引数です"),
    }
}

/// The header of `list`.
pub fn header(lang: Lang) -> [&'static str; 4] {
    match lang {
        Lang::En => ["Setting", "Now", "Default", "Kept in, and how it is"],
        Lang::Ja => ["設定", "今", "既定", "置き場と今のようす"],
    }
}

/// What a setting's value now comes with, one line (empty for nothing to say).
pub fn state(lang: Lang, item: &Item, version: &str) -> String {
    let ja = lang == Lang::Ja;
    match &item.state {
        State::Ok => String::new(),
        State::Ignored(raw) => {
            if ja {
                format!("settings.json の {raw} は受け付けない値で、既定が効いています")
            } else {
                format!(
                    "settings.json has {raw}, which op-reason does not take: the default counts"
                )
            }
        }
        State::RelayDiffers { relay, pid } => {
            if ja {
                format!(
                    "動いている中継 (pid {pid}) は {relay} で動いています: 起こし直すと効きます (relay restart)"
                )
            } else {
                format!(
                    "the running relay (pid {pid}) uses {relay}: it applies when the relay starts again (relay restart)"
                )
            }
        }
        State::TaskDiffers(fields) => {
            let fields = fields.join(", ");
            if ja {
                format!(
                    "op-reason の task が違います ({fields}): set relay.autostart on で作り直します"
                )
            } else {
                format!(
                    "op-reason's task differs ({fields}): set relay.autostart on makes it again"
                )
            }
        }
        State::TaskOff => {
            if ja {
                "op-reason の task を止めてあります: relay start で起こします".into()
            } else {
                "op-reason's task is turned off (stopped): relay start starts it".into()
            }
        }
        State::TaskStarting => {
            if ja {
                "op-reason の task が 1 分以内に中継を起こします".into()
            } else {
                "op-reason's task starts the relay within a minute".into()
            }
        }
        State::OtherRelay { pid, by } => {
            let who = started_by(lang, false, by.as_deref());
            if ja {
                format!(
                    "op-reason の task の外の中継が答えています (pid {pid}、{who}): op-reason の task は毎分 op-reason.exe を起こし、それはすぐ終わります"
                )
            } else {
                format!(
                    "a relay that op-reason's task did not start answers (pid {pid}, {who}): op-reason's task starts op-reason.exe every minute, which exits at once"
                )
            }
        }
        State::Running { pid, ours, by } => {
            format!("pid {pid}, {}", started_by(lang, *ours, by.as_deref()))
        }
        State::Skill { state, link } => {
            let mut s = match state {
                SkillState::Missing => {
                    if ja {
                        "置いていません".to_owned()
                    } else {
                        "not placed".to_owned()
                    }
                }
                SkillState::Ok => {
                    if ja {
                        format!("置いてあります ({version})")
                    } else {
                        format!("placed ({version})")
                    }
                }
                SkillState::Old(v) => {
                    if ja {
                        format!("op-reason {v} が置いたもの: on にすると {version} に更新します")
                    } else {
                        format!("placed by op-reason {v}: setting it on updates it to {version}")
                    }
                }
                SkillState::Changed => {
                    if ja {
                        "手で直してあります: on にすると置き直します".to_owned()
                    } else {
                        "edited by hand: setting it on places it again".to_owned()
                    }
                }
                SkillState::Foreign => {
                    if ja {
                        "op-reason が置いたものではない SKILL.md: 触りません".to_owned()
                    } else {
                        "a SKILL.md that op-reason did not place: left alone".to_owned()
                    }
                }
            };
            if let Some(link) = link {
                s.push_str(" · ");
                s.push_str(&link_text(lang, link));
            }
            s
        }
        State::OtherTasks(tasks) => {
            let tasks = tasks.join(", ");
            if ja {
                format!(
                    "ほかにも中継を起こす task があります: {tasks} (logon で中継を起こす task が 2 つあるのは移る前の残りのことが多いので、使わない方を消してください)"
                )
            } else {
                format!(
                    "other tasks start a relay too: {tasks} (two tasks that each start a relay at logon are likely a leftover: remove the one you do not use)"
                )
            }
        }
        State::Shortcut(state) => match (state, ja) {
            (ShortcutState::Missing, false) => "not there".into(),
            (ShortcutState::Missing, true) => "ありません".into(),
            (ShortcutState::Ok, false) => "starts this op-reason.exe --settings".into(),
            (ShortcutState::Ok, true) => "この op-reason.exe --settings を起こします".into(),
            (ShortcutState::Differs(t), false) => {
                format!("starts {t}: setting it on makes it again for this op-reason.exe")
            }
            (ShortcutState::Differs(t), true) => {
                format!("{t} を起こします: on にすると、この op-reason.exe で作り直します")
            }
            (ShortcutState::Foreign(t), false) => {
                format!(
                    "a shortcut of that name that does not start op-reason's settings ({t}): left alone"
                )
            }
            (ShortcutState::Foreign(t), true) => {
                format!("同じ名前の、op-reason の設定を起こさないショートカット ({t}): 触りません")
            }
        },
        State::NotRunning => {
            if ja {
                "見ていません: distro が止まっています (get・check・set で名指しすると起こします)"
                    .into()
            } else {
                "not checked: the distribution does not run (naming it in get, check or set starts it)".into()
            }
        }
        State::Unreadable(e) => {
            if ja {
                format!("読めませんでした: {e}")
            } else {
                format!("could not be read: {e}")
            }
        }
    }
}

fn started_by(lang: Lang, ours: bool, by: Option<&str>) -> String {
    match (lang, ours, by) {
        (Lang::En, true, _) => "started by op-reason's task".into(),
        (Lang::Ja, true, _) => "op-reason の task が起こしたもの".into(),
        (Lang::En, false, Some(t)) => format!("started by another task, {t}"),
        (Lang::Ja, false, Some(t)) => format!("別の task {t} が起こしたもの"),
        (Lang::En, false, None) => "started outside op-reason's task".into(),
        (Lang::Ja, false, None) => "op-reason の task の外で起こしたもの".into(),
    }
}

fn link_text(lang: Lang, link: &str) -> String {
    match lang {
        Lang::En => {
            format!("{link} is a junction or a symbolic link: op-reason does not write there")
        }
        Lang::Ja => format!("{link} は junction か symbolic link なので、op-reason は書きません"),
    }
}

/// Why a change was not done.
pub fn why(lang: Lang, w: &Why) -> String {
    let ja = lang == Lang::Ja;
    match w {
        Why::Locked => {
            if ja {
                "中継が窓を開いているか、op の呼び出しが 1Password を待っています: 答えてから打ち直してください".into()
            } else {
                "the relay has a window open, or an op call waits for 1Password: answer it and try again".into()
            }
        }
        Why::NotOurs(by) => {
            let who = started_by(lang, false, by.as_deref());
            if ja {
                format!("中継は {who} です: op-reason はそれを止めません")
            } else {
                format!("the relay was {who}: op-reason never stops it")
            }
        }
        Why::NoTask => {
            if ja {
                "op-reason は自分の task からだけ中継を起こします: 先に set relay.autostart on を"
                    .into()
            } else {
                "op-reason starts the relay from its task only: set relay.autostart on first".into()
            }
        }
        Why::NoAnswer => {
            if ja {
                "中継が 15 秒たっても答えませんでした (op-reason --log にわけが出ることがあります)"
                    .into()
            } else {
                "the relay did not answer within 15 seconds (op-reason --log may say why)".into()
            }
        }
        Why::Foreign(path) => {
            if ja {
                format!("{path} は op-reason が置いたものではないので触りません")
            } else {
                format!("{path} was not placed by op-reason: left alone")
            }
        }
        Why::Link(link) => link_text(lang, link),
        Why::Failed(e) => e.clone(),
    }
}

fn note(lang: Lang, n: &Note) -> String {
    match (lang, n) {
        (Lang::En, Note::TaskMadeOff(by)) => format!(
            "op-reason's task is turned off while another relay runs ({}): run op-reason --settings relay start after it has stopped",
            started_by(lang, false, by.as_deref())
        ),
        (Lang::Ja, Note::TaskMadeOff(by)) => format!(
            "別の中継が動いている間は op-reason の task を止めておきます ({}): それが止まってから op-reason --settings relay start を",
            started_by(lang, false, by.as_deref())
        ),
        (Lang::En, Note::Restarted) => {
            "the relay was started again: 1Password asks once more at the next signature".into()
        }
        (Lang::Ja, Note::Restarted) => {
            "中継を起こし直しました: 次の署名で 1Password がもう一度尋ねます".into()
        }
    }
}

/// One line of `set` and the actions.
pub fn change(lang: Lang, c: &Change) -> String {
    let ja = lang == Lang::Ja;
    let mut line = match &c.done {
        Done::Same => {
            if ja {
                format!("{}: {} (そのまま)", c.name, c.to)
            } else {
                format!("{}: {} (already)", c.name, c.to)
            }
        }
        Done::Done => match &c.from {
            Some(from) if *from != c.to => format!("{}: {from} → {}", c.name, c.to),
            _ => format!("{}: {}", c.name, c.to),
        },
        Done::Failed(w) => {
            if ja {
                format!("{}: 変えていません ({}): {}", c.name, c.to, why(lang, w))
            } else {
                format!("{}: not changed ({}): {}", c.name, c.to, why(lang, w))
            }
        }
    };
    if let Some(n) = &c.note {
        line.push_str("\n  ");
        line.push_str(&note(lang, n));
    }
    line
}

/// One line of `check`.
pub fn checked(lang: Lang, c: &Checked, version: &str) -> String {
    let ja = lang == Lang::Ja;
    let value = c.item.value.as_deref().unwrap_or("-");
    let more = state(lang, &c.item, version);
    let more = if more.is_empty() {
        String::new()
    } else {
        format!(": {more}")
    };
    let name = c.item.name.text();
    match (c.same, ja) {
        (true, false) => format!("{name}: same ({value}){more}"),
        (true, true) => format!("{name}: 同じ ({value}){more}"),
        (false, false) => format!("{name}: different: now {value}, wanted {}{more}", c.wanted),
        (false, true) => format!("{name}: 違う: 今は {value}、望みは {}{more}", c.wanted),
    }
}

/// The two lines of the records at the end of `list`.
pub fn records(lang: Lang, folder: &str, accounts: usize) -> [String; 2] {
    match lang {
        Lang::En => [
            format!("records.folder    {folder}    (open-records)"),
            format!("records.accounts  {accounts} remembered for links    (forget-accounts)"),
        ],
        Lang::Ja => [
            format!("records.folder    {folder}    (open-records)"),
            format!(
                "records.accounts  リンクのために覚えたアカウント {accounts} 件    (forget-accounts)"
            ),
        ],
    }
}

/// A value that could not be read, for `get` on stderr.
pub fn unreadable(lang: Lang, name: &str, detail: &str) -> String {
    match lang {
        Lang::En => format!("{name}: could not be read: {detail}"),
        Lang::Ja => format!("{name}: 読めませんでした: {detail}"),
    }
}

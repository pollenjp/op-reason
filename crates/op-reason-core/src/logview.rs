//! The log viewer (`op-reason-log`): merges each call's `start` and `end`, and prints a table or JSON lines.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

use serde_json::Value;

use crate::args::clip;
use crate::lang::Lang;
use crate::time::local_parts;

fn usage(lang: Lang) -> &'static str {
    match lang {
        Lang::En => {
            "Usage: op-reason-log [--since <n>(s|m|h|d)] [--all] [--json]
  Lists the calls of op-reason (the 1Password CLI wrapper) by start time.
  --since 30m / 2h / 1d   the period (default 1h)
  --all                   every record left
  --json                  one JSON object per call (reason and command not cut)
"
        }
        Lang::Ja => {
            "使い方: op-reason-log [--since <数>(s|m|h|d)] [--all] [--json]
  op-reason (1Password CLI の wrapper) の記録を、始まった時刻の順に表で出します。
  --since 30m / 2h / 1d   出す期間 (既定 1h)
  --all                   残っている記録を全部出す
  --json                  1 呼び出し 1 行の JSON で出す (理由とコマンドを切らない)
"
        }
    }
}

struct Labels {
    header: [&'static str; 6],
    human: &'static str,
    running: &'static str,
    refused: &'static str,
    no_window: &'static str,
    lock_skipped: &'static str,
    cleared: &'static str,
    not_cleared: &'static str,
    found_before: &'static str,
    found_after: &'static str,
    no_log: &'static str,
    none_in: &'static str,
    unknown_arg: &'static str,
    bad_since: &'static str,
}

fn labels(lang: Lang) -> Labels {
    match lang {
        Lang::En => Labels {
            header: ["Time", "Result", "Secs", "Caller", "Reason", "Command"],
            human: "Human",
            running: "running or interrupted",
            refused: "refused",
            no_window: " (no window)",
            lock_skipped: " (lock skipped)",
            cleared: "cleared a shared approval",
            not_cleared: "could not clear a shared approval",
            found_before: "found before the call",
            found_after: "found after the call",
            no_log: "No records",
            none_in: "No records in the last",
            unknown_arg: "unknown argument",
            bad_since: "write --since as <n>(s|m|h|d)",
        },
        Lang::Ja => Labels {
            header: ["時刻", "結果", "秒", "呼び手", "理由", "コマンド"],
            human: "人",
            running: "実行中か中断",
            refused: "断った",
            no_window: " (窓なし)",
            lock_skipped: " (lock を待たず)",
            cleared: "共有の承認を消した",
            not_cleared: "共有の承認を消せなかった",
            found_before: "op を打つ前に見つけた",
            found_after: "op を打った後に見つけた",
            no_log: "記録がありません",
            none_in: "直近",
            unknown_arg: "知らない引数です",
            bad_since: "--since は <数>(s|m|h|d) で書いてください",
        },
    }
}

fn result_label(lang: Lang, result: &str) -> String {
    let label = match (lang, result) {
        (Lang::En, "approved") => "approved",
        (Lang::En, "ok") => "ok",
        (Lang::En, "denied") => "denied",
        (Lang::En, "timeout") => "timed out",
        (Lang::En, "failed") => "failed",
        (Lang::En, "launcher-failed") => "failed (launcher)",
        (Lang::En, "interrupted") => "interrupted",
        (Lang::En, "lock-timeout") => "lock wait timed out",
        (Lang::Ja, "approved") => "許可",
        (Lang::Ja, "ok") => "成功",
        (Lang::Ja, "denied") => "拒否",
        (Lang::Ja, "timeout") => "時間切れ",
        (Lang::Ja, "failed") => "失敗",
        (Lang::Ja, "launcher-failed") => "失敗 (launcher)",
        (Lang::Ja, "interrupted") => "中断",
        (Lang::Ja, "lock-timeout") => "承認待ちが終わらない",
        _ => return result.to_string(),
    };
    label.to_string()
}

fn why_label(lang: Lang, why: &str) -> String {
    let label = match (lang, why) {
        (Lang::En, "no-reason") => "no reason",
        (Lang::En, "lock-timeout") => "lock wait timed out",
        (Lang::En, "no-op-exe") => "no op.exe",
        (Lang::En, "no-powershell") => "no powershell.exe",
        (Lang::En, "no-wslpath") => "no wslpath",
        (Lang::En, "no-launcher") => "no launcher",
        (Lang::En, "too-long") => "arguments too long",
        (Lang::En, "interrupted") => "interrupted before the start",
        (Lang::Ja, "no-reason") => "理由なし",
        (Lang::Ja, "lock-timeout") => "承認待ちが終わらない",
        (Lang::Ja, "no-op-exe") => "op.exe が無い",
        (Lang::Ja, "no-powershell") => "powershell.exe が無い",
        (Lang::Ja, "no-wslpath") => "wslpath が無い",
        (Lang::Ja, "no-launcher") => "launcher が無い",
        (Lang::Ja, "too-long") => "引数が長すぎる",
        (Lang::Ja, "interrupted") => "始める前に止められた",
        _ => return why.to_string(),
    };
    label.to_string()
}

/// Display width in a terminal: 2 for East Asian wide and fullwidth characters (and common emoji), 0 for control
/// characters, 1 otherwise.
pub fn display_width(text: &str) -> usize {
    text.chars()
        .map(|c| {
            let u = c as u32;
            if c.is_control() {
                0
            } else if matches!(u,
                0x1100..=0x115F
                | 0x2E80..=0x303E
                | 0x3041..=0x33FF
                | 0x3400..=0x4DBF
                | 0x4E00..=0x9FFF
                | 0xA000..=0xA4CF
                | 0xAC00..=0xD7A3
                | 0xF900..=0xFAFF
                | 0xFE30..=0xFE4F
                | 0xFF00..=0xFF60
                | 0xFFE0..=0xFFE6
                | 0x1F300..=0x1F64F
                | 0x1F900..=0x1F9FF
                | 0x20000..=0x3FFFD)
            {
                2
            } else {
                1
            }
        })
        .sum()
}

fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn cell(v: &str) -> String {
    if v.is_empty() {
        "-".to_string()
    } else {
        v.to_string()
    }
}

/// Control characters (newline, tab, escape, ...) as visible escapes, so that a logged value cannot move the cursor,
/// rewrite the screen or break the table.
pub fn visible(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() {
            let u = c as u32;
            if u < 0x100 {
                out.push_str(&format!("\\x{u:02x}"));
            } else {
                out.push_str(&format!("\\u{{{u:x}}}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn who(lang: Lang, rec: &Value) -> String {
    let caller = &rec["caller"];
    if caller.is_null() {
        return "-".to_string();
    }
    let mut s = if caller["type"] == "claude" {
        "Claude".to_string()
    } else {
        labels(lang).human.to_string()
    };
    let herdr = &caller["herdr"];
    if !herdr.is_null() {
        let label = text(&herdr["label"]);
        s.push(' ');
        s.push_str(if label.is_empty() { "?" } else { &label });
        s.push(' ');
        s.push_str(&text(&herdr["pane"]));
    }
    cell(&clip(&s, 40, "…"))
}

fn command(rec: &Value) -> String {
    let args: Vec<String> = rec["args"]
        .as_array()
        .map(|a| a.iter().map(text).collect())
        .unwrap_or_default();
    let mut s = "op".to_string();
    for a in args {
        s.push(' ');
        s.push_str(&a);
    }
    clip(&s, 60, "…")
}

fn row(lang: Lang, rec: &Value, today: &str) -> [String; 6] {
    let l = labels(lang);
    let epoch = rec["epoch"].as_i64().unwrap_or(0);
    let (date, hms, md) = local_parts(epoch);
    let when = if date == today {
        hms
    } else {
        format!("{md} {hms}")
    };
    let reason = cell(&clip(&text(&rec["reason"]), 48, "…"));
    match rec["event"].as_str().unwrap_or("") {
        "start" => {
            let end = &rec["end"];
            let mut result = if end.is_null() {
                l.running.to_string()
            } else {
                let mut r = result_label(lang, &text(&end["result"]));
                r.push_str(match rec["kind"].as_str().unwrap_or("") {
                    "service-account" => " (SA)",
                    "connect" => " (Connect)",
                    "help" => " (help)",
                    _ => "",
                });
                if text(&end["window"]).starts_with("failed") {
                    r.push_str(l.no_window);
                }
                r
            };
            if rec["lock"] == "skipped" {
                result.push_str(l.lock_skipped);
            }
            [
                when,
                result,
                cell(&text(&end["seconds"])),
                who(lang, rec),
                reason,
                command(rec),
            ]
        }
        "refused" => [
            when,
            format!("{} ({})", l.refused, why_label(lang, &text(&rec["why"]))),
            "-".into(),
            who(lang, rec),
            reason,
            command(rec),
        ],
        _ => [
            when,
            if rec["cleared"] == true {
                l.cleared
            } else {
                l.not_cleared
            }
            .to_string(),
            "-".into(),
            "-".into(),
            if rec["when"] == "start" {
                l.found_before
            } else {
                l.found_after
            }
            .to_string(),
            "op.exe signout".into(),
        ],
    }
}

/// Runs the log viewer. Returns the exit code (0, or 2 on a usage error). Output errors (a closed pipe) are ignored.
pub fn run(
    argv: &[OsString],
    state_dir: &Path,
    now: i64,
    lang: Lang,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let l = labels(lang);
    let mut since_arg = "1h".to_string();
    let mut all = false;
    let mut json = false;
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].to_string_lossy();
        if a == "--since" {
            let Some(v) = argv.get(i + 1) else {
                let _ = err.write_all(usage(lang).as_bytes());
                return 2;
            };
            since_arg = v.to_string_lossy().into_owned();
            i += 2;
            continue;
        }
        if let Some(v) = a.strip_prefix("--since=") {
            since_arg = v.to_string();
        } else if a == "--all" {
            all = true;
        } else if a == "--json" {
            json = true;
        } else if a == "-h" || a == "--help" {
            let _ = out.write_all(usage(lang).as_bytes());
            return 0;
        } else {
            let _ = writeln!(err, "op-reason-log: {}: {a}", l.unknown_arg);
            let _ = err.write_all(usage(lang).as_bytes());
            return 2;
        }
        i += 1;
    }
    let since = {
        let cut = since_arg
            .char_indices()
            .last()
            .map(|(at, _)| at)
            .unwrap_or(0);
        let (num, unit) = since_arg.split_at(cut);
        let factor = match unit {
            "s" => Some(1),
            "m" => Some(60),
            "h" => Some(3600),
            "d" => Some(86400),
            _ => None,
        };
        match (num.parse::<i64>(), factor) {
            (Ok(n), Some(f)) if !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) => {
                n.saturating_mul(f)
            }
            _ => {
                let _ = writeln!(err, "op-reason-log: {}: {since_arg}", l.bad_since);
                let _ = err.write_all(usage(lang).as_bytes());
                return 2;
            }
        }
    };

    let main_log = state_dir.join("log.jsonl");
    let files: Vec<_> = [state_dir.join("log.1.jsonl"), main_log.clone()]
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    if files.is_empty() {
        let _ = writeln!(out, "{} ({})", l.no_log, main_log.display());
        return 0;
    }
    let cutoff = if all { 0 } else { now - since };

    let mut records: Vec<Value> = Vec::new();
    for f in &files {
        let Ok(bytes) = std::fs::read(f) else {
            continue;
        };
        for line in String::from_utf8_lossy(&bytes).lines() {
            if let Ok(v) = serde_json::from_str::<Value>(line)
                && v.is_object()
            {
                records.push(v);
            }
        }
    }
    let mut ends: HashMap<String, Value> = HashMap::new();
    for r in &records {
        if r["event"] == "end"
            && let Some(id) = r["id"].as_str()
        {
            ends.insert(id.to_string(), r.clone());
        }
    }
    let mut rows: Vec<Value> = records
        .into_iter()
        .filter(|r| {
            matches!(
                r["event"].as_str(),
                Some("start" | "refused" | "shared-approval")
            ) && r["epoch"].as_i64().unwrap_or(0) >= cutoff
        })
        .map(|mut r| {
            if r["event"] == "start" {
                let end = r["id"]
                    .as_str()
                    .and_then(|id| ends.get(id).cloned())
                    .unwrap_or(Value::Null);
                r["end"] = end;
            }
            r
        })
        .collect();
    rows.sort_by_key(|r| r["epoch"].as_i64().unwrap_or(0));

    if rows.is_empty() {
        if all {
            let _ = writeln!(out, "{} ({})", l.no_log, main_log.display());
        } else if lang == Lang::Ja {
            let _ = writeln!(
                out,
                "{} {since_arg} の記録はありません ({})",
                l.none_in,
                main_log.display()
            );
        } else {
            let _ = writeln!(out, "{} {since_arg} ({})", l.none_in, main_log.display());
        }
        return 0;
    }
    if json {
        for r in &rows {
            let _ = writeln!(out, "{r}");
        }
        return 0;
    }

    let (today, _, _) = local_parts(now);
    let mut table: Vec<[String; 6]> = vec![l.header.map(str::to_string)];
    table.extend(
        rows.iter()
            .map(|r| row(lang, r, &today).map(|c| visible(&c))),
    );
    let mut widths = [0usize; 6];
    for r in &table {
        for (w, c) in widths.iter_mut().zip(r.iter()) {
            *w = (*w).max(display_width(c));
        }
    }
    for r in &table {
        let mut line = String::new();
        for (k, c) in r.iter().enumerate() {
            line.push_str(c);
            if k + 1 < r.len() {
                line.push_str(&" ".repeat(widths[k] - display_width(c) + 2));
            }
        }
        if writeln!(out, "{line}").is_err() {
            break;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn since_with_multibyte_text_is_a_usage_error() {
        let dir = std::env::temp_dir();
        for bad in ["--since=5分", "--since=1あ", "--since=分"] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let code = run(
                &[OsString::from(bad)],
                &dir,
                0,
                Lang::En,
                &mut out,
                &mut err,
            );
            assert_eq!(code, 2, "{bad}");
        }
    }

    #[test]
    fn control_characters_are_made_visible() {
        assert_eq!(visible("a\u{1b}[2Jb\tc\nd"), "a\\x1b[2Jb\\x09c\\x0ad");
        assert_eq!(visible("日本\u{85}"), "日本\\x85");
    }

    #[test]
    fn widths() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("時刻"), 4);
        assert_eq!(display_width("「x」"), 5);
        assert_eq!(display_width("·…"), 2);
    }
}

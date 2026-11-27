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
  Lists the calls of op-reason (the 1Password CLI wrapper) by start time, and on
  Windows the signatures of op-reason's SSH relay (ssh-log.jsonl).
  --since 30m / 2h / 1d   the period (default 1h)
  --all                   every record left
  --json                  one JSON object per call (reason and command not cut, every reference read)
"
        }
        Lang::Ja => {
            "使い方: op-reason-log [--since <数>(s|m|h|d)] [--all] [--json]
  op-reason (1Password CLI の wrapper) の記録を、始まった時刻の順に表で出します。
  Windows では、op-reason の SSH の中継の署名の記録 (ssh-log.jsonl) も並べます。
  --since 30m / 2h / 1d   出す期間 (既定 1h)
  --all                   残っている記録を全部出す
  --json                  1 呼び出し 1 行の JSON で出す (理由とコマンドを切らず、読む参照を全部)
"
        }
    }
}

struct Labels {
    header: [&'static str; 6],
    human: &'static str,
    program: &'static str,
    terminal: &'static str,
    ssh: &'static str,
    forwarded: &'static str,
    running: &'static str,
    refused: &'static str,
    no_window: &'static str,
    lock_skipped: &'static str,
    fallback: &'static str,
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
            program: "Program",
            terminal: " (terminal)",
            ssh: " (this ssh)",
            forwarded: " (forwarded)",
            running: "running or interrupted",
            refused: "refused",
            no_window: " (no window)",
            lock_skipped: " (lock skipped)",
            fallback: " (fallback)",
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
            program: "プログラム",
            terminal: " (ターミナル)",
            ssh: " (この ssh)",
            forwarded: " (転送)",
            running: "実行中か中断",
            refused: "断った",
            no_window: " (窓なし)",
            lock_skipped: " (lock を待たず)",
            fallback: " (回避経路)",
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

/// The result of a signature through the SSH relay (`ssh-sign`).
fn ssh_result_label(lang: Lang, result: &str) -> String {
    let label = match (lang, result) {
        (Lang::En, "allowed") => "allowed",
        (Lang::En, "passed") => "passed",
        (Lang::En, "denied") => "denied",
        (Lang::En, "timeout") => "timed out",
        (Lang::En, "dismissed") => "window closed",
        (Lang::En, "gone") => "caller left",
        (Lang::En, "failed") => "failed in 1Password",
        (Lang::En, "no-answer") => "no answer from 1Password",
        (Lang::En, "no-upstream") => "no 1Password agent",
        (Lang::En, "lock-timeout") => "lock wait timed out",
        (Lang::En, "signed") => "signed",
        (Lang::En, "malformed") => "malformed request",
        (Lang::En, "no-window") => "no window",
        (Lang::Ja, "allowed") => "許可",
        (Lang::Ja, "passed") => "通過",
        (Lang::Ja, "denied") => "拒否",
        (Lang::Ja, "timeout") => "時間切れ",
        (Lang::Ja, "dismissed") => "窓を閉じた",
        (Lang::Ja, "gone") => "呼び手が去った",
        (Lang::Ja, "failed") => "1Password で失敗",
        (Lang::Ja, "no-answer") => "1Password が答えない",
        (Lang::Ja, "no-upstream") => "1Password の agent が無い",
        (Lang::Ja, "lock-timeout") => "承認待ちが終わらない",
        (Lang::Ja, "signed") => "署名",
        (Lang::Ja, "malformed") => "壊れた依頼",
        (Lang::Ja, "no-window") => "窓を出せなかった",
        _ => return format!("SSH {result}"),
    };
    format!("SSH {label}")
}

/// What the SSH relay itself recorded (`ssh-relay`), with its `detail` when it has one.
fn relay_label(lang: Lang, what: &str, mode: &str, detail: &str) -> String {
    match (lang, what) {
        (Lang::En, "start") => format!("SSH relay started ({mode})"),
        (Lang::En, "exit") => "SSH relay stopped".to_string(),
        (Lang::En, "no-prompt") => format!("SSH: 1Password did not ask ({mode})"),
        (Lang::En, "no-parent") => "SSH relay without a parent process".to_string(),
        (Lang::En, "parent-ended") => "SSH relay's parent process ended".to_string(),
        (Lang::En, "window-stuck") => format!("SSH: op-reason's window stuck ({detail})"),
        (Lang::En, "window-unstuck") => format!("SSH: op-reason's window went on ({detail})"),
        (Lang::En, "stop") if detail.is_empty() => {
            "SSH relay stopped by op-reason --settings".to_string()
        }
        (Lang::En, "stop") => format!("SSH relay stopped by op-reason --settings ({detail})"),
        (Lang::Ja, "start") => format!("SSH の中継を開始 ({mode})"),
        (Lang::Ja, "exit") => "SSH の中継を終了".to_string(),
        (Lang::Ja, "no-prompt") => format!("SSH: 1Password が確認しなかった ({mode})"),
        (Lang::Ja, "no-parent") => "SSH の中継に親の process が無い".to_string(),
        (Lang::Ja, "parent-ended") => "SSH の中継の親の process が終わった".to_string(),
        (Lang::Ja, "window-stuck") => format!("SSH: op-reason の窓が止まっている ({detail})"),
        (Lang::Ja, "window-unstuck") => format!("SSH: op-reason の窓が動き出した ({detail})"),
        (Lang::Ja, "stop") if detail.is_empty() => {
            "SSH の中継を op-reason --settings が止めた".to_string()
        }
        (Lang::Ja, "stop") => format!("SSH の中継を op-reason --settings が止めた ({detail})"),
        _ => format!("SSH relay {what}"),
    }
}

fn why_label(lang: Lang, why: &str) -> String {
    let label = match (lang, why) {
        (Lang::En, "no-reason") => "no reason",
        (Lang::En, "lock-timeout") => "lock wait timed out",
        (Lang::En, "no-op-exe") => "no op.exe",
        (Lang::En, "no-exe") => "no op-reason.exe",
        (Lang::En, "no-powershell") => "no powershell.exe",
        (Lang::En, "no-wslpath") => "no wslpath",
        (Lang::En, "no-launcher") => "no launcher",
        (Lang::En, "too-long") => "arguments too long",
        (Lang::En, "interrupted") => "interrupted before the start",
        (Lang::Ja, "no-reason") => "理由なし",
        (Lang::Ja, "lock-timeout") => "承認待ちが終わらない",
        (Lang::Ja, "no-op-exe") => "op.exe が無い",
        (Lang::Ja, "no-exe") => "op-reason.exe が無い",
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
    let mut s = match caller["type"].as_str() {
        Some("claude") => "Claude".to_string(),
        Some("program") => labels(lang).program.to_string(),
        _ => labels(lang).human.to_string(),
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

fn command(lang: Lang, rec: &Value) -> String {
    let args: Vec<String> = rec["args"]
        .as_array()
        .map(|a| a.iter().map(text).collect())
        .unwrap_or_default();
    let mut s = "op".to_string();
    for a in args {
        s.push(' ');
        s.push_str(&a);
    }
    match refs_count(rec) {
        None => clip(&s, 60, "…"),
        // The count is never cut off with the command.
        Some(n) => {
            let count = match (lang, n) {
                (Lang::En, 1) => " (1 ref)".to_string(),
                (Lang::En, _) => format!(" ({n} refs)"),
                (Lang::Ja, _) => format!(" (参照 {n} 件)"),
            };
            let room = 60usize.saturating_sub(count.chars().count());
            format!("{}{count}", clip(&s, room, "…"))
        }
    }
}

/// The number of references a call reads, when a source other than the arguments gave some (those in the arguments
/// are in the command already).
fn refs_count(rec: &Value) -> Option<u64> {
    let refs = &rec["refs"];
    let elsewhere = refs["from"]
        .as_array()?
        .iter()
        .any(|s| s["kind"].as_str().is_some_and(|k| k != "args"));
    elsewhere.then(|| refs["total"].as_u64()).flatten()
}

/// The row of a signature through the SSH relay. A request that an earlier answer covered takes the caller and the
/// reason of the record that has that answer (`decisions`, by id).
fn ssh_row(
    lang: Lang,
    rec: &Value,
    when: String,
    decisions: &HashMap<String, Value>,
) -> [String; 6] {
    let l = labels(lang);
    let facts = rec["decision"]
        .as_str()
        .filter(|_| rec["caller"].is_null())
        .and_then(|id| decisions.get(id))
        .unwrap_or(rec);
    let mut result = ssh_result_label(lang, &text(&rec["result"]));
    // the click, or the earlier answer that covered it (design 2026-11-27-relay-process-tree-design.md §4.5)
    match rec["answer"].as_str().or_else(|| rec["by"].as_str()) {
        Some("allow-terminal") => result.push_str(l.terminal),
        Some("allow-ssh") => result.push_str(l.ssh),
        _ => {}
    }
    if rec["forwarded"] == true {
        result.push_str(l.forwarded);
    }
    [
        when,
        result,
        cell(&text(&rec["seconds"])),
        who(lang, facts),
        cell(&clip(&text(&facts["reason"]), 48, "…")),
        cell(&clip(&text(&rec["client"]["cmd"]), 60, "…")),
    ]
}

fn row(lang: Lang, rec: &Value, today: &str, decisions: &HashMap<String, Value>) -> [String; 6] {
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
        "ssh-sign" => ssh_row(lang, rec, when, decisions),
        "ssh-relay" => [
            when,
            relay_label(
                lang,
                &text(&rec["what"]),
                &text(&rec["mode"]),
                &text(&rec["detail"]),
            ),
            "-".into(),
            "-".into(),
            "-".into(),
            cell(&text(&rec["pipe"])),
        ],
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
                if end["fallback"].is_object() {
                    r.push_str(l.fallback);
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
                command(lang, rec),
            ]
        }
        "settings" => {
            let ja = lang == Lang::Ja;
            let changes = rec["changes"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .map(|c| {
                            let mut s = text(&c["name"]);
                            if !c["from"].is_null() {
                                s.push(' ');
                                s.push_str(&text(&c["from"]));
                            }
                            s.push_str(" → ");
                            s.push_str(&text(&c["to"]));
                            if c["result"] == "failed" {
                                s.push_str(if ja { " (失敗)" } else { " (failed)" });
                            }
                            s
                        })
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            [
                when,
                if ja { "設定" } else { "settings" }.to_string(),
                "-".into(),
                who(lang, rec),
                "-".into(),
                cell(&clip(&changes, 200, "…")),
            ]
        }
        "refused" => [
            when,
            format!("{} ({})", l.refused, why_label(lang, &text(&rec["why"]))),
            "-".into(),
            who(lang, rec),
            reason,
            command(lang, rec),
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
    let mut files: Vec<_> = [state_dir.join("log.1.jsonl"), main_log.clone()]
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    files.extend(crate::ssh::record::files(state_dir));
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
    let mut decisions: HashMap<String, Value> = HashMap::new();
    for r in &records {
        let Some(id) = r["id"].as_str() else { continue };
        if r["event"] == "end" {
            ends.insert(id.to_string(), r.clone());
        } else if r["event"] == "ssh-sign" && !r["caller"].is_null() {
            decisions.insert(id.to_string(), r.clone());
        }
    }
    let mut rows: Vec<Value> = records
        .into_iter()
        .filter(|r| {
            matches!(
                r["event"].as_str(),
                Some(
                    "start" | "refused" | "shared-approval" | "ssh-sign" | "ssh-relay" | "settings"
                )
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
            .map(|r| row(lang, r, &today, &decisions).map(|c| visible(&c))),
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
    fn a_call_that_fell_back_is_marked() {
        let rec: Value = serde_json::from_str(
            r#"{"event":"start","epoch":0,"kind":"desktop","reason":"r","args":["x"],"caller":{"type":"human"},
                "end":{"result":"approved","seconds":1,"window":"shown","runner":"launcher","fallback":{"why":"no-status","exit":1}}}"#,
        )
        .unwrap();
        assert_eq!(
            row(Lang::En, &rec, "", &HashMap::new())[1],
            "approved (fallback)"
        );
        assert_eq!(
            row(Lang::Ja, &rec, "", &HashMap::new())[1],
            "許可 (回避経路)"
        );
        let mut plain = rec.clone();
        plain["end"]["fallback"] = Value::Null;
        assert_eq!(row(Lang::En, &plain, "", &HashMap::new())[1], "approved");
        let refused: Value = serde_json::from_str(
            r#"{"event":"refused","epoch":0,"why":"no-exe","caller":{"type":"human"}}"#,
        )
        .unwrap();
        assert_eq!(
            row(Lang::En, &refused, "", &HashMap::new())[1],
            "refused (no op-reason.exe)"
        );
    }

    #[test]
    fn the_command_counts_references_read_from_elsewhere() {
        let rec = |from: &str, total: u64| -> Value {
            serde_json::from_str(&format!(
                r#"{{"event":"start","epoch":0,"kind":"desktop","reason":"r","args":["inject","-i","{}"],
                    "caller":{{"type":"human"}},"refs":{{"total":{total},"from":{from},"list":[]}}}}"#,
                "t".repeat(70)
            ))
            .unwrap()
        };
        let file = r#"[{"kind":"args"},{"kind":"file","name":"t"}]"#;
        let en = row(Lang::En, &rec(file, 5), "", &HashMap::new())[5].clone();
        assert!(en.ends_with("… (5 refs)"), "{en}");
        assert_eq!(en.chars().count(), 61, "as long as a cut command: 60 and …");
        assert!(row(Lang::En, &rec(file, 1), "", &HashMap::new())[5].ends_with("(1 ref)"));
        assert!(row(Lang::Ja, &rec(file, 3), "", &HashMap::new())[5].ends_with("… (参照 3 件)"));
        let only_args = row(
            Lang::En,
            &rec(r#"[{"kind":"args"}]"#, 2),
            "",
            &HashMap::new(),
        )[5]
        .clone();
        assert!(!only_args.contains("refs"), "{only_args}");
        let mut none = rec(file, 2);
        none.as_object_mut().unwrap().remove("refs");
        assert!(!row(Lang::Ja, &none, "", &HashMap::new())[5].contains("参照"));
    }

    #[test]
    fn an_ssh_row_names_the_answer_that_let_it_through() {
        let sign = |extra: &str| -> Value {
            serde_json::from_str(&format!(
                r#"{{"v":1,"id":"s9","event":"ssh-sign","time":"t","epoch":1,"mode":"per-call",{extra},"seconds":0.1,"client":{{"pid":1,"exe":"C:\\ssh.exe","cmd":"ssh.exe -T git@github.com"}},"key":{{"fp":"SHA256:x"}},"purpose":{{"kind":"userauth","user":"git"}},"forwarded":false}}"#
            ))
            .unwrap()
        };
        let result =
            |lang: Lang, rec: &Value| ssh_row(lang, rec, String::new(), &HashMap::new())[1].clone();
        let clicked = sign(r#""result":"allowed","answer":"allow-ssh""#);
        assert_eq!(result(Lang::En, &clicked), "SSH allowed (this ssh)");
        assert_eq!(result(Lang::Ja, &clicked), "SSH 許可 (この ssh)");
        let passed = sign(r#""result":"passed","decision":"s8","by":"allow-ssh""#);
        assert_eq!(result(Lang::En, &passed), "SSH passed (this ssh)");
        let terminal = sign(r#""result":"passed","decision":"s8","by":"allow-terminal""#);
        assert_eq!(result(Lang::En, &terminal), "SSH passed (terminal)");
        let call = sign(r#""result":"passed","decision":"s8","by":"allow""#);
        assert_eq!(
            result(Lang::En, &call),
            "SSH passed",
            "the call's answer says nothing more"
        );
    }

    /// A state directory with the records of both logs: the SSH relay's in two files, an `op` call in the middle.
    fn mixed_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "op-reason-logview-ssh-{}",
            crate::log::random_u64()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let allowed = r#"{"v":1,"id":"s1","event":"ssh-sign","time":"t","epoch":1000,"mode":"per-call","result":"allowed","seconds":2.5,"answer":"allow","call":{"rule":"claude"},"caller":{"type":"claude","claude_pid":3,"herdr":{"workspace":"w","label":"lab","pane":"p"},"cwd":"~","repo":null,"branch":null},"reason":"push the fix","client":{"pid":11,"exe":"C:\\ssh.exe","cmd":"ssh.exe git@github.com \"git-receive-pack 'me/r.git'\""},"key":{"fp":"SHA256:x"},"purpose":{"kind":"userauth","user":"git"},"forwarded":false}"#;
        std::fs::write(dir.join("ssh-log.1.jsonl"), format!("{allowed}\n")).unwrap();
        let newer = [
            r#"{"v":1,"id":"r1","event":"ssh-relay","time":"t","epoch":900,"what":"start","mode":"per-call","pipe":"\\\\.\\pipe\\op-reason-ssh-agent","upstream":"u","limit":3600,"pid":5}"#,
            r#"{"v":1,"id":"s2","event":"ssh-sign","time":"t","epoch":3000,"mode":"per-call","result":"passed","seconds":0.1,"decision":"s1","client":{"pid":12,"exe":"C:\\ssh.exe","cmd":"ssh.exe git@github.com"},"key":{"fp":"SHA256:x"},"purpose":{"kind":"userauth","user":"git"},"forwarded":false}"#,
            r#"{"v":1,"id":"s3","event":"ssh-sign","time":"t","epoch":4000,"mode":"per-call","result":"denied","seconds":4.0,"answer":"deny","call":{"rule":"alone"},"caller":{"type":"program","claude_pid":null,"herdr":null,"cwd":"","repo":null,"branch":null},"reason":"","client":{"pid":13,"exe":"C:\\ssh.exe","cmd":"ssh.exe -A jumphost"},"key":{"fp":"SHA256:x"},"purpose":{"kind":"userauth","user":"me"},"forwarded":true}"#,
        ];
        std::fs::write(dir.join("ssh-log.jsonl"), newer.join("\n") + "\n").unwrap();
        let op = [
            r#"{"v":1,"id":"o1","event":"start","time":"t","epoch":2000,"session":null,"mode":"desktop","kind":"desktop","lock":"held","caller":{"type":"human","claude_pid":null,"herdr":null,"cwd":"/w","repo":null,"branch":null},"reason":"list vaults","args":["vault","list"]}"#,
            r#"{"v":1,"id":"o1","event":"end","time":"t","epoch":2001,"session":null,"result":"approved","exit":0,"seconds":1,"window":"shown","toast":"shown","others":0,"signout":"ok","watch":{"start":null,"end":null}}"#,
        ];
        std::fs::write(dir.join("log.jsonl"), op.join("\n") + "\n").unwrap();
        dir
    }

    fn view(dir: &Path, lang: Lang, args: &[&str]) -> Vec<String> {
        let argv: Vec<OsString> = args.iter().map(OsString::from).collect();
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(run(&argv, dir, 5000, lang, &mut out, &mut err), 0);
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn the_ssh_records_are_merged_by_time_one_line_each() {
        let dir = mixed_dir();
        let lines = view(&dir, Lang::En, &["--all"]);
        assert_eq!(lines.len(), 6, "the header and five rows: {lines:#?}");
        assert!(
            lines[1].contains("SSH relay started (per-call)"),
            "{}",
            lines[1]
        );
        assert!(lines[2].contains("SSH allowed"), "{}", lines[2]);
        assert!(
            lines[2].contains("Claude lab p") && lines[2].contains("push the fix"),
            "{}",
            lines[2]
        );
        assert!(lines[2].contains("ssh.exe git@github.com"), "{}", lines[2]);
        assert!(
            lines[3].contains("approved") && lines[3].contains("list vaults"),
            "{}",
            lines[3]
        );
        assert!(lines[4].contains("SSH passed"), "{}", lines[4]);
        assert!(
            lines[4].contains("Claude lab p") && lines[4].contains("push the fix"),
            "a passed request shows the caller and reason of its decision: {}",
            lines[4]
        );
        assert!(
            lines[5].contains("SSH denied (forwarded)") && lines[5].contains("Program"),
            "{}",
            lines[5]
        );
        let ja = view(&dir, Lang::Ja, &["--all"]);
        assert!(ja[1].contains("SSH の中継を開始 (per-call)"), "{}", ja[1]);
        assert!(ja[2].contains("SSH 許可"), "{}", ja[2]);
        assert!(ja[4].contains("SSH 通過"), "{}", ja[4]);
        assert!(
            ja[5].contains("SSH 拒否 (転送)") && ja[5].contains("プログラム"),
            "{}",
            ja[5]
        );
        assert_eq!(ssh_result_label(Lang::En, "no-window"), "SSH no window");
        assert_eq!(
            ssh_result_label(Lang::Ja, "no-window"),
            "SSH 窓を出せなかった"
        );
        let json = view(&dir, Lang::En, &["--all", "--json"]);
        assert_eq!(json.len(), 5);
        assert!(json[1].contains(r#""id":"s1""#) && json[3].contains(r#""decision":"s1""#));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_settings_record_is_one_line_with_its_changes() {
        let dir = std::env::temp_dir().join(format!(
            "op-reason-logview-settings-{}",
            crate::log::random_u64()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = r#"{"v":1,"id":"c1","event":"settings","time":"t","epoch":4500,"session":null,"from":"window","caller":{"type":"claude","claude_pid":3,"herdr":null,"cwd":"~","repo":null,"branch":null},"changes":[{"name":"relay.mode","from":"per-call","to":"show-only","result":"done"},{"name":"relay.running","from":"on","to":"off","result":"failed","why":"locked"}]}"#;
        std::fs::write(dir.join("log.jsonl"), format!("{rec}\n")).unwrap();
        let lines = view(&dir, Lang::En, &["--all"]);
        let line = lines.iter().find(|l| l.contains("relay.mode")).unwrap();
        assert!(line.contains("settings"), "{line}");
        assert!(line.contains("Claude"), "{line}");
        assert!(
            line.contains("relay.mode per-call → show-only; relay.running on → off (failed)"),
            "{line}"
        );
        let lines = view(&dir, Lang::Ja, &["--all"]);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("設定") && l.contains("(失敗)")),
            "{lines:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_window_that_does_not_appear_names_its_step() {
        assert_eq!(
            relay_label(Lang::En, "window-stuck", "per-call", "draw"),
            "SSH: op-reason's window stuck (draw)"
        );
        assert_eq!(
            relay_label(Lang::En, "window-unstuck", "per-call", "after 7.0 s"),
            "SSH: op-reason's window went on (after 7.0 s)"
        );
        assert_eq!(
            relay_label(Lang::Ja, "window-stuck", "per-call", "show"),
            "SSH: op-reason の窓が止まっている (show)"
        );
        assert_eq!(
            relay_label(Lang::En, "start", "per-call", ""),
            "SSH relay started (per-call)",
            "the others have no detail"
        );
        assert_eq!(
            relay_label(Lang::En, "stop", "per-call", ""),
            "SSH relay stopped by op-reason --settings"
        );
        assert_eq!(
            relay_label(Lang::Ja, "stop", "per-call", "1 requests still in progress"),
            "SSH の中継を op-reason --settings が止めた (1 requests still in progress)"
        );
    }

    #[test]
    fn widths() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("時刻"), 4);
        assert_eq!(display_width("「x」"), 5);
        assert_eq!(display_width("·…"), 2);
    }
}

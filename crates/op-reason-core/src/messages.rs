//! Texts printed by the wrapper, in English and Japanese. The window and toast texts live in the launcher (and in
//! `op-reason.exe`'s copy of its table).

use crate::lang::Lang;

/// The prefix of every stderr line.
pub const PREFIX: &str = "op-reason: ";

/// The wrapper's usage, printed on stdout for `op` without arguments and `op-reason --help`.
pub fn usage_full(lang: Lang, version: &str) -> String {
    match lang {
        Lang::En => format!(
            r#"Usage: op --reason "<why you call op>" <op arguments...>

op is op-reason, a wrapper for the 1Password CLI that requires a reason. Every
call starts a fresh op-reason.exe on Windows that starts op.exe, so a 1Password
approval covers that one call only. The reason appears in a window next to the
approval prompt and in the log (op-reason-log).

Reason (--reason)
  One sentence: what for, and what you read or write. People need it too;
  without it the call is refused (exit 2). --reason "<reason>" and
  --reason=<reason> both work, anywhere before -- (after --, the arguments
  belong to op).
  Example: op --reason "read the Proxmox API token to create the staging VM" read "op://Private/proxmox/token"

Without a reason (no approval prompt)
  op               this usage (op.exe is not started)
  op --version     op's version
  op --help        op's own help; op <command> --help for a command

Wrapper options (first argument only)
  op-reason --help                       this usage
  op-reason --version                    op-reason's version
  op-reason --skill                      a guide for AI agents (SKILL.md)
  op-reason --completion fish [<name>]   the fish completion for op (or <name>)
  op-reason-log (or op-reason --log)     who called op and why: the last hour
                                         by default; --since 1d, --all, --json

AI agents: run `op-reason --skill` once to learn how to call op here.

Deny any 1Password CLI approval prompt that comes without the op-reason window.
op-reason {version} is unofficial: it is not made or supported by 1Password.
"#
        ),
        Lang::Ja => format!(
            r#"使い方: op --reason "<なぜ op を呼ぶか>" <op の引数…>

この op は、呼ぶ理由を必須にした 1Password CLI の wrapper (op-reason) です。
呼び出しごとに Windows で新しい op-reason.exe を起こして op.exe を起こさせ、
1Password の承認をその 1 回に閉じます。理由は、承認の画面と一緒に出る窓と、
記録 (op-reason-log) に残ります。

理由 (--reason)
  何のために何を読む・書くかを 1 文で書きます。人が打つときも要り、無ければ 2 で
  断ります。--reason "<理由>" でも --reason=<理由> でもよく、-- より前なら op の
  引数のどこに置いてもかまいません (-- より後ろは op の引数です)。
  例: op --reason "infra の VM を作るのに Proxmox の API の token を読む" read "op://Private/proxmox/token"

理由なしで通るもの (承認の画面は出ません)
  op               この使い方 (op.exe は起こしません)
  op --version     op の版
  op --help        op 本体の使い方。コマンドごとは op <コマンド> --help

wrapper の option (最初の引数のときだけ)
  op-reason --help                       この使い方
  op-reason --version                    op-reason の版
  op-reason --skill                      AI エージェント向けの手引き (SKILL.md)
  op-reason --completion fish [<名前>]   op (か <名前>) の fish の補完
  op-reason-log (か op-reason --log)     誰が何の理由で op を呼んだか。既定は直近
                                         1 時間 (--since 1d・--all・--json)

AI エージェントは、最初に `op-reason --skill` を打って呼び方を読んでください。

op-reason の窓が出ていない 1Password CLI の承認の画面は、拒否してください。
op-reason {version} は非公式の道具です (1Password の製品ではありません)。
"#
        ),
    }
}

/// The short usage printed on stderr with a refusal.
pub fn usage_short(lang: Lang) -> String {
    match lang {
        Lang::En => r#"Usage: op --reason "<why you call op>" <op arguments...>
  op is op-reason, a wrapper for the 1Password CLI: every call needs a reason. The reason appears in the window
  next to the 1Password approval prompt and in the log (op-reason-log). Write what for and what you read or write,
  in one sentence.
  Example: op --reason "read the Proxmox API token to create the staging VM" read "op://Private/proxmox/token"
  Only --version and help (--help, -h, help) work without a reason. Run op without arguments for the full usage.
  AI agents: run `op-reason --skill` for the full guide.
"#
        .to_string(),
        Lang::Ja => r#"使い方: op --reason "<なぜ op を呼ぶか>" <op の引数…>
  この op は、呼ぶ理由を必須にした wrapper (op-reason) です。理由は 1Password の承認の画面と一緒に出る窓と、
  記録 (op-reason-log) に残ります。何のために何を読む・書くかを 1 文で書いてください。
  例: op --reason "infra の VM を作るのに Proxmox の API の token を読む" read "op://Private/proxmox/token"
  理由なしで通るのは --version と help (--help・-h・help) だけです。詳しい使い方は引数なしの op で出ます。
  AI エージェントは `op-reason --skill` で手引きを読んでください。
"#
        .to_string(),
    }
}

/// `--reason` was given but is empty.
pub fn empty_reason(lang: Lang) -> String {
    match lang {
        Lang::En => "The reason (--reason) is empty. Write in one sentence why you call op.".into(),
        Lang::Ja => "理由 (--reason) が空です。なぜ op を呼ぶかを 1 文で書いてください。".into(),
    }
}

/// No `--reason`.
pub fn no_reason(lang: Lang) -> String {
    match lang {
        Lang::En => "There is no reason (--reason). People need it too.".into(),
        Lang::Ja => "理由 (--reason) がありません。人が打つときも要ります。".into(),
    }
}

/// `op.exe` was not found.
pub fn no_op_exe(lang: Lang, path: &str) -> String {
    let at = if path.is_empty() {
        String::new()
    } else {
        format!(" ({path})")
    };
    match lang {
        Lang::En => format!(
            "op.exe was not found{at}. Install the 1Password CLI on Windows and check that the PATH of WSL includes the PATH of Windows."
        ),
        Lang::Ja => format!(
            "op.exe が見つかりません{at}。Windows に 1Password CLI を入れ、WSL の PATH に Windows の PATH が入っているかを確かめてください。"
        ),
    }
}

/// `powershell.exe` was not found.
pub fn no_powershell(lang: Lang, cmd: &str) -> String {
    match lang {
        Lang::En => {
            format!("{cmd} was not found (the PATH of WSL may not include the PATH of Windows).")
        }
        Lang::Ja => format!(
            "{cmd} が見つかりません (WSL の PATH に Windows の PATH が入っていないかもしれません)。"
        ),
    }
}

/// `op-reason.exe` was not found (and, in `auto` mode, no fallback either).
pub fn no_exe(lang: Lang, tried: &str) -> String {
    let at = if tried.is_empty() {
        String::new()
    } else {
        format!(" ({tried})")
    };
    match lang {
        Lang::En => format!(
            "op-reason.exe was not found{at}. Unpack op-reason-windows.zip of the same release on Windows and add its folder to the Windows PATH, or set OPREASON_EXE to the WSL path of op-reason.exe."
        ),
        Lang::Ja => format!(
            "op-reason.exe が見つかりません{at}。同じ版の op-reason-windows.zip を Windows に展開してそのフォルダを Windows の PATH に足すか、OPREASON_EXE に op-reason.exe の WSL のパスを入れてください。"
        ),
    }
}

/// `launcher.ps1` was not found next to `op-reason.exe` (`path` is empty when no folder was known).
pub fn no_launcher(lang: Lang, path: &str) -> String {
    let at = if path.is_empty() {
        String::new()
    } else {
        format!(" ({path})")
    };
    match lang {
        Lang::En => format!(
            "launcher.ps1 was not found{at}. It is looked for next to op-reason.exe (OPREASON_EXE, or op-reason.exe in PATH), where op-reason-windows.zip puts it."
        ),
        Lang::Ja => format!(
            "launcher.ps1 が見つかりません{at}。op-reason.exe と同じフォルダ (OPREASON_EXE か、PATH の op-reason.exe のフォルダ) を探します。op-reason-windows.zip はそこに入れています。"
        ),
    }
}

/// Why `auto` moves from `op-reason.exe` to the launcher.
#[derive(Debug, Clone, Copy)]
pub enum FallbackWhy<'a> {
    /// No `op-reason.exe` where `OPREASON_EXE` points (the folder still has `launcher.ps1`).
    NotFound(&'a str),
    /// Starting `op-reason.exe` failed on the Linux side (the error).
    NotStarted(&'a str),
    /// `op-reason.exe` exited without writing the status file (its exit code).
    NoStatus(i32),
}

/// The one stderr line when a call moves to the launcher.
pub fn runner_fallback(lang: Lang, why: FallbackWhy) -> String {
    match (lang, why) {
        (Lang::En, FallbackWhy::NotFound(path)) => format!(
            "op-reason.exe was not found ({path}), so this call runs through launcher.ps1. Set OPREASON_RUNNER=launcher to make that the rule."
        ),
        (Lang::En, FallbackWhy::NotStarted(err)) => format!(
            "op-reason.exe could not be started ({err}), so this call runs through launcher.ps1 instead. If Smart App Control blocks op-reason.exe, set OPREASON_RUNNER=launcher."
        ),
        (Lang::En, FallbackWhy::NoStatus(code)) => format!(
            "op-reason.exe ended without starting the call (exit {code}), so it runs through launcher.ps1 instead. If Smart App Control blocks op-reason.exe, set OPREASON_RUNNER=launcher."
        ),
        (Lang::Ja, FallbackWhy::NotFound(path)) => format!(
            "op-reason.exe が見つからない ({path}) ので、launcher.ps1 で打ちます。いつもそうするなら OPREASON_RUNNER=launcher を入れてください。"
        ),
        (Lang::Ja, FallbackWhy::NotStarted(err)) => format!(
            "op-reason.exe を起こせなかった ({err}) ので、launcher.ps1 で打ち直します。Smart App Control が op-reason.exe を止めているなら、OPREASON_RUNNER=launcher を入れてください。"
        ),
        (Lang::Ja, FallbackWhy::NoStatus(code)) => format!(
            "op-reason.exe が呼び出しを始めずに終わった (終了コード {code}) ので、launcher.ps1 で打ち直します。Smart App Control が op-reason.exe を止めているなら、OPREASON_RUNNER=launcher を入れてください。"
        ),
    }
}

/// The first of two lines when the fallback is needed but cannot run (the second names the missing part).
pub fn no_fallback(lang: Lang, why: FallbackWhy) -> String {
    match (lang, why) {
        (Lang::En, FallbackWhy::NoStatus(code)) => format!(
            "op-reason.exe ended without starting the call (exit {code}), and its fallback cannot run:"
        ),
        (Lang::En, FallbackWhy::NotStarted(err) | FallbackWhy::NotFound(err)) => {
            format!("op-reason.exe could not be started ({err}), and its fallback cannot run:")
        }
        (Lang::Ja, FallbackWhy::NoStatus(code)) => format!(
            "op-reason.exe が呼び出しを始めずに終わり (終了コード {code})、回避経路も使えません:"
        ),
        (Lang::Ja, FallbackWhy::NotStarted(err) | FallbackWhy::NotFound(err)) => {
            format!("op-reason.exe を起こせず ({err})、回避経路も使えません:")
        }
    }
}

/// `OPREASON_RUNNER=exe` and `op-reason.exe` exited without writing the status file (no fallback).
pub fn exe_no_status(lang: Lang, code: i32) -> String {
    match lang {
        Lang::En => format!(
            "op-reason.exe ended without starting the call (exit {code}); with OPREASON_RUNNER=exe there is no fallback to launcher.ps1."
        ),
        Lang::Ja => format!(
            "op-reason.exe が呼び出しを始めずに終わりました (終了コード {code})。OPREASON_RUNNER=exe なので launcher.ps1 には回しません。"
        ),
    }
}

/// A runner (`op-reason.exe` or `powershell.exe`) was found but could not be started (for example, WSL interop is
/// broken).
pub fn cannot_start(lang: Lang, cmd: &str, error: &str) -> String {
    match lang {
        Lang::En => format!(
            "Could not start {cmd}: {error}. Check that WSL can run Windows programs (interop)."
        ),
        Lang::Ja => format!(
            "{cmd} を起こせません: {error}。WSL から Windows のプログラムを起こせるか (interop) を確かめてください。"
        ),
    }
}

/// `--completion` with a shell other than fish.
pub fn completion_usage(lang: Lang, got: &str) -> String {
    match lang {
        Lang::En => {
            format!("--completion supports only fish: --completion fish [<command>] (got: {got})")
        }
        Lang::Ja => format!(
            "--completion は fish だけです: --completion fish [<コマンド>] (渡されたもの: {got})"
        ),
    }
}

/// `wslpath` was not found.
pub fn no_wslpath(lang: Lang) -> String {
    match lang {
        Lang::En => "wslpath was not found (op-reason works only inside WSL).".into(),
        Lang::Ja => "wslpath が見つかりません (WSL の外では使えません)。".into(),
    }
}

/// The arguments are too long for a Windows environment variable.
pub fn too_long(lang: Lang, b64_len: usize) -> Vec<String> {
    match lang {
        Lang::En => vec![
            format!(
                "The arguments of op are too long ({b64_len} characters of base64; a Windows environment variable holds at most 32767)."
            ),
            "Pass long values with op's --template or a file (op document create <path>, for example).".into(),
        ],
        Lang::Ja => vec![
            format!(
                "op の引数が長すぎます (base64 で {b64_len} 字。Windows の環境変数に載るのは 32767 字まで)。"
            ),
            "長い値は op の --template やファイル (op document create <パス> など) で渡してください。".into(),
        ],
    }
}

/// The payload is too large for a Windows environment variable.
pub fn payload_too_long(lang: Lang, len: usize) -> Vec<String> {
    match lang {
        Lang::En => vec![
            format!("The call is too large to pass to Windows ({len} characters; the limit is 32000)."),
            "Pass long values with op's --template or a file (op document create <path>, for example).".into(),
        ],
        Lang::Ja => vec![
            format!("呼び出しが大きすぎて Windows に渡せません ({len} 字。上限は 32000 字)。"),
            "長い値は op の --template やファイル (op document create <パス> など) で渡してください。".into(),
        ],
    }
}

/// Printed (to a terminal only) when the lock is busy.
pub fn lock_waiting(lang: Lang, secs: u64) -> String {
    match lang {
        Lang::En => {
            format!("Waiting for another op call to finish its approval (up to {secs} seconds).")
        }
        Lang::Ja => format!("ほかの op の承認待ちが終わるのを待っています (最大 {secs} 秒)。"),
    }
}

/// The holder of the lock as one line, from `current.json`'s fields.
pub fn holder_line(lang: Lang, time: &str, caller: &str, reason: &str, command: &str) -> String {
    match lang {
        Lang::En => format!("{time} {caller} reason \"{reason}\" {command}"),
        Lang::Ja => format!("{time} {caller} 理由「{reason}」 {command}"),
    }
}

/// Used when the holder is unknown.
pub fn unknown(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "(unknown)",
        Lang::Ja => "(分からない)",
    }
}

/// The lock wait timed out.
pub fn lock_timeout(lang: Lang, secs: u64, holder: &str) -> Vec<String> {
    match lang {
        Lang::En => vec![
            format!(
                "Another op call is still waiting for its approval after {secs} seconds, so this call was not run."
            ),
            format!("Waiting for: {holder}"),
            "Answer that approval prompt first (deny it if it came without the window), then run this again.".into(),
        ],
        Lang::Ja => vec![
            format!("ほかの op の承認待ちが {secs} 秒たっても終わらないので、打たずに終えます。"),
            format!("待たせている呼び出し: {holder}"),
            "その承認の画面に応えてから (窓の出ていない画面なら拒否してから)、打ち直してください。".into(),
        ],
    }
}

/// The launcher's mutex wait timed out (PR 2: calls from Windows and WSL share the mutex).
pub fn mutex_timeout(lang: Lang, holder: &str) -> Vec<String> {
    match lang {
        Lang::En => vec![
            "Another op-reason call (from Windows or WSL) is still waiting for its approval, so this call was not run."
                .into(),
            format!("Waiting for: {holder}"),
            "Answer that approval prompt first (deny it if it came without the window), then run this again.".into(),
        ],
        Lang::Ja => vec![
            "ほかの op-reason の呼び出し (Windows か WSL) の承認待ちが終わらないので、打たずに終えました。".into(),
            format!("待たせている呼び出し: {holder}"),
            "その承認の画面に応えてから (窓の出ていない画面なら拒否してから)、打ち直してください。".into(),
        ],
    }
}

/// The short caller for `current.json`, the toast and the holder line.
pub fn caller_short(lang: Lang, is_claude: bool, herdr_label: &str) -> String {
    let base = match (is_claude, lang) {
        (true, _) => "Claude Code",
        (false, Lang::En) => "Human",
        (false, Lang::Ja) => "人",
    };
    if herdr_label.is_empty() {
        base.to_string()
    } else {
        format!("{base} · {herdr_label}")
    }
}

/// The herdr notification for a shared approval found by the watchdog: (title, body).
pub fn shared_notice(lang: Lang, when: &str, cleared: bool) -> (String, String) {
    let at_start = when == "start";
    match lang {
        Lang::En => {
            let when_desc = if at_start {
                "before the call"
            } else {
                "after the call"
            };
            if cleared {
                (
                    "Cleared a shared 1Password CLI approval".into(),
                    format!(
                        "An approval usable from every WSL pane was left, so op-reason ran op.exe signout --all ({when_desc}). Someone may have allowed a prompt without the op-reason window, or a tool ran op.exe directly."
                    ),
                )
            } else {
                (
                    "Could not clear a shared 1Password CLI approval".into(),
                    format!(
                        "An approval usable from every WSL pane is left ({when_desc}). Run op.exe signout --all in WSL, or lock 1Password."
                    ),
                )
            }
        }
        Lang::Ja => {
            let when_desc = if at_start {
                "op を打つ前"
            } else {
                "op を打った後"
            };
            if cleared {
                (
                    "共有の 1Password CLI の承認を消しました".into(),
                    format!(
                        "WSL のどの pane からも使える承認が残っていたので、op.exe signout --all で消しました ({when_desc})。窓の出ていない承認の画面を許可したか、道具が op.exe を直に打ったのかもしれません。"
                    ),
                )
            } else {
                (
                    "共有の 1Password CLI の承認を消せませんでした".into(),
                    format!(
                        "WSL のどの pane からも使える承認が残っています ({when_desc})。WSL で op.exe signout --all を打つか、1Password をロックしてください。"
                    ),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usages_mention_the_essentials() {
        for lang in [Lang::En, Lang::Ja] {
            let full = usage_full(lang, "9.9.9");
            assert!(full.contains("op --reason \""), "{full}");
            assert!(full.contains("--version"));
            assert!(full.contains("op-reason-log"));
            assert!(full.contains("op --help"));
            assert!(full.contains("op-reason --skill"));
            assert!(full.contains("9.9.9"));
            let short = usage_short(lang);
            assert!(short.contains("--reason"));
            assert!(short.contains("op-reason --skill"));
        }
    }

    #[test]
    fn usage_no_longer_speaks_of_a_powershell_per_call() {
        for lang in [Lang::En, Lang::Ja] {
            let full = usage_full(lang, "9.9.9");
            assert!(full.contains("op-reason.exe"), "{full}");
            assert!(!full.contains("powershell.exe"), "{full}");
        }
    }

    #[test]
    fn fallback_lines_say_how_to_skip_the_exe() {
        for lang in [Lang::En, Lang::Ja] {
            for why in [
                FallbackWhy::NotStarted("Exec format error"),
                FallbackWhy::NoStatus(1),
                FallbackWhy::NotFound("/mnt/c/x/op-reason.exe"),
            ] {
                let line = runner_fallback(lang, why);
                assert!(line.contains("launcher.ps1"), "{line}");
                assert!(line.contains("OPREASON_RUNNER=launcher"), "{line}");
                assert!(!line.contains('\n'), "{line}");
            }
        }
        assert!(no_fallback(Lang::En, FallbackWhy::NoStatus(3)).contains("exit 3"));
        assert!(no_exe(Lang::En, "").contains("OPREASON_EXE"));
        assert!(no_launcher(Lang::Ja, "/w/launcher.ps1").contains("/w/launcher.ps1"));
    }

    #[test]
    fn caller_short_forms() {
        assert_eq!(caller_short(Lang::Ja, false, ""), "人");
        assert_eq!(caller_short(Lang::En, true, "ws"), "Claude Code · ws");
    }
}

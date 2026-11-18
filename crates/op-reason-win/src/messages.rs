//! Messages that exist only in the exe (the rest comes from `op_reason_core::messages`): the core's texts speak of
//! the WSL path (the command `op`, a `powershell.exe` per call, `op-reason-log`) and of the payload, which the exe
//! does not have.

use op_reason_core::lang::Lang;

/// The usage, printed on stdout for `op-reason` without arguments and `op-reason --help`.
pub fn usage_full(lang: Lang, version: &str) -> String {
    match lang {
        Lang::En => format!(
            r#"Usage: op-reason --reason "<why you call op>" <op arguments...>

op-reason is a wrapper for the 1Password CLI that requires a reason.
op-reason.exe starts op.exe itself for every call, so a 1Password approval
covers that one call only. The reason appears in a window next to the approval
prompt and in the log (op-reason --log).

Reason (--reason)
  One sentence: what for, and what you read or write. People need it too;
  without it the call is refused (exit 2). --reason "<reason>" and
  --reason=<reason> both work, anywhere before -- (after --, the arguments
  belong to op).
  Example: op-reason --reason "read the Proxmox API token to create the staging VM" read "op://Private/proxmox/token"

Without a reason (no approval prompt)
  op-reason --help      this usage
  op-reason --version   op-reason's version
  op-reason help        op's own help (and op-reason <command> --help)

Wrapper options (first argument only)
  op-reason --skill     a guide for AI agents (SKILL.md)
  op-reason --log       who called op and why: the last hour by default;
                        --since 1d, --all, --json

AI agents: run op-reason --skill once to learn how to call op here.

SSH keys (the relay in front of the 1Password SSH agent)
  op-reason --ssh-agent [--mode per-call|show-only] [--pipe <name>]
                        [--upstream <name>] [--time-limit <n>[s|m|h]]
                        [--answer-wait <n>[s|m]]
  op-reason --ssh-agent --status   whether the relay answers on its pipe
  Point ssh at it with SSH_AUTH_SOCK=\\.\pipe\op-reason-ssh-agent and give
  the reason in OPREASON_SSH_REASON. per-call (the default) asks once
  per call in op-reason's window; show-only only shows who and why while
  1Password asks. Keep it running from a scheduled task.

Deny any 1Password CLI approval prompt that comes without the op-reason window.
op-reason {version} is unofficial: it is not made or supported by 1Password.
"#
        ),
        Lang::Ja => format!(
            r#"使い方: op-reason --reason "<なぜ op を呼ぶか>" <op の引数…>

op-reason は、呼ぶ理由を必須にした 1Password CLI の wrapper です。op-reason.exe は
呼び出しごとに自分で op.exe を起こし、1Password の承認をその 1 回に閉じます。理由は、
承認の画面と一緒に出る窓と、記録 (op-reason --log) に残ります。

理由 (--reason)
  何のために何を読む・書くかを 1 文で書きます。人が打つときも要り、無ければ 2 で
  断ります。--reason "<理由>" でも --reason=<理由> でもよく、-- より前ならどこに
  置いてもかまいません (-- より後ろは op の引数です)。
  例: op-reason --reason "staging の VM を作るのに Proxmox の API の token を読む" read "op://Private/proxmox/token"

理由なしで通るもの (承認の画面は出ません)
  op-reason --help      この使い方
  op-reason --version   op-reason の版
  op-reason help        op 本体の使い方 (op-reason <コマンド> --help も)

wrapper の option (最初の引数のときだけ)
  op-reason --skill     AI エージェント向けの手引き (SKILL.md)
  op-reason --log       誰が何の理由で op を呼んだか。既定は直近 1 時間
                        (--since 1d・--all・--json)

AI エージェントは、最初に op-reason --skill を打って呼び方を読んでください。

SSH の鍵 (1Password の SSH agent の前に置く中継)
  op-reason --ssh-agent [--mode per-call|show-only] [--pipe <名前>]
                        [--upstream <名前>] [--time-limit <数>[s|m|h]]
                        [--answer-wait <数>[s|m]]
  op-reason --ssh-agent --status   中継がパイプで答えるか
  ssh は SSH_AUTH_SOCK=\\.\pipe\op-reason-ssh-agent で中継に向け、理由は
  OPREASON_SSH_REASON で渡します。per-call (既定) は呼び出しごとに 1 回、op-reason の
  窓で確認します。show-only は、1Password が確認する間、誰が何のためにを見せるだけ
  です。中継はタスク スケジューラーのタスクから起こしたままにしてください。

op-reason の窓が出ていない 1Password CLI の承認の画面は、拒否してください。
op-reason {version} は非公式の道具です (1Password の製品ではありません)。
"#
        ),
    }
}

/// The short usage printed on stderr with a refusal.
pub fn usage_short(lang: Lang) -> String {
    match lang {
        Lang::En => r#"Usage: op-reason --reason "<why you call op>" <op arguments...>
  op-reason is a wrapper for the 1Password CLI: every call needs a reason. The reason appears in the window next to
  the 1Password approval prompt and in the log (op-reason --log). Write what for and what you read or write, in one
  sentence.
  Example: op-reason --reason "read the Proxmox API token to create the staging VM" read "op://Private/proxmox/token"
  Only --version and help (--help, -h, help) work without a reason. See op-reason --help.
  AI agents: run op-reason --skill for the full guide.
"#
        .to_owned(),
        Lang::Ja => r#"使い方: op-reason --reason "<なぜ op を呼ぶか>" <op の引数…>
  op-reason は、呼ぶ理由を必須にした wrapper です。理由は 1Password の承認の画面と一緒に出る窓と、記録
  (op-reason --log) に残ります。何のために何を読む・書くかを 1 文で書いてください。
  例: op-reason --reason "staging の VM を作るのに Proxmox の API の token を読む" read "op://Private/proxmox/token"
  理由なしで通るのは --version と help (--help・-h・help) だけです。詳しくは op-reason --help。
  AI エージェントは op-reason --skill で手引きを読んでください。
"#
        .to_owned(),
    }
}

/// `op.exe` was not found (Windows wording: no WSL `PATH` involved).
pub fn no_op_exe(lang: Lang, path: &str) -> String {
    let at = if path.is_empty() {
        String::new()
    } else {
        format!(" ({path})")
    };
    match lang {
        Lang::En => format!(
            "op.exe was not found{at}. Install the 1Password CLI (winget install AgileBits.1Password.CLI) or set OPREASON_OP_EXE to the path of op.exe."
        ),
        Lang::Ja => format!(
            "op.exe が見つかりません{at}。1Password CLI を入れる (winget install AgileBits.1Password.CLI) か、OPREASON_OP_EXE に op.exe のパスを入れてください。"
        ),
    }
}

/// The `too-long` refusal: the limit is op.exe's command line.
pub fn too_long(lang: Lang, len: usize) -> Vec<String> {
    match lang {
        Lang::En => vec![
            format!(
                "op's arguments are too long ({len} characters on op.exe's command line; Windows allows 32767)."
            ),
            "Pass long values through op's --template or a file (op document create <path>, ...).".to_owned(),
        ],
        Lang::Ja => vec![
            format!("op の引数が長すぎます (op.exe のコマンドラインで {len} 字。Windows は 32767 字まで)。"),
            "長い値は op の --template やファイル (op document create <パス> など) で渡してください。".to_owned(),
        ],
    }
}

/// `op-reason.exe` called by the name `op-reason-ssh`: that is the WSL command.
pub fn ssh_name(lang: Lang) -> String {
    match lang {
        Lang::En => "op-reason-ssh is the WSL command that runs ssh.exe for the SSH relay. On Windows, run ssh.exe with IdentityAgent or SSH_AUTH_SOCK naming the relay's pipe (op-reason.exe --ssh-agent), and give the reason in OPREASON_SSH_REASON.".to_owned(),
        Lang::Ja => "op-reason-ssh は、SSH の中継のために ssh.exe を起こす WSL のコマンドです。Windows では、IdentityAgent か SSH_AUTH_SOCK に中継のパイプ (op-reason.exe --ssh-agent) を指定して ssh.exe を打ち、理由は OPREASON_SSH_REASON で渡してください。".to_owned(),
    }
}

/// The relay started: where it listens and where it passes requests on.
pub fn relay_started(lang: Lang, mode: &str, pipe: &str, upstream: &str) -> String {
    match lang {
        Lang::En => {
            format!("the SSH relay listens on {pipe} ({mode}) and passes requests on to {upstream}")
        }
        Lang::Ja => {
            format!("SSH の中継が {pipe} で待ち受けています ({mode})。依頼は {upstream} へ渡します")
        }
    }
}

/// Another relay runs in this logon session.
pub fn relay_running(lang: Lang) -> String {
    match lang {
        Lang::En => "another SSH relay runs in this logon session (op-reason.exe --ssh-agent --status names it); stop it first to start one with other options".to_owned(),
        Lang::Ja => "このログオン セッションでは別の SSH の中継が動いています (op-reason.exe --ssh-agent --status で分かります)。別の option で起こすなら、先にそれを止めてください".to_owned(),
    }
}

/// The relay's pipe could not be created (another program has the name).
pub fn relay_pipe_taken(lang: Lang, pipe: &str, err: &str) -> String {
    match lang {
        Lang::En => format!("cannot serve {pipe}: {err} (another program may have the name)"),
        Lang::Ja => format!(
            "{pipe} で待ち受けられません: {err} (ほかのプログラムがその名前を使っているかもしれません)"
        ),
    }
}

/// `--status`: what answers on the pipe.
pub fn relay_status(lang: Lang, pipe: &str, pid: Option<u32>) -> String {
    match (lang, pid) {
        (Lang::En, Some(pid)) => format!("the SSH relay answers on {pipe} (pid {pid})"),
        (Lang::En, None) => format!("nothing answers on {pipe}"),
        (Lang::Ja, Some(pid)) => format!("SSH の中継が {pipe} で答えています (pid {pid})"),
        (Lang::Ja, None) => format!("{pipe} で答えるものはありません"),
    }
}

/// op.exe could not be started (after `start` was written).
pub fn start_failed(lang: Lang, err: &str) -> String {
    match lang {
        Lang::En => format!("could not start op.exe: {err}"),
        Lang::Ja => format!("op.exe を起こせませんでした: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exe's usage speaks of `op-reason` and its own log viewer, not of the WSL path or the PowerShell entry.
    #[test]
    fn usage_speaks_of_the_exe() {
        for lang in [Lang::En, Lang::Ja] {
            let full = usage_full(lang, "9.8.7");
            let short = usage_short(lang);
            for text in [&full, &short] {
                assert!(text.contains(r#"op-reason --reason "<"#), "{text}");
                assert!(text.contains("op-reason --log"), "{text}");
                for wrong in [
                    "powershell.exe",
                    "op-reason.cmd",
                    "op-reason-log",
                    "op --reason",
                ] {
                    assert!(!text.contains(wrong), "{wrong} in {text}");
                }
            }
            assert!(full.contains("op-reason 9.8.7"), "{full}");
        }
    }

    #[test]
    fn the_relay_says_where_it_listens() {
        for lang in [Lang::En, Lang::Ja] {
            let p = r"\\.\pipe\x";
            assert!(relay_started(lang, "per-call", p, "u").contains(p));
            assert!(relay_started(lang, "per-call", p, "u").contains("per-call"));
            assert!(relay_pipe_taken(lang, p, "Access is denied").contains("Access is denied"));
            assert!(relay_status(lang, p, Some(42)).contains("42"));
            assert!(relay_status(lang, p, None).contains(p));
            assert!(relay_running(lang).contains("--status"));
        }
    }

    /// The usage names the SSH relay's face and its options.
    #[test]
    fn usage_names_the_relay() {
        for lang in [Lang::En, Lang::Ja] {
            let full = usage_full(lang, "9.8.7");
            for word in [
                "op-reason --ssh-agent",
                "--mode per-call|show-only",
                "--time-limit",
                "--answer-wait",
                "--status",
                "OPREASON_SSH_REASON",
            ] {
                assert!(full.contains(word), "{word} in {full}");
            }
        }
    }
}

//! Messages that exist only in the exe (the rest comes from `op_reason_core::messages`): the core's texts speak of
//! the WSL path and of the payload, which the exe does not have.

use op_reason_core::lang::Lang;

/// `op.exe` was not found (Windows wording: no WSL `PATH` involved).
pub fn no_op_exe(lang: Lang, path: &str) -> String {
    let at = if path.is_empty() {
        String::new()
    } else {
        format!(" ({path})")
    };
    match lang {
        Lang::En => {
            format!(
                "op.exe was not found{at}. Install the 1Password CLI and check that op.exe is in PATH."
            )
        }
        Lang::Ja => format!(
            "op.exe が見つかりません{at}。1Password CLI を入れ、op.exe が PATH にあるかを確かめてください。"
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

/// op.exe could not be started (after `start` was written).
pub fn start_failed(lang: Lang, err: &str) -> String {
    match lang {
        Lang::En => format!("could not start op.exe: {err}"),
        Lang::Ja => format!("op.exe を起こせませんでした: {err}"),
    }
}

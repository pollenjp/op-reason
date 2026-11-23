//! `op-reason.exe --settings …` (design `2026-11-23-settings-design.md`): the settings on Windows.

use std::ffi::OsString;
use std::io::Write;

use op_reason_core::lang::Lang;
use op_reason_core::settings::cli;

/// `op-reason.exe --settings <args>`. Returns the exit code.
pub fn run(_args: &[OsString], lang: Lang) -> i32 {
    let _ = std::io::stdout()
        .lock()
        .write_all(cli::usage(lang).as_bytes());
    2
}

//! `op-reason --settings …` on WSL (design `2026-11-23-settings-design.md` §7.5).

use std::ffi::OsString;

use op_reason_core::lang::Lang;
use op_reason_core::settings::cli;

use crate::launch::print_stdout;

/// `op-reason --settings <args>`. Returns the exit code.
pub fn run(_args: &[OsString], lang: Lang) -> i32 {
    print_stdout(cli::usage(lang));
    2
}

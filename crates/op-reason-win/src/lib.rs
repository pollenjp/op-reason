//! op-reason for Windows as one native exe (plan C').
//!
//! The planning, messages, records, place and log viewer come from `op-reason-core`, like in the WSL binary. This
//! crate adds what the A' PowerShell entry and launcher do on Windows: the lock and its holder file, the caller from
//! the process tree, the window (Direct2D), the toast, starting `op.exe` and signing out ([`win`], Windows only),
//! plus two platform-neutral helpers that are tested everywhere: the window texts ([`ui`]) and the command-line
//! length ([`cmdline`]).

pub mod cmdline;
pub mod messages;
pub mod ui;

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use win::app::run;

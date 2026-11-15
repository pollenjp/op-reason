//! op-reason for Windows as one native exe (plan C').
//!
//! Two faces: the entry (`op-reason --reason "<why>" <op arguments…>` from Windows) and the runner
//! (`op-reason.exe --run`, started by the WSL binary with the call in `OPREASON_PAYLOAD`).
//!
//! The planning, messages, records, place and log viewer come from `op-reason-core`, like in the WSL binary. This
//! crate adds what the A' PowerShell entry and launcher do on Windows: the lock and its holder file, the caller from
//! the process tree, the window (Direct2D), the toast, starting `op.exe` in a Job Object and signing out ([`win`],
//! Windows only), plus platform-neutral parts that are tested everywhere: the window texts ([`ui`]), the payload and
//! status of the runner ([`payload`]) and the command-line length ([`cmdline`]).

pub mod cmdline;
pub mod messages;
pub mod payload;
pub mod ui;

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use win::app::run;

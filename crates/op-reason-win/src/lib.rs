//! op-reason for Windows as one native exe (plan C').
//!
//! Two faces: the entry (`op-reason --reason "<why>" <op arguments…>` from Windows) and the runner
//! (`op-reason.exe --run`, started by the WSL binary with the call in `OPREASON_PAYLOAD`).
//!
//! The planning, messages, records, place and log viewer come from `op-reason-core`, like in the WSL binary. This
//! crate adds what the A' PowerShell entry and launcher do on Windows: the lock and its holder file, the caller from
//! the process tree, the window (Direct2D), the toast, starting `op.exe` in a Job Object and signing out ([`win`],
//! Windows only), plus platform-neutral parts that are tested everywhere: the window texts ([`ui`]), the payload and
//! status of the runner ([`payload`]), the command-line length ([`cmdline`]) and the account copy that links references
//! to the 1Password app ([`account`]).
//!
//! A third face, `op-reason.exe --ssh-agent`, is the SSH relay (design `2026-11-17-ssh-relay-design.md`): its options
//! ([`relay_opts`]), texts ([`relay_ui`]), the cards of its toasts ([`pass_toast`]; design
//! `2026-11-18-pass-toasts-design.md`) and when its window comes to the front ([`relay_front`]; design
//! `2026-11-22-front-after-allow-design.md`) are portable too.

pub mod account;
pub mod cmdline;
pub mod messages;
pub mod pass_toast;
pub mod payload;
pub mod relay_front;
pub mod relay_opts;
pub mod relay_ui;
pub mod ui;

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use win::app::run;

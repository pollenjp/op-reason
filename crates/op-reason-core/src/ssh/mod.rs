//! The portable parts of the SSH relay (`op-reason.exe --ssh-agent`, design `2026-11-17-ssh-relay-design.md`), tested
//! on Linux and used by the exe and by `op-reason-ssh` on WSL:
//!
//! - [`wire`]: the SSH agent protocol's framing and the messages the relay reads
//! - [`cmdline`]: Windows command lines, masked as the records show them, and ssh's destination
//! - [`roots`]: the root of a call, up the chain of the process that asks (`tests/cases/roots.json`)
//! - [`call`]: `OPREASON_SSH_CALL`, the facts of a call from WSL

pub mod call;
pub mod cmdline;
pub mod roots;
pub mod wire;

//! The portable parts of the SSH relay (`op-reason.exe --ssh-agent`, design `2026-11-17-ssh-relay-design.md`), tested
//! on Linux and used by the exe and by `op-reason-ssh` on WSL:
//!
//! - [`wire`]: the SSH agent protocol's framing and the messages the relay reads
pub mod wire;

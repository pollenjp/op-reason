//! Core of op-reason: the parts that do not depend on WSL, so that they build for Linux and Windows.
//!
//! - [`args`]: the wrapper's options, the reason, the mode, masking (the call table keeps the PowerShell entry in step)
//! - [`contract`]: the payload the entry passes to the launcher and the status the launcher writes back
//! - [`log`]: log records and appending with rotation; [`logview`]: the log viewer
//! - [`place`]: the repository and branch of a directory, read from git files
//! - [`lang`] and [`messages`]: English and Japanese texts
//! - [`result`]: the result of a call from the status, the exit code and `op`'s stderr
//! - [`launcher`]: the embedded launcher and the bootstrap that loads it

pub mod args;
pub mod base64;
pub mod contract;
pub mod json;
pub mod lang;
pub mod launcher;
pub mod log;
pub mod logview;
pub mod messages;
pub mod place;
pub mod result;
pub mod time;

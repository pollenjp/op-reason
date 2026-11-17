//! Core of op-reason: the parts that do not depend on WSL, so that they build for Linux and Windows.
//!
//! - [`args`]: the wrapper's options, the reason, the mode, masking (the call table keeps the PowerShell entry in step)
//! - [`contract`]: the payload the entry passes to the runner and the status the runner writes back
//! - [`log`]: log records and appending with rotation; [`logview`]: the log viewer
//! - [`place`]: the repository and branch of a directory, read from git files
//! - [`lang`] and [`messages`]: English and Japanese texts; [`settings`]: the language chosen in the window
//! - [`result`]: the result of a call from the status, the exit code and `op`'s stderr
//! - [`refs`]: the `op://` references a call reads (the reference table keeps the PowerShell entry in step)

pub mod args;
pub mod base64;
pub mod contract;
pub mod json;
pub mod lang;
pub mod log;
pub mod logview;
pub mod messages;
pub mod place;
pub mod refs;
pub mod result;
pub mod settings;
pub mod time;

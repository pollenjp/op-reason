//! The Windows-only part: lock and holder file, process list, window, toast, op.exe, console events and the flow
//! that ties them together.

pub mod app;
pub mod console;
pub mod holder;
pub mod mutex;
pub mod opexe;
pub mod procs;
pub mod toast;
pub mod window;

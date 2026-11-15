//! The Windows-only part: lock and holder file, process list, window, toast, op.exe and its Job Object, console events,
//! and the flows that tie them together (the entry face in [`app`], the runner face in [`run`]).

pub mod app;
pub mod console;
pub mod holder;
pub mod job;
pub mod mutex;
pub mod opexe;
pub mod procs;
pub mod run;
pub mod toast;
pub mod window;

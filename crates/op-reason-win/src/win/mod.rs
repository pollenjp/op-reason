//! The Windows-only part: lock and holder file, process list, window, toast, op.exe and its Job Object, console events,
//! opening an item in the 1Password app ([`shell`]), and the flows that tie them together (the entry face in [`app`],
//! the runner face in [`run`], the SSH relay in [`relay`]).

pub mod app;
pub mod console;
pub mod holder;
pub mod job;
pub mod mutex;
pub mod opexe;
pub mod procs;
pub mod relay;
pub mod run;
pub mod screen;
pub mod shell;
pub mod toast;
pub mod window;

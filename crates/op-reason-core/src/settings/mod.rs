//! op-reason's settings (design `2026-11-23-settings-design.md`): the file `settings.json` ([`file`]) and the values
//! that op-reason reads itself ([`values`]).

pub mod file;
pub mod values;

pub use file::{COPY_FILE, SETTINGS_FILE, VERSION, parse, read, remove, render, replace, write};

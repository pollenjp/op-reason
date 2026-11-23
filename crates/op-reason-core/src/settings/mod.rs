//! op-reason's settings (design `2026-11-23-settings-design.md`): the file `settings.json` ([`file`]) and the values
//! that op-reason reads itself ([`values`]).

pub mod catalog;
pub mod cli;
pub mod file;
pub mod values;

pub use file::{
    COPY_FILE, SETTINGS_FILE, VERSION, Values, edit, parse, put, read, remove, render, replace,
    values, write,
};

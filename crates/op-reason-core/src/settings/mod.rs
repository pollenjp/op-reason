//! op-reason's settings (design `2026-11-23-settings-design.md`): the file `settings.json` ([`file`]) and the values
//! that op-reason reads itself ([`values`]).

pub mod catalog;
pub mod cli;
pub mod engine;
#[cfg(test)]
pub(crate) mod fake;
pub mod file;
pub mod record;
pub mod relayinfo;
pub mod skill;
pub mod task;
pub mod text;
pub mod values;
pub mod wsl;

pub use file::{
    COPY_FILE, SETTINGS_FILE, VERSION, Values, edit, field, parse, put, read, remove, render,
    replace, values, write,
};

//! op-reason: a wrapper for the 1Password CLI (`op`) on WSL that requires a reason for every call, confines each
//! approval of the 1Password desktop app to that call, shows the caller and the reason in a window, and logs every
//! call. See the README and `docs/superpowers/specs/2026-11-15-op-reason-design.md`.
//!
//! One executable, chosen by the name it is called by: `op` (a symlink) or `op-reason` run the wrapper,
//! `op-reason-log` runs the log viewer.

#[cfg(unix)]
mod launch;
#[cfg(unix)]
mod lock;
#[cfg(unix)]
mod procfs;
#[cfg(unix)]
mod runner;
#[cfg(unix)]
mod signals;
#[cfg(unix)]
mod wrapper;

#[cfg(unix)]
fn main() {
    use op_reason_core::args::{Invocation, invocation};
    let mut argv = std::env::args_os();
    let argv0 = argv.next().unwrap_or_default();
    let args: Vec<std::ffi::OsString> = argv.collect();
    let code = match invocation(&argv0) {
        Invocation::LogViewer => wrapper::log_viewer(&args),
        Invocation::Wrapper(name) => wrapper::run(name, args),
    };
    std::process::exit(code);
}

#[cfg(not(unix))]
fn main() {
    eprintln!(
        "op-reason: this executable is for WSL (Linux); on Windows use op-reason.exe from op-reason-windows.zip"
    );
    std::process::exit(2);
}

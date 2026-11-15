//! `op-reason.exe`: `op-reason --reason "<why>" <op arguments...>` (plan C').

#[cfg(windows)]
fn main() {
    std::process::exit(op_reason_win::run());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("op-reason (plan C'): this is the Windows exe; build it for a Windows target");
    std::process::exit(2);
}

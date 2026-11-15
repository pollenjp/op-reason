//! A stand-in for op.exe in tests. Never talks to 1Password.
//!
//! - `FAKE_OP_DIR`: each call takes the next free number `n` and writes `call.<n>` (the arguments) and `env.<n>`
//!   (which variables it saw). Windows arguments are written as UTF-16LE units separated by a NUL unit, so the test
//!   can compare them exactly (unpaired surrogates included); elsewhere as UTF-8 bytes separated by NUL.
//! - `whoami`: exits with `FAKE_WHOAMI_RC` (default 1, "not signed in").
//! - `signout`: prints `SIGNOUT-OUT` and `SIGNOUT-ERR`, exits 0.
//! - anything else: copies stdin to stdout, sleeps `FAKE_SLEEP_MS`, prints `FAKE_STDERR` (default
//!   `FAKE-ERR 日本語`) to stderr and exits with `FAKE_RC` (default 0).

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

fn var(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

fn encode_args(args: &[OsString]) -> Vec<u8> {
    let mut out = Vec::new();
    for a in args {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            for u in a.encode_wide() {
                out.extend_from_slice(&u.to_le_bytes());
            }
            out.extend_from_slice(&[0, 0]);
        }
        #[cfg(not(windows))]
        {
            out.extend_from_slice(a.to_string_lossy().as_bytes());
            out.push(0);
        }
    }
    out
}

/// Writes `call.<n>` and `env.<n>` with the first free `n` (create_new makes concurrent calls safe).
fn record(dir: &Path, args: &[OsString]) {
    let _ = fs::create_dir_all(dir);
    for n in 1..10_000 {
        let Ok(mut f) = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(format!("call.{n}")))
        else {
            continue;
        };
        let _ = f.write_all(&encode_args(args));
        let seen = |k: &str| match std::env::var_os(k) {
            Some(v) => format!("{k}={}\n", v.to_string_lossy()),
            None => format!("{k}=<unset>\n"),
        };
        let env = [
            "OP_SERVICE_ACCOUNT_TOKEN",
            "OP_CONNECT_HOST",
            "OP_CONNECT_TOKEN",
            "OP_BIOMETRIC_UNLOCK_ENABLED",
            "OPREASON_OP_EXE",
        ]
        .iter()
        .map(|k| seen(k))
        .collect::<String>();
        let _ = fs::write(dir.join(format!("env.{n}")), env);
        return;
    }
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if let Some(dir) = std::env::var_os("FAKE_OP_DIR") {
        record(Path::new(&dir), &args);
    }
    match args.first().and_then(|a| a.to_str()) {
        Some("whoami") => {
            let rc = var("FAKE_WHOAMI_RC")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            std::process::exit(rc);
        }
        Some("signout") => {
            println!("SIGNOUT-OUT");
            eprintln!("SIGNOUT-ERR");
            std::process::exit(0);
        }
        _ => {}
    }
    let mut input = Vec::new();
    let _ = io::stdin().lock().read_to_end(&mut input);
    let mut out = io::stdout().lock();
    let _ = out.write_all(&input);
    let _ = out.flush();
    if let Some(ms) = var("FAKE_SLEEP_MS").and_then(|v| v.parse().ok()) {
        std::thread::sleep(Duration::from_millis(ms));
    }
    let msg = var("FAKE_STDERR").unwrap_or_else(|| "FAKE-ERR 日本語".to_owned());
    let mut err = io::stderr().lock();
    let _ = writeln!(err, "{msg}");
    let rc = var("FAKE_RC").and_then(|v| v.parse().ok()).unwrap_or(0);
    std::process::exit(rc);
}

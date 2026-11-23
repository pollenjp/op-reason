//! `wsl.exe` for the skill in WSL distributions (design `2026-11-23-settings-design.md` §10.2): the distributions
//! (`--list --quiet`, and `--running` for the ones that run, which starts none) and the skill script run inside one
//! (`-d <name> --exec sh -c <script>`, which starts it).

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use op_reason_core::settings::catalog;
use op_reason_core::settings::skill::{self, SkillAct, SkillNow};
use op_reason_core::settings::wsl::{self, Distro};

/// `wsl.exe`, or the fake of the tests (`OPREASON_WSL_EXE`).
fn exe() -> String {
    std::env::var("OPREASON_WSL_EXE")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "wsl.exe".to_owned())
}

/// What a run of `wsl.exe` gave.
struct Ran {
    code: Option<i32>,
    out: Vec<u8>,
    err: Vec<u8>,
}

/// Runs `wsl.exe` with `args` (UTF-8 output asked for), `input` on its stdin, for at most `limit`. `Err` when it could
/// not start or did not end in time.
fn run(args: &[String], input: &[u8], limit: Duration) -> Result<Ran, String> {
    let mut child = Command::new(exe())
        .args(args)
        .env("WSL_UTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("wsl.exe: {e}"))?;
    let mut stdin = child.stdin.take();
    let input = input.to_vec();
    let feeder = std::thread::spawn(move || {
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(&input);
        }
    });
    let mut out_pipe = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let mut err_pipe = child.stderr.take();
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let t0 = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if t0.elapsed() < limit => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "wsl.exe did not answer within {} s",
                    limit.as_secs()
                ));
            }
        }
    };
    let _ = feeder.join();
    let out = reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    Ok(Ran {
        code: status.code(),
        out,
        err,
    })
}

/// The WSL distributions (none without WSL), and which of them run.
pub fn distributions() -> Result<Vec<Distro>, String> {
    let list = |extra: &[&str]| -> Vec<String> {
        let mut args = vec!["--list".to_owned(), "--quiet".to_owned()];
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        match run(&args, b"", Duration::from_secs(15)) {
            Ok(Ran {
                code: Some(0), out, ..
            }) => wsl::names(&out),
            _ => Vec::new(),
        }
    };
    let all = list(&[]);
    if all.is_empty() {
        return Ok(Vec::new());
    }
    let running = list(&["--running"]);
    Ok(all
        .into_iter()
        .map(|name| Distro {
            running: running.contains(&name),
            name,
        })
        .collect())
}

/// The skill script inside the distribution `name` (which starts it): looks at, places or removes `want`.
pub fn skill(name: &str, act: SkillAct, want: &str, version: &str) -> Result<SkillNow, String> {
    if !catalog::distro_name(name) {
        return Err(format!("{name:?} is not the name of a WSL distribution"));
    }
    let mut args = vec![
        "-d".to_owned(),
        name.to_owned(),
        "--exec".to_owned(),
        "sh".to_owned(),
        "-c".to_owned(),
        skill::WSL_SCRIPT.to_owned(),
    ];
    args.extend(skill::wsl_args(act, version));
    let ran = run(&args, want.as_bytes(), Duration::from_secs(60))?;
    skill::parse_wsl(&wsl::text(&ran.out)).map_err(|e| {
        let err = wsl::text(&ran.err);
        let err = err.trim();
        if err.is_empty() {
            e
        } else {
            format!("{e}: {err}")
        }
    })
}

//! Black-box tests of op-reason.exe with the fake op.exe (crates/fake-op). Windows only.
//!
//! Build the fake first: `cargo build -p fake-op` (it lands next to the test binaries' parent, `target\debug\op.exe`;
//! `OPREASON_TEST_FAKE_OP` overrides the path). Desktop-mode tests really show the window on the runner's desktop.
//! One test clicks the window's language switch with the real mouse: it runs only where `CI` is set.
#![cfg(windows)]

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// The exe takes a machine-wide mutex; run these tests one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_op-reason"))
}

fn fake_op() -> PathBuf {
    if let Some(p) = std::env::var_os("OPREASON_TEST_FAKE_OP") {
        return PathBuf::from(p);
    }
    let me = std::env::current_exe().expect("current_exe");
    let p = me
        .parent()
        .and_then(Path::parent)
        .map(|d| d.join("op.exe"))
        .expect("target dir");
    assert!(
        p.is_file(),
        "build the fake op.exe first: cargo build -p fake-op (looked at {})",
        p.display()
    );
    p
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "op-reason-exe-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("fake")).unwrap();
        fs::create_dir_all(root.join("lad")).unwrap();
        Sandbox { root }
    }

    fn fake_dir(&self) -> PathBuf {
        self.root.join("fake")
    }

    fn log(&self) -> String {
        fs::read_to_string(self.root.join("lad").join("op-reason").join("log.jsonl"))
            .unwrap_or_default()
    }

    fn calls(&self) -> usize {
        (1..)
            .take_while(|n| self.fake_dir().join(format!("call.{n}")).exists())
            .count()
    }

    /// The arguments of call `n`, decoded from UTF-16LE units separated by a NUL unit.
    fn call(&self, n: usize) -> Vec<OsString> {
        let bytes = fs::read(self.fake_dir().join(format!("call.{n}"))).unwrap_or_default();
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        let mut out = Vec::new();
        let mut cur = Vec::new();
        for u in units {
            if u == 0 {
                out.push(OsString::from_wide(&cur));
                cur.clear();
            } else {
                cur.push(u);
            }
        }
        out
    }

    fn env_of(&self, n: usize) -> String {
        fs::read_to_string(self.fake_dir().join(format!("env.{n}"))).unwrap_or_default()
    }

    fn command(&self, args: &[&OsStr], envs: &[(&str, &str)]) -> Command {
        self.command_of(&exe(), args, envs)
    }

    /// Like [`Sandbox::command`], with another copy of the exe.
    fn command_of(&self, program: &Path, args: &[&OsStr], envs: &[(&str, &str)]) -> Command {
        let mut c = Command::new(program);
        c.args(args)
            .env("OPREASON_OP_EXE", fake_op())
            .env("LOCALAPPDATA", self.root.join("lad"))
            .env("FAKE_OP_DIR", self.fake_dir())
            .env("OPREASON_LANG", "en")
            .env_remove("OP_SERVICE_ACCOUNT_TOKEN")
            .env_remove("OP_CONNECT_HOST")
            .env_remove("OP_CONNECT_TOKEN")
            .env_remove("FAKE_RC")
            .env_remove("FAKE_WHOAMI_RC")
            .env_remove("FAKE_SLEEP_MS")
            .env_remove("FAKE_STDERR")
            .env_remove("CLAUDECODE")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("OPREASON_LOCK_WAIT");
        for (k, v) in envs {
            c.env(k, v);
        }
        c
    }

    fn run(&self, args: &[&OsStr], envs: &[(&str, &str)], stdin: &[u8]) -> Output {
        let mut child = self
            .command(args, envs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn op-reason.exe");
        {
            let mut i = child.stdin.take().unwrap();
            let _ = i.write_all(stdin);
        }
        child.wait_with_output().expect("wait op-reason.exe")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn s(v: &str) -> &OsStr {
    OsStr::new(v)
}

fn lines_with<'a>(log: &'a str, event: &str) -> Vec<&'a str> {
    let needle = format!(r#""event":"{event}""#);
    log.lines().filter(|l| l.contains(&needle)).collect()
}

#[test]
fn tricky_arguments_stdin_stderr_and_exit_code() {
    let _g = serial();
    let sb = Sandbox::new("tricky");
    let surrogate = OsString::from_wide(&[0xD800, u16::from(b'a')]);
    let tricky: Vec<OsString> = [
        "",
        "a b",
        "x\"y",
        "tr\\",
        "q\\\"z",
        "&|<>^%PATH%!",
        "日本語 の 引数",
        "  lead",
        "trail  ",
        "new\nline",
        "tab\there",
        "\"",
        "\\",
        "😀",
    ]
    .iter()
    .map(OsString::from)
    .chain(std::iter::once(surrogate))
    .collect();
    let mut args: Vec<&OsStr> = vec![s("--reason"), s("test tricky arguments")];
    args.extend(tricky.iter().map(OsString::as_os_str));
    let mut stdin: Vec<u8> = (0..40).flat_map(|_| 0u8..=255).collect();
    stdin.extend_from_slice(b"\r\n\0end\n");

    let out = sb.run(&args, &[("FAKE_RC", "42")], &stdin);

    assert_eq!(
        sb.call(1),
        tricky,
        "op got the arguments one by one, unchanged"
    );
    assert_eq!(out.stdout, stdin, "stdin bytes reach op's stdout unchanged");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "FAKE-ERR 日本語\n",
        "stderr carries only op's text"
    );
    assert_eq!(out.status.code(), Some(42), "the exit code is op's");
    assert_eq!(
        sb.call(2),
        vec![OsString::from("whoami")],
        "whoami after op"
    );
    assert_eq!(sb.calls(), 2, "no signout without a session");
    assert!(
        sb.env_of(1).contains("OPREASON_OP_EXE=<unset>"),
        "the override is not passed to op"
    );
}

#[test]
fn closes_the_session_when_signed_in() {
    let _g = serial();
    let sb = Sandbox::new("close");
    let out = sb.run(
        &[s("--reason"), s("close"), s("item"), s("get"), s("x")],
        &[("FAKE_WHOAMI_RC", "0")],
        b"",
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(sb.calls(), 3);
    assert_eq!(
        sb.call(3),
        vec![OsString::from("signout"), OsString::from("--all")]
    );
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !all.contains("SIGNOUT"),
        "whoami and signout output stays hidden: {all}"
    );
    let end = sb.log();
    let end = lines_with(&end, "end");
    assert_eq!(end.len(), 1);
    assert!(end[0].contains(r#""signout":"ok""#), "{}", end[0]);
    assert!(end[0].contains(r#""result":"approved""#), "{}", end[0]);
    assert!(
        end[0].contains(r#""window":"shown""#),
        "the window was shown: {}",
        end[0]
    );
}

#[test]
fn records_start_and_end_with_masked_arguments() {
    let _g = serial();
    let sb = Sandbox::new("records");
    let out = sb.run(
        &[
            s("--reason"),
            s("create an item"),
            s("item"),
            s("create"),
            s("password=hunter2"),
        ],
        &[("CLAUDE_CODE_SESSION_ID", "sess-1")],
        b"",
    );
    assert_eq!(out.status.code(), Some(0));
    let log = sb.log();
    assert!(
        !log.contains("hunter2"),
        "secrets are masked in the record: {log}"
    );
    let start = lines_with(&log, "start");
    assert_eq!(start.len(), 1, "{log}");
    for want in [
        r#""v":1"#,
        r#""mode":"desktop""#,
        r#""kind":"desktop""#,
        r#""lock":"held""#,
        r#""session":"sess-1""#,
        r#""reason":"create an item""#,
        r#""args":["item","create","password=***"]"#,
        r#""herdr":null"#,
    ] {
        assert!(start[0].contains(want), "start has {want}: {}", start[0]);
    }
    let end = lines_with(&log, "end");
    assert!(
        end[0].contains(r#""watch":{"start":null,"end":null}"#),
        "{}",
        end[0]
    );
    assert!(end[0].contains(r#""exit":0"#), "{}", end[0]);
}

#[test]
fn denied_and_timed_out_prompts_are_classified() {
    let _g = serial();
    let sb = Sandbox::new("denied");
    let out = sb.run(
        &[s("--reason"), s("deny"), s("vault"), s("list")],
        &[
            ("FAKE_RC", "1"),
            (
                "FAKE_STDERR",
                "[ERROR] 2026/11/15 06:05:00 authorization prompt dismissed, please try again",
            ),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(1));
    let log = sb.log();
    assert!(
        lines_with(&log, "end")[0].contains(r#""result":"denied""#),
        "{log}"
    );

    let sb = Sandbox::new("timeout");
    let _ = sb.run(
        &[s("--reason"), s("time out"), s("vault"), s("list")],
        &[
            ("FAKE_RC", "1"),
            (
                "FAKE_STDERR",
                "[ERROR] 2026/11/15 06:05:00 authorization timeout",
            ),
        ],
        b"",
    );
    let log = sb.log();
    assert!(
        lines_with(&log, "end")[0].contains(r#""result":"timeout""#),
        "{log}"
    );
}

#[test]
fn help_runs_without_window_and_closes_the_session() {
    let _g = serial();
    let sb = Sandbox::new("help");
    // `--help` after an op command is op's help (help mode); `op-reason --help` alone would be the wrapper's
    let out = sb.run(&[s("vault"), s("--help")], &[("FAKE_WHOAMI_RC", "0")], b"");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        sb.call(1),
        vec![OsString::from("vault"), OsString::from("--help")]
    );
    assert_eq!(sb.calls(), 3);
    let log = sb.log();
    let end = lines_with(&log, "end");
    assert!(
        end[0].contains(r#""window":"skipped""#) && end[0].contains(r#""toast":"skipped""#),
        "{}",
        end[0]
    );
    assert!(
        lines_with(&log, "start")[0].contains(r#""mode":"help""#),
        "{log}"
    );
}

#[test]
fn service_account_keeps_the_token_and_skips_signout() {
    let _g = serial();
    let sb = Sandbox::new("service");
    let out = sb.run(
        &[s("--reason"), s("ci"), s("vault"), s("list")],
        &[("OP_SERVICE_ACCOUNT_TOKEN", "tok-test")],
        b"",
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(sb.calls(), 1, "no whoami, no signout");
    let env = sb.env_of(1);
    assert!(env.contains("OP_SERVICE_ACCOUNT_TOKEN=tok-test"), "{env}");
    assert!(env.contains("OP_BIOMETRIC_UNLOCK_ENABLED=false"), "{env}");
    let log = sb.log();
    assert!(
        lines_with(&log, "end")[0].contains(r#""signout":"skipped""#),
        "{log}"
    );
    assert!(
        lines_with(&log, "start")[0].contains(r#""kind":"service-account""#),
        "{log}"
    );
}

#[test]
fn desktop_removes_service_variables() {
    let _g = serial();
    let sb = Sandbox::new("desktop-env");
    let out = sb.run(
        &[s("--reason"), s("d"), s("vault"), s("list")],
        &[("OP_CONNECT_HOST", "h")],
        b"",
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(
        sb.env_of(1).contains("OP_CONNECT_HOST=<unset>"),
        "{}",
        sb.env_of(1)
    );
}

#[test]
fn refusals() {
    let _g = serial();
    let sb = Sandbox::new("refuse");
    let out = sb.run(&[s("vault"), s("list")], &[], b"");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Usage: op-reason --reason")
            && err.contains("op-reason: There is no reason (--reason)"),
        "{err}"
    );
    let out = sb.run(&[s("--reason"), s(" \t "), s("vault"), s("list")], &[], b"");
    assert_eq!(out.status.code(), Some(2));
    let out = sb.run(
        &[s("--reason"), s("x"), s("vault"), s("list")],
        &[("OPREASON_OP_EXE", r"C:\no\such\op.exe")],
        b"",
    );
    assert_eq!(out.status.code(), Some(127));
    assert_eq!(sb.calls(), 0, "op never ran");
    let log = sb.log();
    assert_eq!(
        refusal_whys(&log),
        ["no-reason", "no-reason", "no-op-exe"],
        "{log}"
    );
}

fn refusal_whys(log: &str) -> Vec<&str> {
    lines_with(log, "refused")
        .iter()
        .map(|l| {
            let i = l.find(r#""why":""#).unwrap() + 7;
            &l[i..i + l[i..].find('"').unwrap()]
        })
        .collect()
}

/// op-reason's own command line holds op's arguments too, so a command line too long for op.exe is usually too long
/// for op-reason.exe already. It can only fit op-reason and not op when op.exe's path is longer: copy the fake op
/// into a long directory and pick an argument that falls in between.
#[test]
fn too_long_for_op_exe_is_refused_before_starting_it() {
    use op_reason_win::cmdline::{MAX_COMMAND_LINE, command_line_len};
    let _g = serial();
    let sb = Sandbox::new("long");
    let dir = sb.root.join("d".repeat(150));
    fs::create_dir_all(&dir).unwrap();
    let long_op = dir.join("op.exe");
    fs::copy(fake_op(), &long_op).unwrap();
    let exe = exe();
    let one = OsString::from("y");
    let base_e = command_line_len(exe.as_os_str(), &[s("--reason"), s("x"), s("read"), &one]) - 1;
    let base_o = command_line_len(long_op.as_os_str(), &[s("read"), &one]) - 1;
    assert!(
        base_o > base_e + 32,
        "the op path is long enough ({base_o} vs {base_e})"
    );
    let len = MAX_COMMAND_LINE - base_e - 16;
    assert!(base_o + len > MAX_COMMAND_LINE);
    let arg = OsString::from("y".repeat(len));
    let out = sb.run(
        &[s("--reason"), s("x"), s("read"), &arg],
        &[("OPREASON_OP_EXE", long_op.to_str().unwrap())],
        b"",
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(sb.calls(), 0, "op never ran");
    let log = sb.log();
    assert_eq!(refusal_whys(&log), ["too-long"], "{log}");
}

#[test]
fn no_arguments_prints_usage_and_records_nothing() {
    let _g = serial();
    let sb = Sandbox::new("usage");
    let out = sb.run(&[], &[], b"");
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("Usage: op-reason --reason"));
    assert_eq!(sb.log(), "");
    assert_eq!(sb.calls(), 0);
}

#[test]
fn a_second_call_waits_for_the_lock_and_times_out() {
    let _g = serial();
    let first = Sandbox::new("lock1");
    let mut holder = first
        .command(
            &[s("--reason"), s("hold the lock"), s("vault"), s("list")],
            &[("FAKE_SLEEP_MS", "6000")],
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // op starts only after the lock is taken
    let t0 = Instant::now();
    while first.calls() == 0 && t0.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(first.calls(), 1, "the first call started op");
    // the same LOCALAPPDATA as the first call: the holder file is there
    let second = Sandbox::new("lock2");
    let lad = first.root.join("lad");
    let t1 = Instant::now();
    let out = second.run(
        &[s("--reason"), s("wait"), s("vault"), s("list")],
        &[
            ("OPREASON_LOCK_WAIT", "1"),
            ("LOCALAPPDATA", lad.to_str().unwrap()),
        ],
        b"",
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(75), "{err}");
    assert!(
        t1.elapsed() >= Duration::from_millis(900),
        "it waited about a second"
    );
    assert!(
        err.contains(r#"reason "hold the lock" op vault list"#),
        "names the holder: {err}"
    );
    assert_eq!(second.calls(), 0);
    assert!(first.log().contains(r#""why":"lock-timeout""#));
    assert_eq!(holder.wait().unwrap().code(), Some(0));
    assert!(
        !lad.join("op-reason").join("holder.json").exists(),
        "the holder file is removed at the end"
    );
}

/// The wrapper's own options come from the core's planning, as in the WSL binary: nothing is started or recorded.
#[test]
fn wrapper_options_from_the_core() {
    let _g = serial();
    let sb = Sandbox::new("options");
    let out = sb.run(&[s("--version")], &[], b"");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("op-reason {}\n", env!("CARGO_PKG_VERSION"))
    );
    let out = sb.run(&[s("--skill")], &[], b"");
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("---\nname: op-reason"));
    let out = sb.run(&[s("--completion"), s("fish"), s("op-reason")], &[], b"");
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("op-reason"));
    let out = sb.run(&[s("--completion"), s("zsh")], &[], b"");
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(sb.calls(), 0, "op never ran");
    assert_eq!(sb.log(), "", "nothing recorded");
}

/// `op-reason --log` reads the Windows records with the core's log viewer (A' has no viewer on Windows).
#[test]
fn log_viewer_reads_the_windows_records() {
    let _g = serial();
    let sb = Sandbox::new("logview");
    let out = sb.run(
        &[s("--reason"), s("view me later"), s("vault"), s("list")],
        &[],
        b"",
    );
    assert_eq!(out.status.code(), Some(0));
    let out = sb.run(&[s("--log"), s("--json")], &[], b"");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("view me later") && text.contains(r#""result":"approved""#),
        "{text}"
    );
}

/// Installed under the name `op.exe` (to type `op` as on WSL), the exe must not count itself as "another op.exe":
/// the orange warning would always show and the record's `others` would never be 0.
#[test]
fn installed_as_op_exe_does_not_count_itself() {
    let _g = serial();
    let sb = Sandbox::new("as-op");
    let dir = sb.root.join("bin");
    fs::create_dir_all(&dir).unwrap();
    let as_op = dir.join("op.exe");
    fs::copy(exe(), &as_op).unwrap();
    let out = sb
        .command_of(
            &as_op,
            &[s("--reason"), s("installed as op"), s("vault"), s("list")],
            &[],
        )
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let log = sb.log();
    let end = lines_with(&log, "end");
    assert!(end[0].contains(r#""others":0"#), "{}", end[0]);
}

/// From PowerShell the exe is a native command, so the `--` before `op run`'s command reaches op as typed, quoted or
/// not (the PowerShell entry is a script, which takes an unquoted `--` for itself). Windows PowerShell 5.1, and pwsh
/// when it is installed. Service-account calls: no window.
#[test]
fn powershell_passes_the_double_dash_to_op() {
    let _g = serial();
    let mut shells = vec![PathBuf::from("powershell.exe")];
    if Command::new("pwsh.exe").arg("-Version").output().is_ok() {
        shells.push(PathBuf::from("pwsh.exe"));
    }
    let script = "& $env:OPREASON_TEST_EXE --reason 'dash test' run -- a; \
                  & $env:OPREASON_TEST_EXE --reason 'dash test' run '--' b";
    for shell in shells {
        let sb = Sandbox::new("psdash");
        let out = sb
            .command_of(
                &shell,
                &[
                    s("-NoProfile"),
                    s("-NonInteractive"),
                    s("-Command"),
                    s(script),
                ],
                &[
                    ("OPREASON_TEST_EXE", exe().to_str().unwrap()),
                    ("OP_SERVICE_ACCOUNT_TOKEN", "test-token"),
                ],
            )
            .stdin(Stdio::null())
            .output()
            .expect("start PowerShell");
        let shell = shell.display();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{shell}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            sb.call(1),
            ["run", "--", "a"].map(OsString::from),
            "{shell}, unquoted"
        );
        assert_eq!(
            sb.call(2),
            ["run", "--", "b"].map(OsString::from),
            "{shell}, quoted"
        );
    }
}

// --- the runner face (`op-reason.exe --run`, started by the WSL binary) and the Job Object

fn b64(text: &str) -> String {
    op_reason_core::base64::encode(text.as_bytes())
}

impl Sandbox {
    fn status_path(&self) -> PathBuf {
        self.root.join("status.json")
    }

    /// The status file as JSON (null when there is none).
    fn status(&self) -> serde_json::Value {
        fs::read_to_string(self.status_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(serde_json::Value::Null)
    }

    /// A desktop payload as the WSL binary sends it (herdr and a note in the facts).
    fn payload(&self, mode: &str, args: &[&str]) -> serde_json::Value {
        let mut p = serde_json::json!({
            "v": 1,
            "mode": mode,
            "op": fake_op(),
            "args64": args.iter().map(|a| b64(a)).collect::<Vec<_>>(),
            "status": self.status_path(),
            "lang": "en",
            "lock": "held",
            "lockWait": 5,
            "stderr": "inherit",
        });
        if mode == "desktop" {
            p["facts"] = serde_json::json!({
                "time": "2026-11-15 12:00:00",
                "caller": {"type": "claude", "pid": 4242, "session": "s", "detail": null},
                "herdr": {"workspace": "w1", "label": "ws label", "pane": "w1:p2"},
                "place": {"repo": "o/r", "branch": "main", "cwd": "~/src/r"},
                "reason": "test the runner",
                "command": "op item get x",
                "notes": [{"kind": "shared-cleared"}]
            });
        }
        p
    }

    /// `op-reason.exe --run` with the payload in `OPREASON_PAYLOAD`.
    fn run_payload(&self, payload: &str, envs: &[(&str, &str)], stdin: &[u8]) -> Output {
        let mut child = self
            .command(&[s("--run")], envs)
            .env("OPREASON_PAYLOAD", payload)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn op-reason.exe --run");
        {
            let mut i = child.stdin.take().unwrap();
            let _ = i.write_all(stdin);
        }
        child.wait_with_output().expect("wait op-reason.exe --run")
    }

    /// The process id that the fake op.exe of call `n` recorded.
    fn op_pid(&self, n: usize) -> Option<u32> {
        self.env_of(n)
            .lines()
            .find_map(|l| l.strip_prefix("PID="))
            .and_then(|p| p.trim().parse().ok())
    }
}

fn process_alive(pid: u32) -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .expect("tasklist");
    String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
}

/// Waits until `cond` holds, up to `limit`.
fn wait_until(limit: Duration, cond: impl Fn() -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < limit {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cond()
}

#[test]
fn run_desktop_follows_the_launcher_contract() {
    let _g = serial();
    let sb = Sandbox::new("run-desktop");
    let p = sb.payload("desktop", &["item", "get", "a b", "日本語"]);
    let stdin: Vec<u8> = (0u8..=255).chain(*b"\r\n\0end\n").collect();
    let out = sb.run_payload(
        &p.to_string(),
        &[("FAKE_RC", "7"), ("FAKE_STDERR", "op says hi")],
        &stdin,
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(7), "op's exit code: {err}");
    assert_eq!(
        out.stdout, stdin,
        "stdin reaches op and comes out on stdout"
    );
    assert_eq!(
        err.trim_end(),
        "op says hi",
        "stderr has only op's (inherited)"
    );
    assert_eq!(
        sb.call(1),
        ["item", "get", "a b", "日本語"]
            .map(OsString::from)
            .to_vec()
    );
    assert!(
        sb.env_of(1).contains("OPREASON_PAYLOAD=<unset>"),
        "the payload does not reach op: {}",
        sb.env_of(1)
    );
    assert_eq!(sb.call(2), vec![OsString::from("whoami")]);
    assert_eq!(sb.calls(), 2, "not signed in: no signout");
    let st = sb.status();
    assert_eq!(st["started"], true, "{st}");
    assert_eq!(st["exit"], 7, "{st}");
    assert_eq!(st["error"], "", "{st}");
    assert_eq!(st["window"], "shown", "{st}");
    assert_eq!(st["signout"], "not-signed-in", "{st}");
    assert_eq!(st["lock"], "held", "{st}");
    assert_eq!(st["runner"], "exe", "{st}");
    assert_eq!(st["ui"]["title"], "1Password CLI approval request", "{st}");
    let lines = st["ui"]["lines"].as_array().cloned().unwrap_or_default();
    assert!(
        lines.contains(&serde_json::json!([
            "herdr",
            "workspace \"ws label\" · pane w1:p2"
        ])),
        "the herdr line of a WSL call: {st}"
    );
    assert!(
        lines.iter().any(|l| l[0] == "Note"),
        "the note of a WSL call: {st}"
    );
    assert_eq!(st["ui"]["toast"].as_array().map(Vec::len), Some(3), "{st}");
    assert!(sb.log().is_empty(), "the runner writes no records");
    assert!(
        !sb.root
            .join("lad")
            .join("op-reason")
            .join("holder.json")
            .exists(),
        "the holder file is removed at the end"
    );
}

#[test]
fn run_closes_the_session_and_scans_stderr_when_asked() {
    let _g = serial();
    let sb = Sandbox::new("run-scan");
    let mut p = sb.payload("desktop", &["item", "get", "x"]);
    p["stderr"] = "scan".into();
    let denial = "[ERROR] 2026/11/15 00:00:00 authorization prompt dismissed, please try again";
    let out = sb.run_payload(
        &p.to_string(),
        &[
            ("FAKE_RC", "1"),
            ("FAKE_WHOAMI_RC", "0"),
            ("FAKE_STDERR", denial),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(denial),
        "relayed"
    );
    let st = sb.status();
    assert_eq!(st["stderrMatch"], "denied", "{st}");
    assert_eq!(st["signout"], "ok", "{st}");
    assert_eq!(
        sb.call(3),
        vec![OsString::from("signout"), OsString::from("--all")]
    );
}

#[test]
fn run_help_and_service_modes() {
    let _g = serial();
    let sb = Sandbox::new("run-help");
    let out = sb.run_payload(&sb.payload("help", &["--version"]).to_string(), &[], b"");
    assert_eq!(out.status.code(), Some(0));
    let st = sb.status();
    assert_eq!(
        (&st["window"], &st["toast"], &st["signout"], st.get("ui")),
        (
            &serde_json::json!("skipped"),
            &serde_json::json!("skipped"),
            &serde_json::json!("not-signed-in"),
            None
        ),
        "{st}"
    );
    assert_eq!(sb.call(2), vec![OsString::from("whoami")]);

    let sb = Sandbox::new("run-service");
    let out = sb.run_payload(
        &sb.payload("service", &["vault", "list"]).to_string(),
        &[("OP_SERVICE_ACCOUNT_TOKEN", "tok-run")],
        b"",
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(
        sb.env_of(1).contains("OP_SERVICE_ACCOUNT_TOKEN=tok-run"),
        "the token the WSL binary passed reaches op: {}",
        sb.env_of(1)
    );
    assert_eq!(sb.calls(), 1, "service mode closes no session");
    assert_eq!(sb.status()["signout"], "skipped");
}

/// Starts an entry-face call that holds the mutex while its op sleeps; returns it once op runs.
fn hold_the_lock(first: &Sandbox, ms: &str) -> std::process::Child {
    let holder = first
        .command(
            &[s("--reason"), s("hold the lock"), s("vault"), s("list")],
            &[("FAKE_SLEEP_MS", ms)],
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(
        wait_until(Duration::from_secs(20), || first.calls() == 1),
        "the first call started op"
    );
    holder
}

#[test]
fn run_shows_its_first_write_while_waiting_and_times_out() {
    let _g = serial();
    let first = Sandbox::new("run-lock1");
    let mut holder = hold_the_lock(&first, "8000");
    let sb = Sandbox::new("run-lock2");
    let lad = first.root.join("lad");
    let mut p = sb.payload("desktop", &["vault", "list"]);
    p["lockWait"] = 2.into();
    let child = sb
        .command(&[s("--run")], &[("LOCALAPPDATA", lad.to_str().unwrap())])
        .env("OPREASON_PAYLOAD", p.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // (a file being written may be seen empty for a moment)
    let read = || fs::read_to_string(sb.status_path()).unwrap_or_default();
    assert!(
        wait_until(Duration::from_secs(10), || !read().is_empty()),
        "the first write comes at once"
    );
    let first_write = read();
    let out = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        first_write,
        op_reason_win::payload::FIRST_STATUS,
        "while waiting for the mutex, the status file holds the first write"
    );
    assert_eq!(out.status.code(), Some(75), "{err}");
    assert!(
        err.contains("Waiting for another op call"),
        "says once that it waits: {err}"
    );
    let st = sb.status();
    assert_eq!(
        (&st["started"], &st["error"], &st["lock"]),
        (
            &serde_json::json!(false),
            &serde_json::json!("lock-timeout"),
            &serde_json::json!("timeout")
        ),
        "{st}"
    );
    assert!(
        st["holder"]
            .as_str()
            .is_some_and(|h| h.contains(r#"reason "hold the lock""#)),
        "names the holder: {st}"
    );
    assert_eq!(sb.calls(), 0, "op was not started");
    assert_eq!(holder.wait().unwrap().code(), Some(0));
}

#[test]
fn run_with_lock_skipped_does_not_wait_and_keeps_the_sessions() {
    let _g = serial();
    let first = Sandbox::new("run-skip1");
    let mut holder = hold_the_lock(&first, "6000");
    let sb = Sandbox::new("run-skip2");
    let mut p = sb.payload("desktop", &["item", "list"]);
    p["lock"] = "skipped".into();
    let t0 = Instant::now();
    let out = sb.run_payload(&p.to_string(), &[("FAKE_WHOAMI_RC", "0")], b"");
    assert_eq!(out.status.code(), Some(0));
    assert!(
        t0.elapsed() < Duration::from_secs(4),
        "did not wait for the mutex"
    );
    let st = sb.status();
    assert_eq!(
        (&st["lock"], &st["signout"]),
        (&"skipped".into(), &"skipped".into()),
        "{st}"
    );
    assert_eq!(sb.calls(), 1, "no whoami, no signout");
    assert_eq!(holder.wait().unwrap().code(), Some(0));
}

#[test]
fn run_notify_shows_the_toast_only() {
    let _g = serial();
    let sb = Sandbox::new("run-notify");
    let mut p = sb.payload("notify", &[]);
    p["notify"] = serde_json::json!({"kind": "shared-approval", "when": "start", "cleared": true, "time": "12:34:56"});
    p["lock"] = "skipped".into();
    let out = sb.run_payload(&p.to_string(), &[], b"");
    assert_eq!(out.status.code(), Some(0));
    let st = sb.status();
    assert_eq!(
        st["ui"]["title"], "Cleared a shared 1Password CLI approval",
        "{st}"
    );
    assert_eq!(
        st["ui"]["lines"][0],
        serde_json::json!(["Time", "12:34:56"]),
        "{st}"
    );
    assert_ne!(st["toast"], "skipped", "{st}");
    assert_eq!(sb.calls(), 0, "notify starts no op");
}

#[test]
fn run_refuses_bad_payloads_without_starting_op() {
    let _g = serial();
    let sb = Sandbox::new("run-bad");
    let out = sb.run_payload("", &[], b"");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("OPREASON_PAYLOAD is empty"));
    let out = sb.run_payload("not json", &[], b"");
    assert_eq!(out.status.code(), Some(1));
    assert!(!sb.status_path().exists(), "no status path known");

    let mut p = sb.payload("desktop", &["x"]);
    p["v"] = 2.into();
    let out = sb.run_payload(&p.to_string(), &[], b"");
    assert_eq!(out.status.code(), Some(1));
    let st = sb.status();
    assert!(
        st["error"]
            .as_str()
            .is_some_and(|e| e.contains("version 1, the caller sent 2")),
        "{st}"
    );
    assert_eq!(st["started"], false);

    let mut p = sb.payload("desktop", &["x"]);
    p["mode"] = "later".into();
    assert_eq!(
        sb.run_payload(&p.to_string(), &[], b"").status.code(),
        Some(1)
    );
    assert_eq!(sb.status()["error"], "unknown mode: later");

    let mut p = sb.payload("desktop", &["x"]);
    p["args64"] = serde_json::json!(["***"]);
    assert_eq!(
        sb.run_payload(&p.to_string(), &[], b"").status.code(),
        Some(1)
    );
    assert_eq!(
        sb.status()["error"],
        "argument 1 of the payload is not base64"
    );

    let mut p = sb.payload("desktop", &["x"]);
    p["op"] = sb
        .root
        .join("no-such-op.exe")
        .to_string_lossy()
        .into_owned()
        .into();
    let out = sb.run_payload(&p.to_string(), &[], b"");
    assert_eq!(out.status.code(), Some(127));
    assert!(
        sb.status()["error"]
            .as_str()
            .is_some_and(|e| e.starts_with("op.exe not found: ")),
        "{}",
        sb.status()
    );

    let out = sb.run(&[s("--run"), s("extra")], &[], b"");
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(sb.calls(), 0, "op never started");
}

#[test]
fn run_stops_when_the_status_file_cannot_be_written() {
    let _g = serial();
    let sb = Sandbox::new("run-nostatus");
    let mut p = sb.payload("desktop", &["item", "get", "x"]);
    p["status"] = sb
        .root
        .join("no-such-dir")
        .join("status.json")
        .to_string_lossy()
        .into_owned()
        .into();
    let out = sb.run_payload(&p.to_string(), &[], b"");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("cannot write the status file"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        sb.calls(),
        0,
        "without the first write nothing starts: the WSL binary may start the launcher instead"
    );

    // an empty status path (wslpath -w gave nothing) is no path to write: the same
    p["status"] = "".into();
    let out = sb.run_payload(&p.to_string(), &[], b"");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("status path of the payload is empty"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(sb.calls(), 0, "op never started");
}

/// Starts a call whose op sleeps, kills the exe once op runs, and checks that op ends too.
fn killing_the_exe_ends_op(sb: &Sandbox, mut cmd: Command) {
    let mut child = cmd
        .env("FAKE_SLEEP_MS", "60000")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(
        wait_until(Duration::from_secs(20), || sb.op_pid(1).is_some()),
        "op started"
    );
    let pid = sb.op_pid(1).unwrap();
    assert!(process_alive(pid), "op runs");
    child.kill().unwrap();
    let _ = child.wait();
    assert!(
        wait_until(Duration::from_secs(10), || !process_alive(pid)),
        "op.exe {pid} ended with the exe (Job Object)"
    );
}

#[test]
fn killing_the_exe_ends_op_exe() {
    let _g = serial();
    let sb = Sandbox::new("job-entry");
    let cmd = sb.command(
        &[s("--reason"), s("kill the exe"), s("vault"), s("list")],
        &[],
    );
    killing_the_exe_ends_op(&sb, cmd);

    let sb = Sandbox::new("job-run");
    let mut cmd = sb.command(&[s("--run")], &[]);
    cmd.env(
        "OPREASON_PAYLOAD",
        sb.payload("desktop", &["vault", "list"]).to_string(),
    );
    killing_the_exe_ends_op(&sb, cmd);
}

// ---- The language chosen in the window (design 2026-11-15-language-toggle-design.md)

impl Sandbox {
    /// `%LOCALAPPDATA%\op-reason\settings.json` of this sandbox.
    fn settings_file(&self) -> PathBuf {
        self.root
            .join("lad")
            .join("op-reason")
            .join("settings.json")
    }

    fn set_settings(&self, text: &str) {
        let f = self.settings_file();
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, text).unwrap();
    }
}

#[test]
fn the_entry_face_speaks_the_chosen_language_before_opreason_lang() {
    let _g = serial();
    let sb = Sandbox::new("lang-entry");
    sb.set_settings(r#"{"v":1,"lang":"ja"}"#);
    let out = sb.run(&[s("vault"), s("list")], &[("OPREASON_LANG", "en")], b"");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2));
    assert!(err.contains("理由 (--reason) がありません"), "{err}");
    sb.set_settings("{broken");
    let out = sb.run(&[s("vault"), s("list")], &[("OPREASON_LANG", "en")], b"");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("There is no reason (--reason)"),
        "a broken settings.json is no choice: {err}"
    );
    let out = sb.run(
        &[s("vault"), s("list")],
        &[("OPREASON_LANG", ""), ("LANG", "ja_JP.UTF-8")],
        b"",
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("There is no reason (--reason)"),
        "LANG no longer chooses Japanese: {err}"
    );
}

#[test]
fn run_follows_the_chosen_language_and_reports_it() {
    let _g = serial();
    let sb = Sandbox::new("lang-run");
    let p = sb.payload("desktop", &["vault", "list"]).to_string();
    let out = sb.run_payload(&p, &[], b"");
    assert_eq!(out.status.code(), Some(0));
    let st = sb.status();
    assert_eq!(st["ui"]["title"], "1Password CLI approval request", "{st}");
    assert!(
        st.get("chosenLang").is_some_and(serde_json::Value::is_null),
        "nothing chosen: chosenLang null: {st}"
    );
    sb.set_settings(r#"{"v":1,"lang":"ja","later":[1]}"#);
    let out = sb.run_payload(&p, &[("OPREASON_LANG", "en")], b"");
    assert_eq!(out.status.code(), Some(0));
    let st = sb.status();
    assert_eq!(
        st["ui"]["title"], "1Password CLI の承認の依頼",
        "the chosen language beats the payload's en: {st}"
    );
    assert_eq!(st["chosenLang"], "ja", "{st}");
    assert_eq!(
        fs::read_to_string(sb.settings_file()).unwrap(),
        r#"{"v":1,"lang":"ja","later":[1]}"#,
        "a call without a click leaves the file alone"
    );
    let mut n = sb.payload("notify", &[]);
    n["notify"] = serde_json::json!({"kind": "shared-approval", "when": "end", "cleared": false, "time": "12:00:00"});
    n["lock"] = "skipped".into();
    sb.run_payload(&n.to_string(), &[], b"");
    let st = sb.status();
    assert_eq!(
        st["ui"]["title"], "共有の 1Password CLI の承認を消せませんでした",
        "{st}"
    );
    sb.set_settings("not json");
    sb.run_payload(&p, &[], b"");
    let st = sb.status();
    assert_eq!(st["ui"]["title"], "1Password CLI approval request", "{st}");
    assert!(
        st["chosenLang"].is_null(),
        "a broken file: chosenLang null: {st}"
    );
}

mod click {
    //! A real click on the window's switch, through the mouse of the desktop (CI only).
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetClientRect, GetForegroundWindow, GetWindowTextW, IsWindowVisible,
        SetCursorPos,
    };
    use windows::core::{PCWSTR, w};

    #[link(name = "user32")]
    unsafe extern "system" {
        fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra: usize);
    }
    const LEFTDOWN: u32 = 0x0002;
    const LEFTUP: u32 = 0x0004;

    /// The approval window, once visible.
    pub fn window() -> Option<HWND> {
        // SAFETY: plain Win32 queries with valid arguments.
        unsafe {
            let h = FindWindowW(w!("op-reason-approval"), PCWSTR::null()).ok()?;
            IsWindowVisible(h).as_bool().then_some(h)
        }
    }

    pub fn title(h: HWND) -> String {
        let mut buf = [0u16; 256];
        // SAFETY: valid buffer.
        let n = unsafe { GetWindowTextW(h, &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    pub fn foreground() -> HWND {
        // SAFETY: no arguments.
        unsafe { GetForegroundWindow() }
    }

    /// Clicks `日本語`, the right end of the switch: 26 DIPs left of the client area's right edge (its 14 DIP padding,
    /// then about the middle of the word) and 21 DIPs above its bottom (its 12 DIP padding and 2 DIP gap, then a little
    /// above the footer's last baseline). Returns where it clicked, for the failure message.
    pub fn click_japanese(h: HWND) -> String {
        // SAFETY: Win32 calls with valid arguments; the window belongs to another process of this desktop.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let k = GetDpiForWindow(h).max(96) as f32 / 96.0;
            let mut pt = POINT {
                x: rc.right - (26.0 * k).round() as i32,
                y: rc.bottom - (21.0 * k).round() as i32,
            };
            let client = pt;
            let _ = ClientToScreen(h, &mut pt);
            let _ = SetCursorPos(pt.x, pt.y);
            std::thread::sleep(std::time::Duration::from_millis(100));
            mouse_event(LEFTDOWN, 0, 0, 0, 0);
            std::thread::sleep(std::time::Duration::from_millis(60));
            mouse_event(LEFTUP, 0, 0, 0, 0);
            format!(
                "client {}x{} at {k}x, clicked client ({}, {}) = screen ({}, {})",
                rc.right, rc.bottom, client.x, client.y, pt.x, pt.y
            )
        }
    }
}

#[test]
fn a_click_on_the_switch_changes_the_language_and_leaves_the_focus_where_it_was() {
    if std::env::var_os("CI").is_none() {
        eprintln!("skipped: it moves the mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("lang-click");
    let p = sb.payload("desktop", &["vault", "list"]).to_string();
    let mut child = sb
        .command(&[s("--run")], &[("FAKE_SLEEP_MS", "8000")])
        .env("OPREASON_PAYLOAD", &p)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn op-reason.exe --run");
    assert!(
        wait_until(Duration::from_secs(10), || click::window().is_some()),
        "the window appears"
    );
    let h = click::window().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(click::title(h), "1Password CLI approval request");
    let before = click::foreground();
    let at = click::click_japanese(h);
    let switched = wait_until(Duration::from_secs(3), || {
        click::title(h) == "1Password CLI の承認の依頼"
    });
    let after = click::foreground();
    // The window writes the file after it has switched (`choose` in src/win/window.rs): the new caption does not mean
    // that the file is there yet, so wait for the file too
    let read = || fs::read_to_string(sb.settings_file()).unwrap_or_default();
    let written = wait_until(Duration::from_secs(5), || {
        read() == r#"{"v":1,"lang":"ja"}"#
    });
    let settings = read();
    let _ = child.wait();
    let st = sb.status();
    assert!(
        switched,
        "the caption switches to Japanese ({at}; caption now {:?})",
        click::title(h)
    );
    assert!(
        written,
        "the choice is written down within 5 s of the switch (settings.json now {settings:?})"
    );
    assert_eq!(
        after, before,
        "the foreground window is the same as before the click ({at})"
    );
    assert_ne!(after, h, "the window did not take the focus");
    assert_eq!(
        st["chosenLang"], "ja",
        "the status reports the choice: {st}"
    );
    assert_eq!(
        st["ui"]["title"], "1Password CLI approval request",
        "ui keeps the texts the call started with: {st}"
    );
}

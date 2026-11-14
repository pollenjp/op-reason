//! Black-box tests of op-reason.exe with the fake op.exe (crates/fake-op). Windows only.
//!
//! Build the fake first: `cargo build -p fake-op` (it lands next to the test binaries' parent, `target\debug\op.exe`;
//! `OPREASON_TEST_FAKE_OP` overrides the path). Desktop-mode tests really show the window on the runner's desktop.
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
        err.contains("Usage: op --reason")
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
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("Usage: op --reason"));
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

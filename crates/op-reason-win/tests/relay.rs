//! The SSH relay end to end (design `docs/superpowers/specs/2026-11-17-ssh-relay-design.md` §11): `op-reason.exe
//! --ssh-agent` between the scripted client and the fake 1Password agent of `crates/op-reason-win/fake-ssh`. Windows
//! only.
//!
//! Build the fake first: `cargo build -p fake-ssh` (`OPREASON_TEST_FAKE_SSH` overrides its path). The relay shows its
//! window on the runner's desktop; the tests that click it with the real mouse run only where `CI` is set.
#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::Value;

/// The relay takes session-wide names (its mutex, op-reason's lock): one test at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

const ASK: &str = "SSH key: allow this call?";
const ASKING: &str = "SSH key: 1Password is asking";
/// The fingerprint of the fake agent's key.
const KEY_FP: &str = "SHA256:ylAg/YojegDgH4I6h4lafZQUAs401hI3a3JtvWR6U+s";

fn exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_op-reason"))
}

fn fake_ssh() -> PathBuf {
    if let Some(p) = std::env::var_os("OPREASON_TEST_FAKE_SSH") {
        return PathBuf::from(p);
    }
    let me = std::env::current_exe().expect("current_exe");
    let p = me
        .parent()
        .and_then(Path::parent)
        .map(|d| d.join("fake-ssh.exe"))
        .expect("target dir");
    assert!(
        p.is_file(),
        "build the fake first: cargo build -p fake-ssh (looked at {})",
        p.display()
    );
    p
}

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

/// A child killed when the test ends.
struct Proc(Child);

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Sandbox {
    root: PathBuf,
    n: usize,
}

impl Sandbox {
    fn new(tag: &str) -> Sandbox {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("op-reason-relay-{tag}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("lad")).unwrap();
        fs::create_dir_all(root.join("agent")).unwrap();
        Sandbox { root, n }
    }

    fn pipe(&self) -> String {
        format!(r"\\.\pipe\op-reason-test-{}-{}", std::process::id(), self.n)
    }

    fn upstream(&self) -> String {
        format!(
            r"\\.\pipe\op-reason-test-up-{}-{}",
            std::process::id(),
            self.n
        )
    }

    fn lad(&self) -> PathBuf {
        self.root.join("lad")
    }

    fn agent_dir(&self) -> PathBuf {
        self.root.join("agent")
    }

    /// Every record of `ssh-log.jsonl`.
    fn records(&self) -> Vec<Value> {
        fs::read_to_string(self.lad().join("op-reason").join("ssh-log.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    fn signs(&self) -> Vec<Value> {
        self.records()
            .into_iter()
            .filter(|r| r["event"] == "ssh-sign")
            .collect()
    }

    /// The messages the fake agent got, in hex.
    fn agent_log(&self) -> Vec<String> {
        fs::read_to_string(self.agent_dir().join("agent.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// How many sign requests reached the agent.
    fn agent_signs(&self) -> usize {
        self.agent_log()
            .iter()
            .filter(|l| l.starts_with("0d"))
            .count()
    }

    fn agent(&self) -> Proc {
        let child = Command::new(fake_ssh())
            .arg("agent")
            .arg(self.upstream())
            .arg(self.agent_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the fake agent");
        let p = Proc(child);
        assert!(
            wait_until(Duration::from_secs(10), || self
                .agent_dir()
                .join("agent.ready")
                .exists()),
            "the fake agent listens"
        );
        p
    }

    fn relay_command(&self, extra: &[&str]) -> Command {
        let mut c = Command::new(exe());
        c.arg("--ssh-agent")
            .arg("--pipe")
            .arg(self.pipe())
            .arg("--upstream")
            .arg(self.upstream())
            .args(extra)
            .env("LOCALAPPDATA", self.lad())
            .env("OPREASON_LANG", "en")
            .env_remove("OPREASON_LOCK_WAIT");
        c
    }

    /// Starts the relay and waits until its pipe answers.
    fn relay(&self, extra: &[&str], envs: &[(&str, &str)]) -> Proc {
        let mut c = self.relay_command(extra);
        for (k, v) in envs {
            c.env(k, v);
        }
        let err = fs::File::create(self.root.join("relay.err")).unwrap();
        let child = c
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(err)
            .spawn()
            .expect("start the relay");
        let p = Proc(child);
        assert!(
            wait_until(Duration::from_secs(10), || self.status().0 == Some(0)),
            "the relay answers on its pipe; stderr: {}",
            fs::read_to_string(self.root.join("relay.err")).unwrap_or_default()
        );
        p
    }

    /// `op-reason.exe --ssh-agent --status` for this pipe: its exit code and stdout.
    fn status(&self) -> (Option<i32>, String) {
        let out = Command::new(exe())
            .args(["--ssh-agent", "--status", "--pipe"])
            .arg(self.pipe())
            .env("OPREASON_LANG", "en")
            .stdin(Stdio::null())
            .output()
            .expect("run --status");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    }

    /// A plain window of another process (`fake-ssh front`) at the left of the screen, to be the window in front (design
    /// `docs/superpowers/specs/2026-11-22-front-after-allow-design.md` §8). With `relay`, that process starts this
    /// sandbox's relay too, and stands for the terminal program the relay was started in.
    fn front(&self, title: &str, relay: bool) -> (Proc, screen::Hwnd) {
        let mut c = Command::new(fake_ssh());
        c.arg("front").arg(title);
        if relay {
            c.arg(exe())
                .arg("--ssh-agent")
                .arg("--pipe")
                .arg(self.pipe())
                .arg("--upstream")
                .arg(self.upstream())
                .env("LOCALAPPDATA", self.lad())
                .env("OPREASON_LANG", "en")
                .env_remove("OPREASON_LOCK_WAIT");
        }
        let child = c
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start fake-ssh front");
        let p = Proc(child);
        assert!(
            wait_until(Duration::from_secs(10), || screen::front_window(title)
                .is_some()),
            "the plain window is up"
        );
        (p, screen::front_window(title).expect("the plain window"))
    }

    fn client(&self, reason: Option<&str>, steps: &[&str]) -> Command {
        let mut c = Command::new(fake_ssh());
        c.arg("client")
            .arg(self.pipe())
            .args(steps)
            .env_remove("OPREASON_SSH_REASON")
            .env_remove("OPREASON_SSH_CALL")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(r) = reason {
            c.env("OPREASON_SSH_REASON", r);
        }
        c
    }
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

mod screen {
    //! The relay's window, found by its class and title; real clicks on its buttons (CI only); the window in front.
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetClassNameW, GetClientRect, GetForegroundWindow, GetWindowTextW,
        IsWindowVisible, SetCursorPos,
    };
    use windows::core::{HSTRING, PCWSTR, w};

    pub type Hwnd = HWND;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra: usize);
    }

    /// The visible window and its title.
    pub fn window() -> Option<(HWND, String)> {
        // SAFETY: plain Win32 queries with valid arguments.
        unsafe {
            let h = FindWindowW(w!("op-reason-approval"), PCWSTR::null()).ok()?;
            if !IsWindowVisible(h).as_bool() {
                return None;
            }
            let mut buf = [0u16; 256];
            let n = GetWindowTextW(h, &mut buf);
            Some((h, String::from_utf16_lossy(&buf[..n.max(0) as usize])))
        }
    }

    pub fn titled(title: &str) -> Option<HWND> {
        window().filter(|(_, t)| t == title).map(|(h, _)| h)
    }

    /// Clicks the first button (`left`) or the last one: 20 DIPs inside the client area's left or right edge and 26
    /// DIPs above its bottom (its 12 DIP padding and half of the 28 DIP row; see `src/win/window.rs`).
    pub fn click_button(h: HWND, left: bool) {
        // SAFETY: Win32 calls with valid arguments; the window belongs to another process of this desktop.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let k = GetDpiForWindow(h).max(96) as f32 / 96.0;
            let inset = (20.0 * k).round() as i32;
            let mut pt = POINT {
                x: if left { inset } else { rc.right - inset },
                y: rc.bottom - (26.0 * k).round() as i32,
            };
            let _ = ClientToScreen(h, &mut pt);
            let _ = SetCursorPos(pt.x, pt.y);
            std::thread::sleep(std::time::Duration::from_millis(100));
            mouse_event(0x0002, 0, 0, 0, 0);
            std::thread::sleep(std::time::Duration::from_millis(60));
            mouse_event(0x0004, 0, 0, 0, 0);
        }
    }

    /// The visible window of `fake-ssh front` titled `title`.
    pub fn front_window(title: &str) -> Option<HWND> {
        // SAFETY: plain Win32 queries with valid arguments.
        unsafe {
            let h = FindWindowW(w!("fake-ssh-front"), &HSTRING::from(title)).ok()?;
            IsWindowVisible(h).as_bool().then_some(h)
        }
    }

    /// Clicks the middle of a window's client area with the real mouse: a window that may activate comes to the front.
    pub fn click_middle(h: HWND) {
        // SAFETY: Win32 calls with valid arguments; the window belongs to another process of this desktop.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let mut pt = POINT {
                x: rc.right / 2,
                y: rc.bottom / 2,
            };
            let _ = ClientToScreen(h, &mut pt);
            let _ = SetCursorPos(pt.x, pt.y);
            std::thread::sleep(std::time::Duration::from_millis(100));
            mouse_event(0x0002, 0, 0, 0, 0);
            std::thread::sleep(std::time::Duration::from_millis(60));
            mouse_event(0x0004, 0, 0, 0, 0);
        }
    }

    pub fn foreground() -> HWND {
        // SAFETY: a plain query.
        unsafe { GetForegroundWindow() }
    }

    /// A window's class and title, for the messages of failed checks.
    pub fn describe(h: HWND) -> String {
        if h.is_invalid() {
            return "no window".to_owned();
        }
        let mut class = [0u16; 256];
        let mut title = [0u16; 256];
        // SAFETY: plain queries into buffers on the stack.
        let (c, t) = unsafe { (GetClassNameW(h, &mut class), GetWindowTextW(h, &mut title)) };
        format!(
            "{} {:?}",
            String::from_utf16_lossy(&class[..c.max(0) as usize]),
            String::from_utf16_lossy(&title[..t.max(0) as usize])
        )
    }
}

fn on_ci() -> bool {
    std::env::var_os("CI").is_some()
}

/// Waits for the asking window and clicks Allow (`true`) or Deny.
fn answer(allow: bool) {
    assert!(
        wait_until(Duration::from_secs(15), || screen::titled(ASK).is_some()),
        "the window asks (now: {:?})",
        screen::window().map(|w| w.1)
    );
    std::thread::sleep(Duration::from_millis(500));
    let h = screen::titled(ASK).expect("the window");
    screen::click_button(h, allow);
}

#[test]
fn requests_pass_unchanged_and_answers_come_back() {
    let _g = serial();
    let sb = Sandbox::new("pass");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[]);
    let sent = sb.root.join("sent.log");
    let out = sb
        .client(None, &["list", "bind", "list"])
        .env("FAKE_SSH_SENT", &sent)
        .output()
        .unwrap();
    assert_eq!(text(&out), "list 12\nbind 5\nlist 12\n", "{out:?}");
    let sent: Vec<String> = fs::read_to_string(&sent)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        sb.agent_log(),
        sent,
        "every message reached the agent unchanged"
    );
    assert!(sb.signs().is_empty(), "no signature, no record of one");
    let starts: Vec<Value> = sb
        .records()
        .into_iter()
        .filter(|r| r["event"] == "ssh-relay" && r["what"] == "start")
        .collect();
    assert_eq!(starts.len(), 1, "{:?}", sb.records());
    assert_eq!(starts[0]["mode"], "per-call");
    assert_eq!(starts[0]["pipe"], sb.pipe().as_str());
}

#[test]
fn a_signature_waits_for_the_window_and_an_unanswered_one_is_not_remembered() {
    let _g = serial();
    let sb = Sandbox::new("timeout");
    let _agent = sb.agent();
    let _relay = sb.relay(&["--answer-wait", "3s"], &[]);
    for round in 0..2 {
        let t0 = Instant::now();
        let child = sb
            .client(Some("check the time-out"), &["list", "sign"])
            .spawn()
            .unwrap();
        assert!(
            wait_until(Duration::from_secs(10), || screen::titled(ASK).is_some()),
            "round {round}: the window asks"
        );
        let out = child.wait_with_output().unwrap();
        assert_eq!(text(&out), "list 12\nsign 5\n", "round {round}: {out:?}");
        assert!(
            t0.elapsed() >= Duration::from_secs(3),
            "it waited for an answer"
        );
    }
    assert_eq!(sb.agent_signs(), 0, "nothing reached the agent");
    let signs = sb.signs();
    assert_eq!(signs.len(), 2, "{signs:?}");
    let s = &signs[0];
    assert_eq!(s["result"], "timeout");
    assert_eq!(s["mode"], "per-call");
    assert_eq!(s["reason"], "check the time-out");
    assert_eq!(s["key"]["fp"], KEY_FP);
    assert_eq!(
        s["key"]["comment"], "test key",
        "the comment from the key list"
    );
    assert_eq!(s["purpose"]["kind"], "userauth");
    assert_eq!(s["purpose"]["user"], "git");
    // the test process starts the client: a program (CI), or the command typed in an interactive shell
    let rule = (
        s["call"]["rule"].as_str().unwrap_or(""),
        s["caller"]["type"].as_str().unwrap_or(""),
    );
    assert!(
        rule == ("parent", "program") || rule == ("shell", "human"),
        "{s}"
    );
    assert!(
        s["client"]["exe"]
            .as_str()
            .is_some_and(|e| e.to_ascii_lowercase().ends_with("fake-ssh.exe")),
        "{s}"
    );
    assert_eq!(signs[1]["result"], "timeout", "the second one asked again");
}

#[test]
fn allow_covers_the_call_and_deny_fails_it_at_once() {
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("allow-deny");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[]);
    let a = sb.client(Some("allow check"), &["sign"]).spawn().unwrap();
    answer(true);
    assert_eq!(text(&a.wait_with_output().unwrap()), "sign 14\n");
    let t0 = Instant::now();
    let b = sb.client(Some("allow check"), &["sign"]).output().unwrap();
    assert_eq!(text(&b), "sign 14\n", "a later request of the call passes");
    assert!(t0.elapsed() < Duration::from_secs(5));
    let c = sb.client(Some("deny check"), &["sign"]).spawn().unwrap();
    answer(false);
    assert_eq!(text(&c.wait_with_output().unwrap()), "sign 5\n");
    let t0 = Instant::now();
    let d = sb.client(Some("deny check"), &["sign"]).output().unwrap();
    assert_eq!(
        text(&d),
        "sign 5\n",
        "a later request of a denied call fails"
    );
    assert!(t0.elapsed() < Duration::from_secs(5), "at once");
    assert!(screen::window().is_none(), "no window is left");
    assert_eq!(
        sb.agent_signs(),
        2,
        "only the allowed call's signatures reached the agent"
    );
    let signs = sb.signs();
    let results: Vec<(&str, &str)> = signs
        .iter()
        .map(|s| {
            (
                s["result"].as_str().unwrap_or(""),
                s["answer"].as_str().unwrap_or("-"),
            )
        })
        .collect();
    assert_eq!(
        results,
        [
            ("allowed", "allow"),
            ("passed", "-"),
            ("denied", "deny"),
            ("denied", "-")
        ]
    );
    assert_eq!(signs[1]["decision"], signs[0]["id"]);
    assert_eq!(signs[3]["decision"], signs[2]["id"]);
}

#[test]
fn two_requests_of_one_call_wait_for_one_answer() {
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("forks");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[]);
    let first = sb.client(Some("fork check"), &["sign"]).spawn().unwrap();
    let second = sb.client(Some("fork check"), &["sign"]).spawn().unwrap();
    std::thread::sleep(Duration::from_secs(2));
    answer(true);
    assert_eq!(text(&first.wait_with_output().unwrap()), "sign 14\n");
    assert_eq!(text(&second.wait_with_output().unwrap()), "sign 14\n");
    let mut results: Vec<String> = sb
        .signs()
        .iter()
        .map(|s| s["result"].as_str().unwrap_or("").to_owned())
        .collect();
    results.sort();
    assert_eq!(results, ["allowed", "passed"], "one window, one answer");
}

#[test]
fn the_time_limit_forgets_the_answer() {
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("limit");
    let _agent = sb.agent();
    let _relay = sb.relay(&["--time-limit", "4s"], &[]);
    let a = sb.client(Some("limit check"), &["sign"]).spawn().unwrap();
    answer(true);
    assert_eq!(text(&a.wait_with_output().unwrap()), "sign 14\n");
    assert_eq!(
        text(&sb.client(Some("limit check"), &["sign"]).output().unwrap()),
        "sign 14\n"
    );
    std::thread::sleep(Duration::from_secs(5));
    let c = sb.client(Some("limit check"), &["sign"]).spawn().unwrap();
    answer(false);
    assert_eq!(
        text(&c.wait_with_output().unwrap()),
        "sign 5\n",
        "it asked again"
    );
}

#[test]
fn without_the_agent_requests_fail_at_once_and_without_a_window() {
    let _g = serial();
    let sb = Sandbox::new("no-agent");
    let _relay = sb.relay(&[], &[]);
    let t0 = Instant::now();
    let out = sb
        .client(Some("no agent"), &["list", "sign"])
        .output()
        .unwrap();
    assert_eq!(text(&out), "list 5\nsign 5\n", "{out:?}");
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert!(screen::window().is_none());
    assert_eq!(sb.signs()[0]["result"], "no-upstream");
}

/// The current user's SID (`whoami /user`).
fn user_sid() -> String {
    let out = Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .expect("whoami");
    let line = String::from_utf8_lossy(&out.stdout).into_owned();
    line.trim()
        .rsplit(',')
        .next()
        .unwrap_or("")
        .trim_matches('"')
        .to_owned()
}

/// The DACL of a pipe as SDDL.
fn pipe_dacl(name: &str) -> String {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
        SE_KERNEL_OBJECT,
    };
    use windows::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};
    use windows::core::PWSTR;
    // a client waits while every instance is busy (the relay creates the next one at once)
    let mut tries = 0;
    let f = loop {
        match fs::OpenOptions::new().read(true).write(true).open(name) {
            Err(e) if e.raw_os_error() == Some(231) && tries < 50 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
            r => break r.expect("open the pipe"),
        }
    };
    // SAFETY: a valid handle; the descriptor and the string are freed with LocalFree.
    unsafe {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        let r = GetSecurityInfo(
            HANDLE(f.as_raw_handle()),
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            Some(&mut sd),
        );
        assert!(r.is_ok(), "GetSecurityInfo: {r:?}");
        let mut text = PWSTR::null();
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            sd,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            None,
        )
        .expect("the SDDL");
        let s = text.to_string().unwrap_or_default();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        let _ = LocalFree(Some(HLOCAL(sd.0)));
        s
    }
}

#[test]
fn the_pipe_is_for_this_user_only_and_the_relay_needs_its_name_free() {
    let _g = serial();
    let sb = Sandbox::new("dacl");
    {
        let _relay = sb.relay(&[], &[]);
        let dacl = pipe_dacl(&sb.pipe());
        let sid = user_sid();
        assert!(sid.starts_with("S-1-"), "{sid}");
        assert_eq!(dacl.matches("(A;").count(), 1, "one entry: {dacl}");
        // SDDL names the built-in Administrator (RID 500, as on the CI runner) by its alias LA
        let named = dacl.contains(&sid) || (sid.ends_with("-500") && dacl.contains(";;;LA)"));
        assert!(named, "{dacl} for {sid}");
        assert!(dacl.starts_with("D:P"), "protected: {dacl}");
    }
    // another server took the name first: the relay does not start
    let other = Command::new(fake_ssh())
        .arg("agent")
        .arg(sb.pipe())
        .arg(sb.root.join("squatter"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _other = Proc(other);
    assert!(wait_until(Duration::from_secs(10), || sb
        .root
        .join("squatter")
        .join("agent.ready")
        .exists()));
    let out = sb.relay_command(&[]).stdin(Stdio::null()).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("cannot serve"), "{err}");
}

#[test]
fn one_relay_per_logon_session_and_status() {
    let _g = serial();
    let sb = Sandbox::new("single");
    let (code, out) = sb.status();
    assert_eq!(code, Some(1), "{out}");
    assert!(out.contains("nothing answers"), "{out}");
    let relay = sb.relay(&[], &[]);
    let (code, out) = sb.status();
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains("answers on") && out.contains(&relay.0.id().to_string()),
        "{out}"
    );
    let other = Sandbox::new("single-2");
    let out = other
        .relay_command(&[])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("another SSH relay runs"), "{err}");
    drop(relay);
    assert!(wait_until(Duration::from_secs(5), || sb.status().0 == Some(1)));
}

#[test]
fn a_window_waits_for_op_reasons_lock() {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
    use windows::core::w;
    let _g = serial();
    let sb = Sandbox::new("lock");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[("OPREASON_LOCK_WAIT", "2")]);
    // SAFETY: a named mutex held by this thread for the test, then released and closed.
    let mutex = unsafe { CreateMutexW(None, false, w!(r"Local\op-reason")).unwrap() };
    assert_eq!(unsafe { WaitForSingleObject(mutex, 5000) }, WAIT_OBJECT_0);
    let t0 = Instant::now();
    let child = sb.client(Some("lock check"), &["sign"]).spawn().unwrap();
    let seen = wait_until(Duration::from_millis(1500), || screen::window().is_some());
    let out = child.wait_with_output().unwrap();
    // SAFETY: as above.
    unsafe {
        let _ = ReleaseMutex(mutex);
        let _ = CloseHandle(mutex);
    }
    assert!(!seen, "no window while op-reason.exe holds the lock");
    assert_eq!(text(&out), "sign 5\n");
    assert!(t0.elapsed() >= Duration::from_secs(2));
    assert_eq!(sb.signs()[0]["result"], "lock-timeout");
}

#[test]
fn a_wsl_call_brings_its_facts_and_a_direct_one_is_alone() {
    let _g = serial();
    let sb = Sandbox::new("wsl");
    let _agent = sb.agent();
    let _relay = sb.relay(&["--answer-wait", "2s"], &[]);
    let wsl = sb.root.join("wsl");
    fs::create_dir_all(&wsl).unwrap();
    fs::copy(fake_ssh(), wsl.join("wsl.exe")).unwrap();
    let call = serde_json::json!({
        "v": 1,
        "distro": "Ubuntu-Test",
        "rule": "shell",
        "root": {"pid": 4242, "start": 123, "name": "git", "cmd": "git push"},
        "shell": {"pid": 4000, "start": 100, "name": "fish", "cmd": ""},
        "caller": {"type": "human", "claude_pid": null,
                   "herdr": {"workspace": "w1", "label": "deploy", "pane": "w1:p2"},
                   "cwd": "~/src/r", "repo": "me/r", "branch": "main"},
        "session": null,
        "reason": "deploy the fix"
    });
    let through = |env: Option<&str>| {
        let mut c = Command::new(wsl.join("wsl.exe"));
        c.arg("spawn")
            .arg(fake_ssh())
            .arg("client")
            .arg(sb.pipe())
            .arg("sign")
            .env_remove("OPREASON_SSH_REASON")
            .env_remove("OPREASON_SSH_CALL")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(v) = env {
            c.env("OPREASON_SSH_CALL", v);
        }
        c.output().unwrap()
    };
    let out = through(Some(&call.to_string()));
    assert_eq!(text(&out), "sign 5\n", "{out:?}");
    let out = through(None);
    assert_eq!(text(&out), "sign 5\n", "{out:?}");
    let signs = sb.signs();
    assert_eq!(signs.len(), 2, "{signs:?}");
    let s = &signs[0];
    assert_eq!(s["result"], "timeout");
    assert_eq!(s["call"]["rule"], "shell");
    assert_eq!(s["call"]["distro"], "Ubuntu-Test");
    assert_eq!(s["call"]["root"]["pid"], 4242);
    assert_eq!(s["call"]["shell"]["name"], "fish");
    assert_eq!(s["caller"]["herdr"]["label"], "deploy");
    assert_eq!(s["caller"]["repo"], "me/r");
    assert_eq!(s["reason"], "deploy the fix");
    let d = &signs[1];
    assert_eq!(
        d["call"]["rule"], "alone",
        "a WSL ssh.exe without op-reason-ssh: {d}"
    );
    assert!(d["caller"].is_null(), "{d}");
}

#[test]
fn show_only_signs_and_says_once_that_1password_did_not_ask() {
    let _g = serial();
    let sb = Sandbox::new("show-only");
    let _agent = sb.agent();
    let _relay = sb.relay(&["--mode", "show-only"], &[]);
    let out = sb
        .client(Some("show check"), &["sign", "sign"])
        .output()
        .unwrap();
    assert_eq!(text(&out), "sign 14\nsign 14\n", "{out:?}");
    let results: Vec<String> = sb
        .signs()
        .iter()
        .map(|s| s["result"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(results, ["signed", "signed"]);
    assert_eq!(sb.signs()[0]["mode"], "show-only");
    let told = sb
        .records()
        .iter()
        .filter(|r| r["event"] == "ssh-relay" && r["what"] == "no-prompt")
        .count();
    assert_eq!(told, 1, "once per run: {:?}", sb.records());
}

#[test]
fn a_forwarded_connection_asks_for_every_signature() {
    let _g = serial();
    let sb = Sandbox::new("forwarded");
    let _agent = sb.agent();
    let _relay = sb.relay(&["--answer-wait", "2s"], &[]);
    let out = sb
        .client(Some("local reason"), &["bind-fwd", "bind", "sign"])
        .output()
        .unwrap();
    assert_eq!(text(&out), "bind-fwd 5\nbind 5\nsign 5\n", "{out:?}");
    let s = &sb.signs()[0];
    assert_eq!(s["result"], "timeout");
    assert_eq!(s["forwarded"], true);
    assert_eq!(s["call"]["rule"], "alone");
    assert!(
        s["host"]["fp"]
            .as_str()
            .is_some_and(|f| f.starts_with("SHA256:")),
        "{s}"
    );
    assert!(
        s["host"]["onward"]
            .as_str()
            .is_some_and(|f| f.starts_with("SHA256:")),
        "{s}"
    );
    assert_eq!(
        s["reason"], "",
        "the local reason is not the remote caller's"
    );
}

#[test]
fn a_slow_answer_gets_the_window_without_buttons() {
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("slow");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[]);
    let a = sb.client(Some("slow check"), &["sign"]).spawn().unwrap();
    answer(true);
    assert_eq!(text(&a.wait_with_output().unwrap()), "sign 14\n");
    fs::write(sb.agent_dir().join("sign-delay-ms"), "3000").unwrap();
    let b = sb.client(Some("slow check"), &["sign"]).spawn().unwrap();
    assert!(
        wait_until(Duration::from_secs(3), || screen::titled(ASKING).is_some()),
        "1Password is slow: the window without buttons (now: {:?})",
        screen::window().map(|w| w.1)
    );
    assert_eq!(text(&b.wait_with_output().unwrap()), "sign 14\n");
    assert!(
        wait_until(Duration::from_secs(3), || screen::window().is_none()),
        "it closes with the answer"
    );
}

/// Sets a named event of another process (the relay's stop event).
fn set_event(name: &str) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{EVENT_MODIFY_STATE, OpenEventW, SetEvent};
    use windows::core::HSTRING;
    // SAFETY: a valid name; the handle is closed here.
    unsafe {
        let h = OpenEventW(EVENT_MODIFY_STATE, false, &HSTRING::from(name))
            .expect("the relay's stop event");
        SetEvent(h).expect("set the event");
        let _ = CloseHandle(h);
    }
}

/// The relay starts with the values of `settings.json` (its options win), tells them in `relay.json`, and its stop
/// event lets the request that is with 1Password finish, fails a new one, and ends it (design
/// `docs/superpowers/specs/2026-11-23-settings-design.md` §8).
#[test]
fn the_relay_reads_its_settings_and_a_stop_lets_the_request_with_1password_finish() {
    let _g = serial();
    let sb = Sandbox::new("settings");
    let dir = sb.lad().join("op-reason");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("settings.json"),
        r#"{"v":1,"relay":{"mode":"show-only","answerWait":7}}"#,
    )
    .unwrap();
    let _agent = sb.agent();
    let mut relay = sb.relay(&["--time-limit", "2m"], &[]);
    let pid = relay.0.id();
    // The relay writes relay.json just after its pipe answers (design §8.2): wait for it
    assert!(
        wait_until(Duration::from_secs(10), || dir.join("relay.json").is_file()),
        "the relay writes relay.json"
    );
    let info: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("relay.json")).unwrap()).unwrap();
    assert_eq!(info["pid"], pid);
    assert_eq!(info["mode"], "show-only", "from settings.json");
    assert_eq!(info["answerWait"], 7, "from settings.json");
    assert_eq!(info["timeLimit"], 120, "the option wins");
    assert_eq!(info["pipe"], sb.pipe());
    assert!(
        sb.records()
            .iter()
            .any(|r| r["event"] == "ssh-relay" && r["what"] == "start" && r["mode"] == "show-only"),
        "{:?}",
        sb.records()
    );
    fs::write(sb.agent_dir().join("sign-delay-ms"), "3000").unwrap();
    let slow = sb.client(Some("stop check"), &["sign"]).spawn().unwrap();
    assert!(
        wait_until(Duration::from_secs(3), || sb.agent_signs() == 1),
        "the request is with 1Password"
    );
    set_event(&format!(r"Local\op-reason-ssh-relay-stop-{pid}"));
    let late = sb
        .client(Some("after the stop"), &["sign"])
        .output()
        .unwrap();
    assert_eq!(
        text(&late),
        "sign 5\n",
        "a new request fails at once: {late:?}"
    );
    assert_eq!(
        text(&slow.wait_with_output().unwrap()),
        "sign 14\n",
        "the request with 1Password finished"
    );
    let t0 = Instant::now();
    let ended = loop {
        if let Ok(Some(status)) = relay.0.try_wait() {
            break Some(status);
        }
        if t0.elapsed() > Duration::from_secs(10) {
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(ended.expect("the relay ends").code(), Some(0));
    assert_eq!(
        sb.agent_signs(),
        1,
        "the late request never reached 1Password"
    );
    assert!(
        sb.records()
            .iter()
            .any(|r| r["event"] == "ssh-relay" && r["what"] == "stop"),
        "{:?}",
        sb.records()
    );
    assert!(!dir.join("relay.json").exists(), "removed at the stop");
}

/// Holds op-reason's lock (`Local\op-reason`) until dropped, as an `op` call or `op-reason --settings` does.
struct HeldLock(windows::Win32::Foundation::HANDLE);

impl HeldLock {
    fn take() -> HeldLock {
        use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
        // SAFETY: a valid name; the handle is released and closed on drop.
        unsafe {
            let h = CreateMutexW(
                None,
                false,
                &windows::core::HSTRING::from(r"Local\op-reason"),
            )
            .unwrap();
            let _ = WaitForSingleObject(h, 5_000);
            HeldLock(h)
        }
    }
}

impl Drop for HeldLock {
    fn drop(&mut self) {
        // SAFETY: the handle was opened and taken by this thread.
        unsafe {
            let _ = windows::Win32::System::Threading::ReleaseMutex(self.0);
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

/// A stop while a request waits for op-reason's lock (its window cannot open while `op-reason --settings` holds it):
/// the request gives up at once, and the relay ends without waiting 25 seconds for it.
#[test]
fn a_stop_ends_a_request_that_waits_for_the_lock() {
    let _g = serial();
    let sb = Sandbox::new("stop-waiting");
    let _agent = sb.agent();
    let mut relay = sb.relay(&[], &[]);
    let pid = relay.0.id();
    let lock = HeldLock::take();
    let waiting = sb
        .client(Some("waits for the lock"), &["sign"])
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let t0 = Instant::now();
    set_event(&format!(r"Local\op-reason-ssh-relay-stop-{pid}"));
    let ended = loop {
        if let Ok(Some(status)) = relay.0.try_wait() {
            break Some(status);
        }
        if t0.elapsed() > Duration::from_secs(20) {
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    drop(lock);
    let out = waiting.wait_with_output().unwrap();
    assert!(
        t0.elapsed() < Duration::from_secs(10),
        "the relay ended in {:?}, not after the 25 seconds of the drain",
        t0.elapsed()
    );
    assert_eq!(ended.expect("the relay ends").code(), Some(0));
    assert_eq!(
        sb.agent_signs(),
        0,
        "the request never reached 1Password: {out:?}"
    );
}

#[test]
fn an_answer_that_never_comes_is_recorded() {
    let _g = serial();
    let sb = Sandbox::new("no-answer");
    let _agent = sb.agent();
    fs::write(sb.agent_dir().join("close-on-sign"), "").unwrap();
    let _relay = sb.relay(&["--mode", "show-only"], &[]);
    let out = sb.client(Some("no answer"), &["sign"]).output().unwrap();
    assert_eq!(text(&out), "sign 5\n", "{out:?}");
    assert_eq!(sb.signs()[0]["result"], "no-answer");
}

#[test]
fn the_relay_says_when_its_parent_is_gone() {
    let _g = serial();
    let sb = Sandbox::new("parent");
    // cmd starts the relay and exits at once
    let status = Command::new("cmd.exe")
        .args(["/d", "/c", "start", "", "/b"])
        .arg(exe())
        .args(["--ssh-agent", "--pipe"])
        .arg(sb.pipe())
        .arg("--upstream")
        .arg(sb.upstream())
        .env("LOCALAPPDATA", sb.lad())
        .env("OPREASON_LANG", "en")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let told = || {
        sb.records().iter().any(|r| {
            r["event"] == "ssh-relay" && (r["what"] == "parent-ended" || r["what"] == "no-parent")
        })
    };
    let ok = wait_until(Duration::from_secs(20), told);
    let pid = sb
        .status()
        .1
        .split("pid ")
        .nth(1)
        .and_then(|t| t.trim_end_matches([')', '\n', '\r']).parse::<u32>().ok());
    // never leave the relay behind: the next test's relay would find it running
    let target = match pid {
        Some(pid) => vec!["/pid".to_owned(), pid.to_string()],
        None => vec!["/im".to_owned(), "op-reason.exe".to_owned()],
    };
    let _ = Command::new("taskkill").args(&target).arg("/f").output();
    assert!(wait_until(Duration::from_secs(10), || sb.status().0 == Some(1)));
    assert!(ok, "the relay noticed that cmd ended: {:?}", sb.records());
}

#[test]
fn a_client_that_goes_away_closes_its_window_and_a_queued_one_never_gets_one() {
    let _g = serial();
    let sb = Sandbox::new("gone");
    let _agent = sb.agent();
    let _relay = sb.relay(&["--answer-wait", "4s"], &[]);
    // the asking client goes away, nobody of its call waits: the window closes
    let mut a = sb.client(Some("gone A"), &["sign"]).spawn().unwrap();
    assert!(wait_until(Duration::from_secs(10), || screen::titled(ASK).is_some()));
    let _ = a.kill();
    let _ = a.wait();
    assert!(
        wait_until(Duration::from_secs(3), || screen::window().is_none()),
        "the window closes when its client is gone"
    );
    // a client that goes away while it waits for its turn never gets a window
    let b = sb.client(Some("gone B"), &["sign"]).spawn().unwrap();
    assert!(wait_until(Duration::from_secs(10), || screen::titled(ASK).is_some()));
    let mut c = sb.client(Some("gone C"), &["sign"]).spawn().unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let _ = c.kill();
    let _ = c.wait();
    assert_eq!(
        text(&b.wait_with_output().unwrap()),
        "sign 5\n",
        "B times out"
    );
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        screen::window().is_none(),
        "no window for the client that left"
    );
    let results: Vec<(String, String)> = sb
        .signs()
        .iter()
        .map(|s| {
            (
                s["reason"].as_str().unwrap_or("").to_owned(),
                s["result"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect();
    for (reason, result) in [
        ("gone A", "gone"),
        ("gone B", "timeout"),
        ("gone C", "gone"),
    ] {
        assert!(
            results.contains(&(reason.to_owned(), result.to_owned())),
            "{reason} {result}: {results:?}"
        );
    }
    assert_eq!(sb.agent_signs(), 0);
}

#[test]
fn an_ssh_exe_connection_without_session_bind_asks_for_every_signature() {
    let _g = serial();
    let sb = Sandbox::new("unbound");
    let _agent = sb.agent();
    let _relay = sb.relay(&["--answer-wait", "2s"], &[]);
    let dir = sb.root.join("openssh");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(fake_ssh(), dir.join("ssh.exe")).unwrap();
    let run = |steps: &[&str]| {
        Command::new(dir.join("ssh.exe"))
            .arg("client")
            .arg(sb.pipe())
            .args(steps)
            .env("OPREASON_SSH_REASON", "unbound check")
            .env_remove("OPREASON_SSH_CALL")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap()
    };
    assert_eq!(text(&run(&["sign"])), "sign 5\n");
    assert_eq!(text(&run(&["bind", "sign"])), "bind 5\nsign 5\n");
    let rules: Vec<String> = sb
        .signs()
        .iter()
        .map(|s| s["call"]["rule"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(rules.len(), 2, "{:?}", sb.signs());
    assert_eq!(
        rules[0], "unbound",
        "an older OpenSSH: every signature asks"
    );
    assert_ne!(rules[1], "unbound", "with session-bind, the call's rule");
}

#[test]
fn closing_the_window_fails_the_requests_that_wait_with_it() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    let _g = serial();
    let sb = Sandbox::new("dismiss");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[]);
    let first = sb.client(Some("close check"), &["sign"]).spawn().unwrap();
    let second = sb.client(Some("close check"), &["sign"]).spawn().unwrap();
    assert!(wait_until(Duration::from_secs(10), || screen::titled(ASK).is_some()));
    std::thread::sleep(Duration::from_secs(1));
    let h = screen::titled(ASK).unwrap();
    // SAFETY: a message to a window of this desktop.
    unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)) }.unwrap();
    assert_eq!(text(&first.wait_with_output().unwrap()), "sign 5\n");
    assert_eq!(text(&second.wait_with_output().unwrap()), "sign 5\n");
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        screen::window().is_none(),
        "no second window for the same call"
    );
    let results: Vec<String> = sb
        .signs()
        .iter()
        .map(|s| s["result"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(results, ["dismissed", "dismissed"]);
}

#[test]
fn an_allowed_call_whose_answer_never_comes_is_recorded() {
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("no-answer-per-call");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[]);
    let a = sb
        .client(Some("per-call no answer"), &["sign"])
        .spawn()
        .unwrap();
    answer(true);
    assert_eq!(text(&a.wait_with_output().unwrap()), "sign 14\n");
    fs::write(sb.agent_dir().join("close-on-sign"), "").unwrap();
    let b = sb
        .client(Some("per-call no answer"), &["sign"])
        .output()
        .unwrap();
    assert_eq!(text(&b), "sign 5\n");
    let last = sb.signs().last().cloned().unwrap();
    assert_eq!(last["result"], "no-answer", "{last}");
    assert!(last["decision"].is_string(), "{last}");
}

/// Stops a relay that a test did not start itself (by the PID that `--status` names), also when the test fails: the next
/// test's relay would find it running.
struct StopRelay<'a>(&'a Sandbox);

impl Drop for StopRelay<'_> {
    fn drop(&mut self) {
        let pid = self
            .0
            .status()
            .1
            .split("pid ")
            .nth(1)
            .and_then(|t| t.trim_end_matches([')', '\n', '\r']).parse::<u32>().ok());
        if let Some(pid) = pid {
            let _ = Command::new("taskkill")
                .args(["/pid", &pid.to_string(), "/f"])
                .output();
        }
        let _ = wait_until(Duration::from_secs(10), || self.0.status().0 == Some(1));
    }
}

#[test]
fn after_allow_the_window_comes_to_the_front_and_gives_it_back() {
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("front");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[]);
    let (_front, fw) = sb.front("op-reason front check", false);
    screen::click_middle(fw);
    assert!(
        wait_until(Duration::from_secs(3), || screen::foreground() == fw),
        "a click brings the plain window to the front (now: {})",
        screen::describe(screen::foreground())
    );
    // Deny: nothing goes to 1Password, and the front stays where it was
    let a = sb.client(Some("front deny"), &["sign"]).spawn().unwrap();
    assert!(wait_until(Duration::from_secs(15), || screen::titled(ASK).is_some()));
    assert_eq!(screen::foreground(), fw, "the window opens behind");
    answer(false);
    assert_eq!(text(&a.wait_with_output().unwrap()), "sign 5\n");
    assert_eq!(
        screen::foreground(),
        fw,
        "Deny leaves the front where it was (now: {})",
        screen::describe(screen::foreground())
    );
    // Allow: the window comes to the front while 1Password (the fake agent, 2.5 seconds) answers, then gives it back
    fs::write(sb.agent_dir().join("sign-delay-ms"), "2500").unwrap();
    let b = sb.client(Some("front allow"), &["sign"]).spawn().unwrap();
    assert!(wait_until(Duration::from_secs(15), || screen::titled(ASK).is_some()));
    assert_eq!(screen::foreground(), fw, "the window opens behind");
    answer(true);
    assert!(
        wait_until(Duration::from_secs(2), || screen::titled(ASKING)
            .is_some_and(|h| screen::foreground() == h)),
        "after Allow, op-reason's window is in front while 1Password answers (now: {})",
        screen::describe(screen::foreground())
    );
    assert_eq!(text(&b.wait_with_output().unwrap()), "sign 14\n");
    assert!(
        wait_until(Duration::from_secs(3), || screen::foreground() == fw),
        "the front goes back to the window that had it (now: {})",
        screen::describe(screen::foreground())
    );
    let results: Vec<String> = sb
        .signs()
        .iter()
        .map(|s| s["result"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(results, ["denied", "allowed"]);
    // the windows appeared at once: the watchdog of the windows said nothing
    let notes: Vec<Value> = sb
        .records()
        .into_iter()
        .filter(|r| {
            r["event"] == "ssh-relay"
                && r["what"].as_str().is_some_and(|w| w.starts_with("window-"))
        })
        .collect();
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn the_window_stays_behind_while_the_program_that_started_the_relay_is_in_front() {
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("front-parent");
    let _agent = sb.agent();
    // the plain window's process starts the relay: the terminal program the relay runs in
    let (_front, fw) = sb.front("op-reason front parent check", true);
    let _stop = StopRelay(&sb);
    assert!(
        wait_until(Duration::from_secs(10), || sb.status().0 == Some(0)),
        "the relay answers on its pipe"
    );
    screen::click_middle(fw);
    assert!(
        wait_until(Duration::from_secs(3), || screen::foreground() == fw),
        "a click brings the plain window to the front (now: {})",
        screen::describe(screen::foreground())
    );
    fs::write(sb.agent_dir().join("sign-delay-ms"), "1500").unwrap();
    let a = sb.client(Some("front parent"), &["sign"]).spawn().unwrap();
    answer(true);
    // 1Password (the fake agent) answers after 1.5 seconds: the window never came to the front meanwhile
    let t0 = Instant::now();
    let mut ahead = None;
    while t0.elapsed() < Duration::from_millis(1200) {
        let fg = screen::foreground();
        if screen::window().is_some_and(|(h, _)| h == fg) {
            ahead = Some(screen::describe(fg));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(ahead, None, "op-reason's window stays behind");
    assert_eq!(text(&a.wait_with_output().unwrap()), "sign 14\n");
    assert_eq!(
        screen::foreground(),
        fw,
        "the program that started the relay keeps the front (now: {})",
        screen::describe(screen::foreground())
    );
    assert_eq!(sb.signs()[0]["result"], "allowed");
}

mod toasts {
    //! The relay's toast window, found by its class (design `docs/superpowers/specs/2026-11-18-pass-toasts-design.md`
    //! §4), and the screen it should be on.
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow,
    };
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        GetWindowTextW, IsWindowVisible, WindowFromPoint,
    };
    use windows::core::{PCWSTR, w};

    /// The visible toast window and its text (the cards' first lines).
    pub fn window() -> Option<(HWND, String)> {
        // SAFETY: plain Win32 queries with valid arguments.
        unsafe {
            let h = FindWindowW(w!("op-reason-pass-toast"), PCWSTR::null()).ok()?;
            if !IsWindowVisible(h).as_bool() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let n = GetWindowTextW(h, &mut buf);
            Some((h, String::from_utf16_lossy(&buf[..n.max(0) as usize])))
        }
    }

    pub fn ex_style(h: HWND) -> u32 {
        // SAFETY: a window of this desktop.
        unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 }
    }

    /// Its rectangle and the window under its first card's middle, in physical pixels.
    pub fn rect_and_below(h: HWND) -> (RECT, HWND) {
        // SAFETY: Win32 queries with valid arguments.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let mut r = RECT::default();
            let _ = GetWindowRect(h, &mut r);
            let k = GetDpiForWindow(h).max(96) as f32 / 96.0;
            // the first card: 12 DIPs in, 300 x 70
            let pt = POINT {
                x: r.left + ((12.0 + 150.0) * k) as i32,
                y: r.top + ((12.0 + 35.0) * k) as i32,
            };
            (r, WindowFromPoint(pt))
        }
    }

    pub fn foreground() -> HWND {
        // SAFETY: a plain query.
        unsafe { GetForegroundWindow() }
    }

    /// The work area of the foreground window's screen (the primary one without a foreground window).
    pub fn work_area() -> RECT {
        // SAFETY: Win32 queries with a valid out parameter.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let m = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(m, &mut info);
            info.rcWork
        }
    }
}

/// The relay's notifications in the notification center (its process's tags in the group `op-reason`), read with the
/// AppUserModelID it borrows; `None` when the history cannot be read.
fn notices(pid: u32) -> Option<usize> {
    use windows::UI::Notifications::ToastNotificationManager;
    use windows::core::HSTRING;
    let aumid = HSTRING::from(
        r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe",
    );
    let list = ToastNotificationManager::History()
        .ok()?
        .GetHistoryWithId(&aumid)
        .ok()?;
    let mine = format!("{pid}-");
    let mut n = 0;
    for i in 0..list.Size().ok()? {
        let t = list.GetAt(i).ok()?;
        let group = t.Group().map(|g| g.to_string()).unwrap_or_default();
        let tag = t.Tag().map(|g| g.to_string()).unwrap_or_default();
        if group == "op-reason" && tag.starts_with(&mine) {
            n += 1;
        }
    }
    Some(n)
}

/// Says in the test's output, also when it passes, what the notification center showed (the harness keeps the output of
/// passing tests to itself; a direct write to stderr goes through).
fn report(line: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{line}");
}

/// Removes the notifications that a test left in the notification center.
fn clear_notices() {
    use windows::UI::Notifications::ToastNotificationManager;
    use windows::core::HSTRING;
    if let Ok(h) = ToastNotificationManager::History() {
        let _ = h.RemoveGroupWithId(
            &HSTRING::from("op-reason"),
            &HSTRING::from(
                r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe",
            ),
        );
    }
}

#[test]
fn a_signature_that_passes_without_a_window_shows_a_toast() {
    use windows::Win32::UI::WindowsAndMessaging::{
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    };
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("toast");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[]);
    let a = sb.client(Some("toast check"), &["sign"]).spawn().unwrap();
    answer(true);
    assert_eq!(text(&a.wait_with_output().unwrap()), "sign 14\n");
    assert!(
        toasts::window().is_none(),
        "no toast for a signature that had a window"
    );
    // that window came to the front after Allow and gave it back as it closed: let the front settle
    std::thread::sleep(Duration::from_millis(300));
    let before = toasts::foreground();
    let b = sb
        .client(Some("toast check"), &["sign", "sign", "sign"])
        .output()
        .unwrap();
    assert_eq!(text(&b), "sign 14\nsign 14\nsign 14\n", "{b:?}");
    assert!(
        wait_until(Duration::from_secs(3), || toasts::window()
            .is_some_and(|(_, t)| t.contains("×3"))),
        "one card for the three signatures of the call (now: {:?})",
        toasts::window().map(|w| w.1)
    );
    let (h, title) = toasts::window().unwrap();
    assert!(title.starts_with("Passed · "), "{title}");
    assert_eq!(title.lines().count(), 1, "{title}");
    let ex = toasts::ex_style(h);
    for (name, flag) in [
        ("layered", WS_EX_LAYERED),
        ("transparent", WS_EX_TRANSPARENT),
        ("no activate", WS_EX_NOACTIVATE),
        ("tool window", WS_EX_TOOLWINDOW),
        ("topmost", WS_EX_TOPMOST),
    ] {
        assert_eq!(ex & flag.0, flag.0, "{name}: {ex:#x}");
    }
    let (r, below) = toasts::rect_and_below(h);
    let work = toasts::work_area();
    report(&format!(
        "toast: {title:?} at ({}, {})-({}, {}), work area ({}, {})-({}, {}), extended style {ex:#x}",
        r.left, r.top, r.right, r.bottom, work.left, work.top, work.right, work.bottom
    ));
    assert_eq!(
        (r.left, r.top),
        (work.left, work.top),
        "at the top left of the work area"
    );
    assert_ne!(below, h, "clicks go through to the window below");
    assert_eq!(toasts::foreground(), before, "the focus stays");
    assert!(
        wait_until(Duration::from_secs(8), || toasts::window().is_none()),
        "gone after four seconds and the fade"
    );
    let results: Vec<String> = sb
        .signs()
        .iter()
        .map(|s| s["result"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(results, ["allowed", "passed", "passed", "passed"]);
}

#[test]
fn a_fourth_call_pushes_a_card_out_and_counts_it() {
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("toast-more");
    let _agent = sb.agent();
    let _relay = sb.relay(&[], &[]);
    let reasons = ["first call", "second call", "third call", "fourth call"];
    for r in reasons {
        let c = sb.client(Some(r), &["sign"]).spawn().unwrap();
        answer(true);
        assert_eq!(text(&c.wait_with_output().unwrap()), "sign 14\n", "{r}");
    }
    // each call signs again, without a window and all at once (well within the four seconds of the first): four
    // cards, one pushed out
    let again: Vec<Child> = reasons
        .iter()
        .map(|r| sb.client(Some(r), &["sign"]).spawn().unwrap())
        .collect();
    for (r, c) in reasons.iter().zip(again) {
        let out = c.wait_with_output().unwrap();
        assert_eq!(text(&out), "sign 14\n", "{r}: {out:?}");
    }
    // the text that showed +1, kept: the four seconds of the card pushed out may end right after
    let seen = std::cell::RefCell::new(None);
    assert!(
        wait_until(Duration::from_secs(3), || {
            let t = toasts::window().map(|w| w.1);
            let more = t.as_ref().is_some_and(|t| t.lines().last() == Some("+1"));
            *seen.borrow_mut() = t;
            more
        }),
        "three cards and +1 (now: {:?})",
        seen.borrow()
    );
    let title = seen.into_inner().unwrap_or_default();
    report(&format!("toasts of four calls: {title:?}"));
    assert_eq!(
        title.lines().count(),
        4,
        "three first lines and +1: {title}"
    );
    assert!(
        wait_until(Duration::from_secs(10), || toasts::window().is_none()),
        "gone after the four seconds and the fade"
    );
}

#[test]
fn the_window_notification_goes_with_the_window() {
    if !on_ci() {
        eprintln!("skipped: it clicks with the real mouse (set CI=1 to run it)");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("notice-goes");
    let _agent = sb.agent();
    let relay = sb.relay(&[], &[]);
    let pid = relay.0.id();
    let child = sb
        .client(Some("notification check"), &["sign"])
        .spawn()
        .unwrap();
    assert!(wait_until(Duration::from_secs(15), || screen::titled(ASK).is_some()));
    let kept = wait_until(Duration::from_secs(5), || {
        notices(pid).is_some_and(|n| n > 0)
    });
    answer(false);
    assert_eq!(text(&child.wait_with_output().unwrap()), "sign 5\n");
    if !kept {
        report(&format!(
            "relay notification: not checked, the notification center keeps no history here (history: {:?})",
            notices(pid)
        ));
        return;
    }
    let gone = wait_until(Duration::from_secs(5), || notices(pid) == Some(0));
    report(&format!(
        "relay notification: in the history while the window was open, after Deny {:?}",
        notices(pid)
    ));
    clear_notices();
    assert!(gone, "removed with its window: {:?}", notices(pid));
}

#[test]
fn a_timed_out_window_leaves_its_notification() {
    let _g = serial();
    let sb = Sandbox::new("notice-stays");
    let _agent = sb.agent();
    let relay = sb.relay(&["--answer-wait", "3s"], &[]);
    let pid = relay.0.id();
    let child = sb
        .client(Some("time-out notification"), &["sign"])
        .spawn()
        .unwrap();
    assert!(wait_until(Duration::from_secs(10), || screen::titled(ASK).is_some()));
    let kept = wait_until(Duration::from_secs(2), || {
        notices(pid).is_some_and(|n| n > 0)
    });
    assert_eq!(text(&child.wait_with_output().unwrap()), "sign 5\n");
    if !kept {
        report(&format!(
            "relay time-out notification: not checked, the notification center keeps no history here (history: {:?})",
            notices(pid)
        ));
        return;
    }
    assert!(wait_until(Duration::from_secs(3), || screen::window().is_none()));
    std::thread::sleep(Duration::from_secs(1));
    let left = notices(pid);
    report(&format!(
        "relay time-out notification: in the history while the window was open, after the time-out {left:?}"
    ));
    clear_notices();
    assert_eq!(left, Some(1), "the sign of a request that went unanswered");
}

// --- The relay's icon in the notification area (design docs/superpowers/specs/2026-11-23-settings-design.md §12)

/// The relay's hidden window that holds its icon.
fn tray_window() -> Option<windows::Win32::Foundation::HWND> {
    use windows::core::{HSTRING, PCWSTR};
    // SAFETY: a static class name.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::FindWindowW(
            &HSTRING::from(op_reason_win::relay_opts::TRAY_CLASS),
            PCWSTR::null(),
        )
    }
    .ok()
    .filter(|h| !h.is_invalid())
}

/// Whether the relay's icon is in the notification area, as its window says (nothing when there is no window).
fn tray_shown() -> Option<bool> {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        RegisterWindowMessageW, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
    };
    let hwnd = tray_window()?;
    let mut result = 0usize;
    // SAFETY: a registered message without pointers; the result outlives the call.
    let answered = unsafe {
        let query = RegisterWindowMessageW(&windows::core::HSTRING::from(
            op_reason_win::relay_opts::TRAY_QUERY,
        ));
        SendMessageTimeoutW(
            hwnd,
            query,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            2_000,
            Some(&mut result),
        )
    };
    (answered.0 != 0).then_some(result == 1)
}

/// `op-reason.exe --settings <args>` with the sandbox's state.
fn settings(sb: &Sandbox, args: &[&str]) -> Output {
    Command::new(exe())
        .arg("--settings")
        .args(args)
        .env("LOCALAPPDATA", sb.lad())
        .env("OPREASON_LANG", "en")
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::null())
        .output()
        .expect("run op-reason.exe --settings")
}

#[test]
fn the_relay_shows_its_icon_and_the_setting_hides_it_at_once() {
    let _g = serial();
    let sb = Sandbox::new("tray");
    let _relay = sb.relay(&[], &[]);
    assert!(
        wait_until(Duration::from_secs(15), || tray_shown() == Some(true)),
        "the relay added its icon: {:?}",
        tray_shown()
    );
    let out = settings(&sb, &["set", "relay.tray-icon", "off"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        wait_until(Duration::from_secs(3), || tray_shown() == Some(false)),
        "set relay.tray-icon off takes the icon away at once"
    );
    let out = settings(&sb, &["set", "relay.tray-icon", "on"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        wait_until(Duration::from_secs(5), || tray_shown() == Some(true)),
        "and on brings it back"
    );
    // The menu's Hide this icon: op-reason --settings in a process of its own, recorded as from the icon
    let hide = op_reason_win::tray_ui::Item::Hide.id();
    // SAFETY: a message without pointers.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            tray_window(),
            windows::Win32::UI::WindowsAndMessaging::WM_COMMAND,
            windows::Win32::Foundation::WPARAM(usize::from(hide)),
            windows::Win32::Foundation::LPARAM(0),
        );
    }
    assert!(
        wait_until(Duration::from_secs(15), || tray_shown() == Some(false)),
        "Hide this icon took it away"
    );
    let log = fs::read_to_string(sb.lad().join("op-reason").join("log.jsonl")).unwrap_or_default();
    assert!(
        log.lines()
            .any(|l| l.contains(r#""event":"settings""#) && l.contains(r#""from":"tray""#)),
        "{log}"
    );
}

#[test]
fn a_relay_started_with_the_icon_off_shows_none() {
    let _g = serial();
    let sb = Sandbox::new("tray-off");
    let dir = sb.lad().join("op-reason");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("settings.json"),
        r#"{"v":1,"relay":{"trayIcon":false}}"#,
    )
    .unwrap();
    let _relay = sb.relay(&[], &[]);
    assert!(
        wait_until(Duration::from_secs(10), || tray_shown().is_some()),
        "the icon's window is there, to be told when the setting changes"
    );
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(tray_shown(), Some(false));
}

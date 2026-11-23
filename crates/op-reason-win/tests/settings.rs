//! Black-box tests of `op-reason.exe --settings` (design `docs/superpowers/specs/2026-11-23-settings-design.md`).
//! Windows only.
//!
//! The skill tests point `USERPROFILE`, `LOCALAPPDATA` and the chosen folders at temporary folders, and WSL at a fake
//! `wsl.exe` (fake-ssh copied, `OPREASON_WSL_EXE`): they run anywhere. The tests of op-reason's scheduled task make a
//! real task (named `op-reason-test-<pid>` through `OPREASON_RELAY_TASK`) whose relay serves the default pipe of the
//! real `LOCALAPPDATA`: they run only where `CI` is set (GitHub's runners), and remove the task at the end.
#![cfg(windows)]

use std::collections::BTreeMap;
use std::ffi::{OsString, c_void};
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::Value;

/// The task tests share the default pipe and the lock; run these tests one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_op-reason"))
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn on_ci() -> bool {
    std::env::var_os("CI").is_some()
}

fn fake_ssh() -> PathBuf {
    let me = std::env::current_exe().expect("current_exe");
    let p = me
        .parent()
        .and_then(Path::parent)
        .map(|d| d.join("fake-ssh.exe"))
        .expect("target dir");
    assert!(
        p.is_file(),
        "build the fakes first: cargo build -p fake-ssh (looked at {})",
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
        std::thread::sleep(Duration::from_millis(100));
    }
    cond()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Temporary folders for the profile, the state and the chosen folders.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Sandbox {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "op-reason-settings-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        for d in ["profile", "lad", "wsl"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        Sandbox { root }
    }

    fn profile(&self) -> PathBuf {
        self.root.join("profile")
    }

    fn skill_file(&self) -> PathBuf {
        self.profile()
            .join(".claude")
            .join("skills")
            .join("op-reason")
            .join("SKILL.md")
    }

    /// The variables that point op-reason at this sandbox: its profile and state, and the fake `wsl.exe`.
    fn vars(&self) -> Vec<(&'static str, OsString)> {
        let wsl = self.root.join("wsl.exe");
        if !wsl.exists() {
            fs::copy(fake_ssh(), &wsl).unwrap();
        }
        vec![
            ("USERPROFILE", self.profile().into()),
            ("APPDATA", self.root.join("appdata").into()),
            ("LOCALAPPDATA", self.root.join("lad").into()),
            ("OPREASON_LANG", "en".into()),
            ("OPREASON_WSL_EXE", wsl.into()),
            ("FAKE_WSL_DIR", self.root.join("wsl").into()),
        ]
    }

    /// `op-reason.exe --settings <args>` with this sandbox's profile and state, and the fake `wsl.exe`.
    fn settings(&self, args: &[&str]) -> Output {
        Command::new(exe())
            .arg("--settings")
            .args(args)
            .envs(self.vars())
            .env_remove("CLAUDECODE")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .stdin(Stdio::null())
            .output()
            .expect("run op-reason.exe --settings")
    }

    fn log(&self) -> String {
        fs::read_to_string(self.root.join("lad").join("op-reason").join("log.jsonl"))
            .unwrap_or_default()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn the_skill_is_placed_checked_and_removed_on_windows() {
    let sb = Sandbox::new("skill");
    let out = sb.settings(&["set", "skill.windows", "on"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let placed = fs::read_to_string(sb.skill_file()).unwrap();
    assert!(placed.starts_with("---\n"), "the front matter stays first");
    assert!(
        placed.contains(&format!("<!-- placed by op-reason {VERSION}:")),
        "{placed}"
    );
    let out = sb.settings(&["check", "skill.windows=on"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = sb.settings(&["get", "skill.windows", "--json"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["settings"][0]["state"], "ok", "{v}");
    let out = sb.settings(&["set", "skill.windows=off"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(!sb.skill_file().exists());
    assert!(
        sb.log().contains(r#""event":"settings""#),
        "every change is recorded: {}",
        sb.log()
    );
}

#[test]
fn a_skill_that_op_reason_did_not_place_is_left_alone() {
    let sb = Sandbox::new("foreign");
    fs::create_dir_all(sb.skill_file().parent().unwrap()).unwrap();
    fs::write(sb.skill_file(), "---\nname: mine\n---\n").unwrap();
    let out = sb.settings(&["set", "skill.windows", "on"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("was not placed by op-reason"),
        "{}",
        text(&out)
    );
    assert_eq!(
        fs::read_to_string(sb.skill_file()).unwrap(),
        "---\nname: mine\n---\n"
    );
    let out = sb.settings(&["check", "skill.windows=off"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "nothing of op-reason's is there: {}",
        text(&out)
    );
}

#[test]
fn a_junction_on_the_way_is_never_written_through() {
    let sb = Sandbox::new("junction");
    let elsewhere = sb.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::create_dir_all(sb.profile().join(".claude")).unwrap();
    let link = sb.profile().join(".claude").join("skills");
    let made = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&elsewhere)
        .output()
        .unwrap();
    assert!(made.status.success(), "mklink /J: {}", text(&made));
    let out = sb.settings(&["set", "skill.windows", "on"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("junction"), "{}", text(&out));
    assert!(
        fs::read_dir(&elsewhere).unwrap().next().is_none(),
        "nothing written through the junction"
    );
    let _ = fs::remove_dir(&link);
}

#[test]
fn a_chosen_folder_is_remembered_and_listed() {
    let sb = Sandbox::new("folder");
    let folder = sb.root.join("agents").join("skills");
    let name = format!("skill.folder.{}", folder.display());
    let out = sb.settings(&["set", &format!("{name}=on")]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(folder.join("op-reason").join("SKILL.md").is_file());
    let settings =
        fs::read_to_string(sb.root.join("lad").join("op-reason").join("settings.json")).unwrap();
    let v: Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(v["skill"]["folders"][0], folder.display().to_string());
    let out = sb.settings(&["list", "--json"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["settings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == name.as_str() && s["value"] == "on"),
        "{v}"
    );
    let out = sb.settings(&["set", &name, "off"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(!folder.join("op-reason").exists());
}

#[test]
fn a_wsl_distribution_goes_through_wsl_exe() {
    let sb = Sandbox::new("wsl");
    let wsl = sb.root.join("wsl");
    fs::write(wsl.join("list"), "Fake\r\ndocker-desktop\r\nStopped\r\n").unwrap();
    fs::write(wsl.join("running"), "Fake\r\n").unwrap();
    let path = "/home/u/.claude/skills/op-reason/SKILL.md";
    fs::write(
        wsl.join("answer-look"),
        format!("op-reason-skill 1 missing - - {path}\n"),
    )
    .unwrap();
    fs::write(
        wsl.join("answer-place"),
        format!("op-reason-skill 1 ok {VERSION} - {path}\n"),
    )
    .unwrap();
    let out = sb.settings(&["list"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let listed = text(&out);
    assert!(listed.contains("skill.wsl.Fake"), "{listed}");
    assert!(listed.contains("skill.wsl.Stopped"), "{listed}");
    assert!(!listed.contains("docker-desktop"), "{listed}");
    let calls = fs::read_to_string(wsl.join("calls.log")).unwrap_or_default();
    assert!(calls.contains("Fake op-reason-skill look"), "{calls}");
    assert!(
        !calls.contains("Stopped"),
        "a stopped distribution is not started: {calls}"
    );
    let out = sb.settings(&["set", "skill.wsl.Fake", "on"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let calls = fs::read_to_string(wsl.join("calls.log")).unwrap();
    assert!(
        calls.contains(&format!("Fake op-reason-skill place {VERSION}")),
        "{calls}"
    );
    let place = calls.lines().find(|l| l.contains(" place ")).unwrap();
    assert!(
        !place.contains("(0 bytes in)"),
        "the file goes on stdin: {calls}"
    );
}

#[test]
fn the_start_menu_shortcut_is_made_checked_and_removed() {
    let sb = Sandbox::new("start-menu");
    let lnk = sb
        .root
        .join("appdata")
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("op-reason.lnk");
    let out = sb.settings(&["set", "start-menu", "on"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(lnk.is_file(), "{}", lnk.display());
    let out = sb.settings(&["check", "start-menu=on", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = sb.settings(&["set", "start-menu", "off"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(!lnk.exists());
    // A file of that name that is not op-reason's shortcut is left alone
    fs::create_dir_all(lnk.parent().unwrap()).unwrap();
    fs::write(&lnk, b"not a shortcut").unwrap();
    let out = sb.settings(&["set", "start-menu", "on"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert_eq!(fs::read(&lnk).unwrap(), b"not a shortcut");
}

// --- op-reason's scheduled task (on CI only)

/// Removes the test's task (and stops its relay) whatever happens.
struct TaskGuard(String);

impl Drop for TaskGuard {
    fn drop(&mut self) {
        let _ = task_settings(&self.0, &["set", "relay.autostart", "off"]);
    }
}

/// `op-reason.exe --settings <args>` with the test's task name and the real profile and state.
fn task_settings(task: &str, args: &[&str]) -> Output {
    Command::new(exe())
        .arg("--settings")
        .args(args)
        .env("OPREASON_RELAY_TASK", task)
        .env("OPREASON_LANG", "en")
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::null())
        .output()
        .expect("run op-reason.exe --settings")
}

fn relay_answers() -> bool {
    Command::new(exe())
        .args(["--ssh-agent", "--status"])
        .stdin(Stdio::null())
        .output()
        .map(|o| o.status.code() == Some(0))
        .unwrap_or(false)
}

/// Holds op-reason's lock (`Local\op-reason`) until dropped.
struct Lock(windows::Win32::Foundation::HANDLE);

impl Lock {
    fn take() -> Lock {
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
            Lock(h)
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // SAFETY: the handle was opened and taken by this thread.
        unsafe {
            let _ = windows::Win32::System::Threading::ReleaseMutex(self.0);
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[test]
fn op_reasons_task_starts_stops_and_goes() {
    if !on_ci() {
        eprintln!(
            "skipped: it makes a scheduled task and runs a relay on the default pipe (set CI=1 to run it)"
        );
        return;
    }
    let _g = serial();
    assert!(!relay_answers(), "no relay may run before this test");
    let task = format!("op-reason-test-{}", std::process::id());
    let _guard = TaskGuard(task.clone());
    let s = |args: &[&str]| task_settings(&task, args);

    let out = s(&["set", "relay.autostart", "on"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(relay_answers(), "the task's relay answers: {}", text(&out));
    let out = s(&["check", "relay.autostart=on", "relay.running=on", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = s(&["get", "relay.running", "--json"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let info = std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("op-reason").join("relay.json"))
        .and_then(|f| fs::read_to_string(f).ok())
        .unwrap_or_default();
    assert_eq!(
        v["settings"][0]["detail"]["ours"], true,
        "{v}; relay.json: {info}"
    );
    assert!(
        info.contains(&format!(r#""task":"{task}""#)),
        "relay.json: {info}"
    );

    // The lock held elsewhere: nothing is stopped
    {
        let _lock = Lock::take();
        let out = s(&["relay", "stop"]);
        assert_eq!(out.status.code(), Some(1), "{}", text(&out));
        assert!(text(&out).contains("window open"), "{}", text(&out));
        assert!(relay_answers());
    }

    let out = s(&["relay", "stop"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(wait_until(Duration::from_secs(5), || !relay_answers()));
    let out = s(&["check", "relay.running=on", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["settings"][0]["state"], "task-off", "{v}");
    std::thread::sleep(Duration::from_secs(70));
    assert!(
        !relay_answers(),
        "a stopped relay stays stopped past the trigger of every minute"
    );

    let out = s(&["relay", "start"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(relay_answers());
    let out = s(&["set", "relay.mode", "show-only"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = s(&["check", "relay.mode=show-only"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the relay was started again: {}",
        text(&out)
    );

    let out = s(&["set", "relay.autostart", "off"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(wait_until(Duration::from_secs(5), || !relay_answers()));
    let out = s(&["get", "relay.autostart"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "off");
    let _ = s(&["set", "relay.mode", "default"]);
}

/// A relay that the test starts itself (outside op-reason's task).
struct Outside(Child);

impl Drop for Outside {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_relay_outside_the_task_is_named_and_never_stopped() {
    if !on_ci() {
        eprintln!(
            "skipped: it runs a relay on the default pipe and makes a scheduled task (set CI=1 to run it)"
        );
        return;
    }
    let _g = serial();
    assert!(!relay_answers(), "no relay may run before this test");
    let task = format!("op-reason-test-outside-{}", std::process::id());
    let _guard = TaskGuard(task.clone());
    let s = |args: &[&str]| task_settings(&task, args);
    let outside = Outside(
        Command::new(exe())
            .arg("--ssh-agent")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(wait_until(Duration::from_secs(10), relay_answers));
    let out = s(&["relay", "stop"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("never stops it"), "{}", text(&out));
    assert!(relay_answers());
    let out = s(&["set", "relay.autostart", "on"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        text(&out).contains("turned off while another relay runs"),
        "{}",
        text(&out)
    );
    let out = s(&["get", "relay.running", "--json"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["settings"][0]["detail"]["ours"], false, "{v}");
    drop(outside);
    assert!(wait_until(Duration::from_secs(5), || !relay_answers()));
    let out = s(&["relay", "start"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(relay_answers());
}

// --- The settings window (on CI only: there is one per logon session, and a desktop may have its own open)

/// The class of the settings window.
const WINDOW_CLASS: &str = "op-reason-settings";

unsafe extern "system" fn each_window(
    hwnd: windows::Win32::Foundation::HWND,
    found: windows::Win32::Foundation::LPARAM,
) -> windows::core::BOOL {
    let mut name = [0u16; 64];
    // SAFETY: `found` points at the vector of `settings_windows` for the length of EnumWindows; the buffer outlives
    // the call.
    unsafe {
        let n = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut name);
        if n > 0 && String::from_utf16_lossy(&name[..n as usize]) == WINDOW_CLASS {
            (*(found.0 as *mut Vec<windows::Win32::Foundation::HWND>)).push(hwnd);
        }
    }
    true.into()
}

/// The top-level windows of the settings window's class.
fn settings_windows() -> Vec<windows::Win32::Foundation::HWND> {
    let mut found: Vec<windows::Win32::Foundation::HWND> = Vec::new();
    // SAFETY: the callback pushes into `found`, which outlives the call.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::EnumWindows(
            Some(each_window),
            windows::Win32::Foundation::LPARAM(
                &mut found as *mut Vec<windows::Win32::Foundation::HWND> as isize,
            ),
        );
    }
    found
}

/// Closes every settings window when a window test ends, and waits for their processes to end.
struct CloseWindows;

impl Drop for CloseWindows {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{CloseHandle, LPARAM, WPARAM};
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
        };
        for hwnd in settings_windows() {
            // SAFETY: plain calls on a window handle, and a process handle closed here.
            unsafe {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let process = OpenProcess(PROCESS_SYNCHRONIZE, false, pid);
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                if let Ok(p) = process {
                    let _ = WaitForSingleObject(p, 30_000);
                    let _ = CloseHandle(p);
                }
            }
        }
    }
}

/// Starts `op-reason.exe` without arguments in a console of its own (hidden), as a double click in Explorer does, with
/// the sandbox's variables. Returns its exit code once it ended (within 10 seconds).
fn start_alone(sb: &Sandbox) -> Option<u32> {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        CREATE_NEW_CONSOLE, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetExitCodeProcess,
        PROCESS_INFORMATION, STARTF_USESHOWWINDOW, STARTUPINFOW, WaitForSingleObject,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    let mut vars: BTreeMap<String, (OsString, OsString)> = std::env::vars_os()
        .map(|(k, v)| (k.to_string_lossy().to_uppercase(), (k, v)))
        .collect();
    for k in ["CLAUDECODE", "CLAUDE_CODE_SESSION_ID"] {
        vars.remove(k);
    }
    for (k, v) in sb.vars() {
        vars.insert(k.to_owned(), (k.into(), v));
    }
    let mut block: Vec<u16> = Vec::new();
    for (k, v) in vars.values() {
        block.extend(k.encode_wide());
        block.push(u16::from(b'='));
        block.extend(v.encode_wide());
        block.push(0);
    }
    block.push(0);
    let app = HSTRING::from(exe().as_os_str());
    let mut line: Vec<u16> = format!("\"{}\"", exe().display())
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESHOWWINDOW,
        wShowWindow: SW_HIDE.0 as u16,
        ..Default::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: the strings, the block and the structures outlive the calls; the handles are closed here.
    unsafe {
        CreateProcessW(
            &app,
            Some(PWSTR(line.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NEW_CONSOLE | CREATE_UNICODE_ENVIRONMENT,
            Some(block.as_ptr() as *const c_void),
            PCWSTR::null(),
            &startup,
            &mut info,
        )
        .ok()?;
        let ended = WaitForSingleObject(info.hProcess, 10_000) == WAIT_OBJECT_0;
        let mut code = 0u32;
        let got = GetExitCodeProcess(info.hProcess, &mut code).is_ok();
        let _ = CloseHandle(info.hThread);
        let _ = CloseHandle(info.hProcess);
        (ended && got).then_some(code)
    }
}

#[test]
fn op_reason_settings_opens_one_window_and_returns_at_once() {
    if !on_ci() {
        eprintln!("skipped: the window tests run where CI is set");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("window");
    assert!(
        settings_windows().is_empty(),
        "no settings window may be open before this test"
    );
    let _close = CloseWindows;
    let t0 = Instant::now();
    let out = sb.settings(&[]);
    let took = t0.elapsed();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        took < Duration::from_secs(2),
        "op-reason --settings returns at once, not when the window closes (took {took:?})"
    );
    assert!(
        wait_until(Duration::from_secs(15), || settings_windows().len() == 1),
        "a window of class {WINDOW_CLASS} appears"
    );
    // Again: the open window comes to the front, and no second one opens
    let out = sb.settings(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    // A second window's process (two launches at the same moment) finds the first window and ends
    let out = sb.settings(&["window"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(settings_windows().len(), 1, "one settings window");
}

#[test]
fn a_double_click_opens_the_window_instead_of_the_usage() {
    if !on_ci() {
        eprintln!("skipped: the window tests run where CI is set");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("double-click");
    assert!(
        settings_windows().is_empty(),
        "no settings window may be open before this test"
    );
    let _close = CloseWindows;
    assert_eq!(
        start_alone(&sb),
        Some(0),
        "op-reason.exe in a console of its own ends at once"
    );
    assert!(
        wait_until(Duration::from_secs(15), || settings_windows().len() == 1),
        "a window of class {WINDOW_CLASS} appears"
    );
}

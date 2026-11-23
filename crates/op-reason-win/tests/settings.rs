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

/// The control `id` of the settings window.
fn control(
    window: windows::Win32::Foundation::HWND,
    id: u16,
) -> Option<windows::Win32::Foundation::HWND> {
    // SAFETY: a plain call on a window handle.
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(Some(window), i32::from(id)) }.ok()
}

fn enabled(hwnd: windows::Win32::Foundation::HWND) -> bool {
    // SAFETY: a plain call on a window handle.
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(hwnd) }.as_bool()
}

fn title(hwnd: windows::Win32::Foundation::HWND) -> String {
    let mut buf = [0u16; 128];
    // SAFETY: the buffer's length is passed.
    let n = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0)])
}

/// Posts a control's notification to the settings window, as a click or a choice of the user does.
fn notify(window: windows::Win32::Foundation::HWND, id: u16, code: u32) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    let ctl = control(window, id).expect("the control");
    // SAFETY: a message without pointers.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(window),
            windows::Win32::UI::WindowsAndMessaging::WM_COMMAND,
            WPARAM(((code as usize) << 16) | usize::from(id)),
            LPARAM(ctl.0 as isize),
        );
    }
}

#[test]
fn the_window_changes_the_settings_with_its_controls() {
    use op_reason_win::settings_ui::ids;
    use windows::Win32::Foundation::WPARAM;
    use windows::Win32::UI::Controls::LVM_GETITEMCOUNT;
    use windows::Win32::UI::WindowsAndMessaging::{
        BN_CLICKED, CB_SETCURSEL, CBN_SELCHANGE, SendMessageW,
    };
    if !on_ci() {
        eprintln!("skipped: the window tests run where CI is set");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("window-controls");
    assert!(
        settings_windows().is_empty(),
        "no settings window may be open before this test"
    );
    let _close = CloseWindows;
    let out = sb.settings(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        wait_until(Duration::from_secs(15), || settings_windows().len() == 1),
        "a window of class {WINDOW_CLASS} appears"
    );
    let window = settings_windows()[0];
    assert_eq!(title(window), "op-reason settings");
    // The window lies within the work area of its screen (the runner's screen is 768 pixels high)
    {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        };
        let mut r = RECT::default();
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: the structures outlive the calls.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(window, &mut r);
            let _ = GetMonitorInfoW(
                MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST),
                &mut info,
            );
        }
        let work = info.rcWork;
        assert!(
            r.top >= work.top
                && r.bottom <= work.bottom
                && r.left >= work.left
                && r.right <= work.right,
            "window {:?} in the work area {:?}",
            (r.left, r.top, r.right, r.bottom),
            (work.left, work.top, work.right, work.bottom)
        );
    }
    for id in [
        ids::LANG,
        ids::RELAY_LINE,
        ids::START,
        ids::STOP,
        ids::RESTART,
        ids::AUTOSTART,
        ids::TRAY_ICON,
        ids::MODE,
        ids::TIME_LIMIT,
        ids::ANSWER_WAIT,
        ids::APPLY,
        ids::SKILLS,
        ids::PLACE,
        ids::REMOVE,
        ids::CHECK,
        ids::ADD_FOLDER,
        ids::START_MENU,
        ids::RECORDS,
        ids::OPEN_RECORDS,
        ids::ACCOUNTS,
        ids::FORGET,
        ids::STATUS,
        ids::REFRESH,
        ids::CLOSE,
    ] {
        assert!(control(window, id).is_some(), "the control {id}");
    }
    let refresh = control(window, ids::REFRESH).unwrap();
    assert!(
        wait_until(Duration::from_secs(60), || enabled(refresh)),
        "the window read the settings (Refresh acts again)"
    );
    // The skill list shows the temporary profile's place, and Place or update places SKILL.md there
    let skills = control(window, ids::SKILLS).unwrap();
    // SAFETY: a message without pointers.
    let rows = unsafe { SendMessageW(skills, LVM_GETITEMCOUNT, None, None) }.0;
    assert!(rows >= 1, "the Windows place at least: {rows}");
    assert!(!sb.skill_file().exists());
    notify(window, ids::PLACE, BN_CLICKED);
    assert!(
        wait_until(Duration::from_secs(30), || sb.skill_file().is_file()),
        "Place or update placed {}",
        sb.skill_file().display()
    );
    assert!(wait_until(Duration::from_secs(60), || enabled(refresh)));
    // Choosing 日本語 writes lang ja, and the window speaks Japanese
    let lang = control(window, ids::LANG).unwrap();
    // SAFETY: a message without pointers.
    unsafe { SendMessageW(lang, CB_SETCURSEL, Some(WPARAM(1)), None) };
    notify(window, ids::LANG, CBN_SELCHANGE);
    let file = sb.root.join("lad").join("op-reason").join("settings.json");
    let lang_is_ja = || {
        fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .is_some_and(|v| v["lang"] == "ja")
    };
    assert!(
        wait_until(Duration::from_secs(30), lang_is_ja),
        "{}",
        fs::read_to_string(&file).unwrap_or_default()
    );
    assert!(
        wait_until(Duration::from_secs(30), || title(window)
            == "op-reason の設定"),
        "{}",
        title(window)
    );
    let log = sb.log();
    assert!(log.contains(r#""from":"window""#), "{log}");
}

/// The value at `path` (`relay.timeLimit`) of the sandbox's `settings.json`.
fn file_value(sb: &Sandbox, path: &[&str]) -> Value {
    let text = fs::read_to_string(sb.root.join("lad").join("op-reason").join("settings.json"))
        .unwrap_or_default();
    let mut v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    for key in path {
        v = v[*key].clone();
    }
    v
}

/// Types `text` into a field of the settings window, as the user does (the field tells the window it changed).
/// `WM_SETTEXT` directly: `SetWindowTextW` does not change a control of another process.
fn type_into(field: windows::Win32::Foundation::HWND, text: &str) {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETTEXT};
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY: Windows carries WM_SETTEXT's string to the other process; it outlives the call.
    unsafe {
        SendMessageW(
            field,
            WM_SETTEXT,
            None,
            Some(LPARAM(wide.as_ptr() as isize)),
        );
    }
}

/// The text of a field of the settings window (`WM_GETTEXT`, which Windows carries back from the other process).
fn field_text(field: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_GETTEXT};
    let mut buf = [0u16; 64];
    // SAFETY: the buffer's length is passed; it outlives the call.
    let n = unsafe {
        SendMessageW(
            field,
            WM_GETTEXT,
            Some(WPARAM(buf.len())),
            Some(LPARAM(buf.as_mut_ptr() as isize)),
        )
    }
    .0;
    String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0).min(buf.len())])
}

#[test]
fn a_typed_time_is_written_before_the_next_action_and_when_the_window_closes() {
    use op_reason_win::settings_ui::ids;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_SETCHECK, BN_CLICKED, PostMessageW, SendMessageW, WM_CLOSE,
    };
    if !on_ci() {
        eprintln!("skipped: the window tests run where CI is set");
        return;
    }
    let _g = serial();
    let sb = Sandbox::new("window-times");
    assert!(
        settings_windows().is_empty(),
        "no settings window may be open before this test"
    );
    let _close = CloseWindows;
    let out = sb.settings(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(wait_until(Duration::from_secs(15), || settings_windows()
        .len()
        == 1));
    let window = settings_windows()[0];
    let refresh = control(window, ids::REFRESH).unwrap();
    assert!(wait_until(Duration::from_secs(60), || enabled(refresh)));
    // 30 minutes typed, and at once (well within the time a field waits) a click on the icon's box
    type_into(control(window, ids::TIME_LIMIT).unwrap(), "30");
    // SAFETY: messages without pointers.
    unsafe {
        SendMessageW(
            control(window, ids::TRAY_ICON).unwrap(),
            BM_SETCHECK,
            Some(WPARAM(0)),
            None,
        );
    }
    notify(window, ids::TRAY_ICON, BN_CLICKED);
    assert!(
        wait_until(Duration::from_secs(30), || file_value(
            &sb,
            &["relay", "timeLimit"]
        ) == 1800
            && file_value(&sb, &["relay", "trayIcon"]) == false),
        "both written: {}",
        file_value(&sb, &[])
    );
    assert!(wait_until(Duration::from_secs(60), || enabled(refresh)));
    assert_eq!(
        field_text(control(window, ids::TIME_LIMIT).unwrap()),
        "30",
        "the field keeps what was typed"
    );
    // 45 seconds typed, and the window closed at once: written before the window's process ends
    type_into(control(window, ids::ANSWER_WAIT).unwrap(), "45");
    let pid = window_pid(window);
    // SAFETY: a message without pointers.
    unsafe {
        let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    assert!(
        wait_until(Duration::from_secs(30), || !process_alive(pid)),
        "the window's process ends"
    );
    assert_eq!(
        file_value(&sb, &["relay", "answerWait"]),
        45,
        "{}",
        file_value(&sb, &[])
    );
}

fn window_pid(hwnd: windows::Win32::Foundation::HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: a plain question about a window handle.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

fn process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    // SAFETY: a handle opened and closed here.
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let alive = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(h);
        alive
    }
}

// --- The icon of op-reason's own task (on CI only, under the task's real name)

/// `op-reason.exe --settings <args>` with the real profile, for op-reason's task under its real name: the relay's icon
/// runs its actions with the relay's variables, which do not name a test's task.
fn real_settings(args: &[&str]) -> Output {
    Command::new(exe())
        .arg("--settings")
        .args(args)
        .env_remove("OPREASON_RELAY_TASK")
        .env("OPREASON_LANG", "en")
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::null())
        .output()
        .expect("run op-reason.exe --settings")
}

/// Removes op-reason's task (its real name) and stops its relay whatever happens.
struct RealTaskGuard;

impl Drop for RealTaskGuard {
    fn drop(&mut self) {
        let _ = real_settings(&["set", "relay.autostart", "off"]);
    }
}

/// The process that `relay.json` of the real profile names.
fn relay_json_pid() -> Option<u64> {
    let info = std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("op-reason").join("relay.json"))
        .and_then(|f| fs::read_to_string(f).ok())?;
    serde_json::from_str::<Value>(&info).ok()?["pid"].as_u64()
}

#[test]
fn the_icon_restarts_the_relay_of_op_reasons_task() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_COMMAND};
    use windows::core::{HSTRING, PCWSTR};
    if !on_ci() {
        eprintln!(
            "skipped: it makes op-reason's scheduled task under its real name (set CI=1 to run it)"
        );
        return;
    }
    let _g = serial();
    assert!(!relay_answers(), "no relay may run before this test");
    let _guard = RealTaskGuard;
    let out = real_settings(&["set", "relay.autostart", "on"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(relay_answers(), "the task's relay answers: {}", text(&out));
    // the relay writes relay.json just after its pipe answers
    assert!(
        wait_until(Duration::from_secs(10), || relay_json_pid().is_some()),
        "relay.json names the task's relay"
    );
    let before = relay_json_pid();
    let tray = || {
        // SAFETY: a static class name.
        unsafe {
            FindWindowW(
                &HSTRING::from(op_reason_win::relay_opts::TRAY_CLASS),
                PCWSTR::null(),
            )
        }
        .ok()
        .filter(|h| !h.is_invalid())
    };
    assert!(
        wait_until(Duration::from_secs(15), || tray().is_some()),
        "the task's relay has its icon's window"
    );
    // Restart the relay, from the icon's menu: op-reason --settings relay restart in a process of its own, out of the
    // task's job, so that the task can start the new relay at once
    let restart = op_reason_win::tray_ui::Item::Restart.id();
    // SAFETY: a message without pointers.
    unsafe {
        let _ = PostMessageW(tray(), WM_COMMAND, WPARAM(usize::from(restart)), LPARAM(0));
    }
    assert!(
        wait_until(Duration::from_secs(45), || {
            relay_json_pid().is_some_and(|p| Some(p) != before) && relay_answers()
        }),
        "a new relay of op-reason's task answers: before {before:?}, now {:?}",
        relay_json_pid()
    );
    let log = std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("op-reason").join("log.jsonl"))
        .and_then(|f| fs::read_to_string(f).ok())
        .unwrap_or_default();
    assert!(
        log.lines().any(|l| l.contains(r#""event":"settings""#)
            && l.contains(r#""from":"tray""#)
            && l.contains(r#""result":"done""#)),
        "the restart is recorded as from the icon: {}",
        log.lines().rev().take(5).collect::<Vec<_>>().join("\n")
    );
}

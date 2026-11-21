//! Stand-ins for the SSH relay's tests (`crates/op-reason-win/tests/relay.rs` and the screenshot script). Never talks to
//! 1Password.
//!
//! - `fake-ssh agent <pipe> <dir>`: a fake 1Password agent on `<pipe>`, until killed. It writes `<dir>/agent.ready` once
//!   it listens and appends each message it gets (hex, one per line) to `<dir>/agent.log`. It answers the key list with
//!   one key (comment `test key`), a sign request with a signature (after the milliseconds in `<dir>/sign-delay-ms`, or
//!   by closing the connection when `<dir>/close-on-sign` exists), an extension with `SSH_AGENT_FAILURE` (as 1Password
//!   does), and anything else with `SSH_AGENT_SUCCESS`.
//! - `fake-ssh client <pipe> <step>…`: one connection, step by step: `list`, `bind` and `bind-fwd`
//!   (`session-bind@openssh.com` without and with the forwarding flag), `sign` or `sign:<user>` (user authentication),
//!   `sshsig`, `sleep:<ms>`, `hold` (waits for stdin to close). It prints `<step> <type of the answer>` per step, or
//!   `<step> closed` and exits 3 when the connection closes. It hexes each message it sends to `FAKE_SSH_SENT` (a file),
//!   when set.
//! - `fake-ssh spawn <program> <args>…`: runs the program and exits with its code; copied as `wsl.exe`, it stands for
//!   the WSL interop host that starts a WSL call's `ssh.exe`.
//! - `fake-ssh front <title> [<program> <args>…]`: a plain window titled `<title>` (class `fake-ssh-front`) at the left
//!   of the screen, until killed: the window in front for the tests of the relay's window coming to the front. With a
//!   program, it starts it once the window is up, so that the program's parent owns the window (it stands for the
//!   terminal program the relay was started in).
//! - With `FAKE_SSH_PIPE` set and any other arguments (copied as `ssh.exe` and started with an ssh command line, such as
//!   `-T git@github.com`, which the relay reads for the command and the destination): `client` on that pipe with the
//!   steps of `FAKE_SSH_STEPS` (separated by spaces).

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

use op_reason_core::ssh::wire;

/// The key the fake agent holds (made for the tests; its private half was thrown away).
const KEY_B64: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJIdrbVVUhN/Pw7374GtdFg/XdjLsCfjFXLmmQn9U8ff";

fn key() -> Vec<u8> {
    op_reason_core::base64::decode(KEY_B64).unwrap_or_default()
}

/// An SSH string.
fn s(b: &[u8]) -> Vec<u8> {
    let mut out = (b.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(b);
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn append(path: &Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

/// The fake agent's answer to one message, or `None` to close the connection.
#[cfg(windows)]
fn answer(body: &[u8], dir: &Path) -> Option<Vec<u8>> {
    Some(match body.first().copied() {
        Some(wire::REQUEST_IDENTITIES) => {
            let mut a = vec![wire::IDENTITIES_ANSWER];
            a.extend_from_slice(&1u32.to_be_bytes());
            a.extend(s(&key()));
            a.extend(s(b"test key"));
            a
        }
        Some(wire::SIGN_REQUEST) => {
            if dir.join("close-on-sign").exists() {
                return None;
            }
            if let Some(ms) = fs::read_to_string(dir.join("sign-delay-ms"))
                .ok()
                .and_then(|t| t.trim().parse::<u64>().ok())
            {
                std::thread::sleep(Duration::from_millis(ms));
            }
            let mut sig = s(b"ssh-ed25519");
            sig.extend(s(&[7u8; 64]));
            let mut a = vec![wire::SIGN_RESPONSE];
            a.extend(s(&sig));
            a
        }
        Some(wire::EXTENSION) => vec![wire::FAILURE],
        _ => vec![wire::SUCCESS],
    })
}

/// The data of a user authentication as `user`.
fn userauth(user: &str) -> Vec<u8> {
    let mut d = s(b"session id");
    d.push(50);
    d.extend(s(user.as_bytes()));
    d.extend(s(b"ssh-connection"));
    d.extend(s(b"publickey"));
    d.push(1);
    d.extend(s(b"ssh-ed25519"));
    d.extend(s(&key()));
    d
}

/// The message of a client step (`None` for steps that send nothing).
fn message(step: &str) -> Option<Vec<u8>> {
    let mut host = s(b"ssh-ed25519");
    host.extend(s(&[9u8; 32]));
    let bind = |fwd: u8| {
        let mut m = vec![wire::EXTENSION];
        m.extend(s(wire::SESSION_BIND.as_bytes()));
        m.extend(s(&host));
        m.extend(s(b"session id"));
        m.extend(s(b"signature"));
        m.push(fwd);
        m
    };
    let sign = |data: Vec<u8>| {
        let mut m = vec![wire::SIGN_REQUEST];
        m.extend(s(&key()));
        m.extend(s(&data));
        m.extend_from_slice(&0u32.to_be_bytes());
        m
    };
    match step {
        "list" => Some(vec![wire::REQUEST_IDENTITIES]),
        "bind" => Some(bind(0)),
        "bind-fwd" => Some(bind(1)),
        "sign" => Some(sign(userauth("git"))),
        "sshsig" => {
            let mut d = b"SSHSIG".to_vec();
            d.extend(s(b"git"));
            d.extend(s(b""));
            d.extend(s(b"sha512"));
            d.extend(s(&[1u8; 64]));
            Some(sign(d))
        }
        _ => step.strip_prefix("sign:").map(|user| sign(userauth(user))),
    }
}

fn client(pipe: &str, steps: &[String]) -> i32 {
    let mut f = match connect(pipe) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("fake-ssh: cannot connect to {pipe}: {e}");
            return 2;
        }
    };
    let sent = std::env::var_os("FAKE_SSH_SENT");
    let mut out = io::stdout().lock();
    for step in steps {
        if let Some(ms) = step.strip_prefix("sleep:") {
            std::thread::sleep(Duration::from_millis(ms.parse().unwrap_or(0)));
            continue;
        }
        if step == "hold" {
            let _ = io::stdin().read_to_end(&mut Vec::new());
            continue;
        }
        let Some(m) = message(step) else {
            eprintln!("fake-ssh: unknown step {step}");
            return 2;
        };
        if let Some(path) = &sent {
            append(Path::new(path), &hex(&m));
        }
        if f.write_all(&wire::frame(&m)).is_err() {
            let _ = writeln!(out, "{step} closed");
            return 3;
        }
        match wire::read_message(&mut f) {
            Ok(Some(reply)) => {
                let _ = writeln!(out, "{step} {}", reply[0]);
                let _ = out.flush();
            }
            _ => {
                let _ = writeln!(out, "{step} closed");
                let _ = out.flush();
                return 3;
            }
        }
    }
    0
}

/// Opens a pipe as a client, waiting while it is busy or not there yet (up to ten seconds).
fn connect(pipe: &str) -> io::Result<fs::File> {
    let mut last = io::Error::other("not tried");
    for _ in 0..100 {
        match OpenOptions::new().read(true).write(true).open(pipe) {
            Ok(f) => return Ok(f),
            Err(e) => last = e,
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(last)
}

fn spawn(args: &[OsString]) -> i32 {
    let Some((program, rest)) = args.split_first() else {
        return 2;
    };
    match std::process::Command::new(program).args(rest).status() {
        Ok(st) => st.code().unwrap_or(1),
        Err(e) => {
            eprintln!("fake-ssh: cannot start {}: {e}", program.to_string_lossy());
            127
        }
    }
}

#[cfg(windows)]
fn agent(pipe: &str, dir: &Path) -> i32 {
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::ERROR_PIPE_CONNECTED;
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows::core::HSTRING;

    let _ = fs::create_dir_all(dir);
    let mut first = true;
    loop {
        let mut open = PIPE_ACCESS_DUPLEX;
        if first {
            open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: a valid name; default security (the current user).
        let h = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(pipe),
                open,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                65536,
                65536,
                0,
                None,
            )
        };
        if h.is_invalid() {
            eprintln!(
                "fake-ssh: cannot create {pipe}: {}",
                io::Error::last_os_error()
            );
            return 2;
        }
        if first {
            let _ = fs::write(dir.join("agent.ready"), std::process::id().to_string());
            first = false;
        }
        // SAFETY: our handle.
        let connected = match unsafe { ConnectNamedPipe(h, None) } {
            Ok(()) => true,
            Err(e) => e.code() == ERROR_PIPE_CONNECTED.to_hresult(),
        };
        // SAFETY: we own the handle and give it to the file.
        let mut f = unsafe { fs::File::from_raw_handle(h.0 as RawHandle) };
        if !connected {
            continue;
        }
        let dir = dir.to_path_buf();
        std::thread::spawn(move || {
            while let Ok(Some(body)) = wire::read_message(&mut f) {
                append(&dir.join("agent.log"), &hex(&body));
                let Some(reply) = answer(&body, &dir) else {
                    return;
                };
                if f.write_all(&wire::frame(&reply)).is_err() {
                    return;
                }
            }
        });
    }
}

#[cfg(not(windows))]
fn agent(_pipe: &str, _dir: &Path) -> i32 {
    eprintln!("fake-ssh: the agent serves a Windows named pipe");
    2
}

#[cfg(windows)]
fn front(title: &str, program: &[OsString]) -> i32 {
    use std::process::{Command, Stdio};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, WHITE_BRUSH};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, PostQuitMessage,
        RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WM_DESTROY, WNDCLASSW,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };
    use windows::core::{HSTRING, w};

    unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        // SAFETY: the window procedure of our own class.
        unsafe {
            if msg == WM_DESTROY {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            DefWindowProcW(h, msg, wp, lp)
        }
    }

    // SAFETY: a class with a static name and a valid procedure, and a window of it; the strings outlive the calls.
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return 2;
        };
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: module.into(),
            lpszClassName: w!("fake-ssh-front"),
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            eprintln!("fake-ssh: cannot register the window class");
            return 2;
        }
        // at the left of the screen, below the toasts (top left) and away from op-reason's window (top right)
        if CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("fake-ssh-front"),
            &HSTRING::from(title),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            40,
            360,
            360,
            200,
            None,
            None,
            Some(module.into()),
            None,
        )
        .is_err()
        {
            eprintln!("fake-ssh: cannot create the window");
            return 2;
        }
    }
    if let Some((p, rest)) = program.split_first()
        && let Err(e) = Command::new(p)
            .args(rest)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    {
        eprintln!("fake-ssh: cannot start {}: {e}", p.to_string_lossy());
        return 127;
    }
    let mut msg = MSG::default();
    // SAFETY: the message loop of this thread, whose window lives until the process is killed.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    0
}

#[cfg(not(windows))]
fn front(_title: &str, _program: &[OsString]) -> i32 {
    eprintln!("fake-ssh: the window is a Windows window");
    2
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let text = |i: usize| {
        args.get(i)
            .map(|a| a.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let code = match text(0).as_str() {
        "agent" => agent(&text(1), Path::new(&text(2))),
        "client" => client(
            &text(1),
            &args
                .iter()
                .skip(2)
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        ),
        "spawn" => spawn(&args[1..]),
        "front" => front(&text(1), args.get(2..).unwrap_or_default()),
        _ => match std::env::var("FAKE_SSH_PIPE") {
            Ok(pipe) if !pipe.is_empty() => {
                let steps = std::env::var("FAKE_SSH_STEPS").unwrap_or_default();
                client(
                    &pipe,
                    &steps
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect::<Vec<_>>(),
                )
            }
            _ => {
                eprintln!(
                    "usage: fake-ssh agent <pipe> <dir> | client <pipe> <step>… | spawn <program> <args>… \
                     | front <title> [<program> <args>…] \
                     (or FAKE_SSH_PIPE and FAKE_SSH_STEPS with an ssh command line)"
                );
                2
            }
        },
    };
    std::process::exit(code);
}

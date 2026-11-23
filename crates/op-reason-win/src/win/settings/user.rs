//! The user who runs the settings: the SID for op-reason's task, whether two users of a task are the same, and the
//! caller of a `settings` record.

use op_reason_core::log::Caller;
use op_reason_core::place;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, LookupAccountNameW, PSID, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PCWSTR, PWSTR};

/// A SID as text (`S-1-5-…`).
///
/// # Safety
/// `sid` points to a valid SID.
unsafe fn sid_text(sid: PSID) -> Option<String> {
    let mut text = PWSTR::null();
    // SAFETY: a valid SID (the caller's promise); the string is freed with LocalFree.
    unsafe {
        ConvertSidToStringSidW(sid, &mut text).ok()?;
        let s = text.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        s
    }
}

/// The SID of the user this process runs as.
pub fn sid() -> Result<String, String> {
    // SAFETY: the token is closed here; the buffer is sized by the first call.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| format!("the user's token: {}", e.message()))?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        let mut buf = vec![0u8; len as usize];
        let got = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            len,
            &mut len,
        );
        let _ = CloseHandle(token);
        got.map_err(|e| format!("the user's token: {}", e.message()))?;
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        sid_text(user.User.Sid).ok_or_else(|| "the user's SID".to_owned())
    }
}

/// A user's SID as text: the text itself when it is a SID, else the name looked up.
fn as_sid(user: &str) -> Option<String> {
    if user.to_ascii_uppercase().starts_with("S-1-") {
        return Some(user.to_owned());
    }
    // SAFETY: buffers sized by the first call; the strings live across the calls.
    unsafe {
        let name = HSTRING::from(user);
        let mut sid_len = 0u32;
        let mut domain_len = 0u32;
        let mut kind = SID_NAME_USE::default();
        let _ = LookupAccountNameW(
            PCWSTR::null(),
            &name,
            None,
            &mut sid_len,
            None,
            &mut domain_len,
            &mut kind,
        );
        if sid_len == 0 {
            return None;
        }
        let mut sid = vec![0u8; sid_len as usize];
        let mut domain = vec![0u16; domain_len.max(1) as usize];
        LookupAccountNameW(
            PCWSTR::null(),
            &name,
            Some(PSID(sid.as_mut_ptr().cast())),
            &mut sid_len,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_len,
            &mut kind,
        )
        .ok()?;
        sid_text(PSID(sid.as_mut_ptr().cast()))
    }
}

/// Whether two users of a task (a SID, `DOMAIN\user` or `user`) are the same user.
pub fn same(a: &str, b: &str) -> bool {
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    match (as_sid(a), as_sid(b)) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(&y),
        _ => {
            let last = |u: &str| u.rsplit('\\').next().unwrap_or(u).to_ascii_lowercase();
            last(a) == last(b)
        }
    }
}

fn env_nonempty(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// The caller of a `settings` record: Claude Code when its variables are set (or the WSL executable said so in
/// `OPREASON_SETTINGS_CALLER`), else a person; the directory, repository and branch.
pub fn caller() -> (Caller, Option<String>) {
    let claude = env_nonempty("CLAUDECODE").as_deref() == Some("1")
        || env_nonempty("CLAUDE_CODE_SESSION_ID").is_some()
        || env_nonempty("OPREASON_SETTINGS_CALLER").as_deref() == Some("claude");
    let cwd = std::env::current_dir().unwrap_or_default();
    let info = place::detect(&cwd);
    (
        Caller {
            kind: if claude { "claude" } else { "human" }.to_owned(),
            claude_pid: if claude {
                env_nonempty("CLAUDE_PID").and_then(|p| p.parse().ok())
            } else {
                None
            },
            herdr: None,
            cwd: cwd.display().to_string(),
            repo: info.repo,
            branch: info.branch,
        },
        env_nonempty("CLAUDE_CODE_SESSION_ID"),
    )
}

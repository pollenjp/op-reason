//! The relay's named pipe and the connection to the 1Password agent's (design §8): instances for the current user only
//! (a DACL), refusing remote clients, the first one created with `FILE_FLAG_FIRST_PIPE_INSTANCE`; byte mode, blocking.

use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
    PIPE_WAIT, PeekNamedPipe, WaitNamedPipeW,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PWSTR};

/// The size of each instance's buffers.
const BUFFER: u32 = 64 * 1024;

/// The current user's SID as text (`S-1-5-21-…`).
fn user_sid() -> io::Result<String> {
    // SAFETY: the token handle is closed below; the buffer outlives GetTokenInformation and the SID string is freed with
    // LocalFree after it is copied.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
        let r = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            (buf.len() * 8) as u32,
            &mut len,
        );
        let _ = CloseHandle(token);
        r?;
        let user = &*(buf.as_ptr().cast::<TOKEN_USER>());
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text)?;
        let sid = text.to_string().unwrap_or_default();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        Ok(sid)
    }
}

/// The security descriptor of the relay's pipe: full access for the current user, nobody else (`D:P(A;;GA;;;<SID>)`).
struct Security(PSECURITY_DESCRIPTOR);

impl Security {
    fn for_user() -> io::Result<Security> {
        let sddl = HSTRING::from(format!("D:P(A;;GA;;;{})", user_sid()?));
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: a valid SDDL string; the descriptor is freed in Drop.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &sddl,
                SDDL_REVISION_1,
                &mut sd,
                None,
            )?;
        }
        Ok(Security(sd))
    }
}

impl Drop for Security {
    fn drop(&mut self) {
        // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0.0)));
        }
    }
}

/// One instance of the relay's pipe, waiting for a client or connected to one.
pub struct Server(HANDLE);

// SAFETY: a pipe handle may be used from any thread; each instance is used by one thread at a time.
unsafe impl Send for Server {}

impl Server {
    /// Creates an instance of `name`; `first` refuses when the name exists already (another relay, or anyone else).
    pub fn create(name: &str, first: bool) -> io::Result<Server> {
        let security = Security::for_user()?;
        let attrs = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security.0.0,
            bInheritHandle: false.into(),
        };
        let mut open = PIPE_ACCESS_DUPLEX;
        if first {
            open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: valid name and attributes, which outlive the call.
        let h = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(name),
                open,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER,
                BUFFER,
                0,
                Some(&attrs),
            )
        };
        if h.is_invalid() {
            return Err(io::Error::last_os_error());
        }
        Ok(Server(h))
    }

    /// Waits for a client.
    pub fn accept(&self) -> io::Result<()> {
        // SAFETY: our handle, synchronous.
        match unsafe { ConnectNamedPipe(self.0, None) } {
            Ok(()) => Ok(()),
            // the client connected between CreateNamedPipeW and ConnectNamedPipe
            Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => Ok(()),
            Err(e) => Err(io::Error::from_raw_os_error(e.code().0 & 0xFFFF)),
        }
    }

    /// The connected client's process.
    pub fn client_pid(&self) -> Option<u32> {
        let mut pid = 0u32;
        // SAFETY: our handle and an out parameter on the stack.
        unsafe { GetNamedPipeClientProcessId(self.0, &mut pid) }
            .ok()
            .filter(|_| pid != 0)
            .map(|_| pid)
    }

    /// The instance as a file to read and write.
    pub fn into_file(self) -> File {
        let h = self.0;
        std::mem::forget(self);
        // SAFETY: we own the handle and hand it to the file, which closes it.
        unsafe { File::from_raw_handle(h.0 as RawHandle) }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SAFETY: we own the handle.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// How many bytes wait to be read on a pipe; `BrokenPipe` when the other end has closed it.
pub fn available(file: &File) -> io::Result<u32> {
    let mut n = 0u32;
    // SAFETY: a pipe handle owned by `file`; only the count is asked for.
    unsafe {
        PeekNamedPipe(
            HANDLE(file.as_raw_handle()),
            None,
            0,
            None,
            Some(&mut n),
            None,
        )
    }
    .map(|_| n)
    .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
}

/// Whether the other end of a pipe has gone.
pub fn gone(file: &File) -> bool {
    available(file).is_err()
}

/// Connects to a pipe as a client (the 1Password agent): busy instances are waited for, up to two seconds, three times.
pub fn connect(name: &str) -> io::Result<File> {
    let mut tries = 0;
    loop {
        match OpenOptions::new().read(true).write(true).open(name) {
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) && tries < 3 => {
                tries += 1;
                // SAFETY: a valid name.
                let _ = unsafe { WaitNamedPipeW(&HSTRING::from(name), 2000) };
            }
            r => return r,
        }
    }
}

/// Whether something serves the pipe: the server's process ID, when it answers within two seconds.
pub fn probe(name: &str) -> Option<u32> {
    let f = connect(name).ok().or_else(|| {
        std::thread::sleep(Duration::from_millis(200));
        connect(name).ok()
    })?;
    let mut pid = 0u32;
    // SAFETY: a pipe handle owned by `f` and an out parameter on the stack.
    unsafe { GetNamedPipeServerProcessId(HANDLE(f.as_raw_handle()), &mut pid) }.ok()?;
    Some(pid)
}

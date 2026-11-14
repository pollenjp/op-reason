//! SIGINT, SIGTERM and SIGHUP, handled on a dedicated thread.
//!
//! The signals are blocked in the calling thread (and so in every thread started later) and a thread waits for them
//! with `sigwait`. The standard library does not reset the signal mask of a child, so every child is started with
//! `launch::command`, which empties the mask between fork and exec; a Ctrl+C or a TERM to the process group still
//! reaches the children, and the handler forwards TERM and HUP to the launcher's relay.

use std::thread;

/// Blocks SIGINT, SIGTERM and SIGHUP here and calls `handler(exit_code)` on a dedicated thread when one arrives. The
/// handler is expected to exit the process. Exit codes: 130 (INT), 143 (TERM), 129 (HUP).
pub fn install(handler: impl Fn(i32) + Send + 'static) {
    // SAFETY: sigset_t is plain data; the libc calls get valid pointers to it.
    let set = unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGHUP);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        set
    };
    thread::spawn(move || {
        loop {
            let mut sig: libc::c_int = 0;
            // SAFETY: valid pointers to a filled set and to an int.
            if unsafe { libc::sigwait(&set, &mut sig) } == 0 {
                handler(128 + sig);
            }
        }
    });
}

/// Whether a file descriptor is a terminal.
pub fn isatty(fd: i32) -> bool {
    // SAFETY: isatty only reads the descriptor number.
    unsafe { libc::isatty(fd) == 1 }
}

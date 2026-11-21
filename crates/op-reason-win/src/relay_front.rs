//! When the SSH relay's window comes to the front (design `2026-11-22-front-after-allow-design.md` §5): after a click on a
//! button that hands the request on to 1Password (Allow this call, Allow this terminal), so that 1Password shows its
//! prompt at once. 1Password keeps the prompt of an application that is not in front behind its icon in the taskbar,
//! and for a relay started from a scheduled task the application it sees is the relay itself.
//!
//! The window stays behind when the window in front belongs to the program that started the relay (a terminal program,
//! for a relay started in one): 1Password takes that program for the application, and sees it in front already. This
//! module decides that from the relay's ancestors; the Windows side reads them and the window in front.

/// A process above the relay: its PID, its creation time (0 when it could not be read) and its image name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ancestor {
    pub pid: u32,
    pub start: u64,
    pub name: String,
}

/// The shell: a program that it starts is an application of its own for 1Password, so the shell being in front (the
/// desktop, the taskbar) never counts as the relay's program.
const SHELL: &str = "explorer.exe";

/// Whether the window in front belongs to the program that started the relay: the process of the window in front
/// (`front_pid`, created at `front_start` when that could be read) is one of the relay's ancestors (nearest first, the
/// relay itself not among them), and not the shell. A process that only reuses an ancestor's PID (another creation
/// time, when both could be read) does not count, nor does no window in front (`front_pid` 0).
pub fn front_is_its_program(
    ancestors: &[Ancestor],
    front_pid: u32,
    front_start: Option<u64>,
) -> bool {
    front_pid != 0
        && ancestors.iter().any(|a| {
            a.pid == front_pid
                && !a.name.eq_ignore_ascii_case(SHELL)
                && (a.start == 0 || front_start.is_none_or(|s| s == a.start))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(pid: u32, start: u64, name: &str) -> Ancestor {
        Ancestor {
            pid,
            start,
            name: name.to_owned(),
        }
    }

    /// A relay started in a terminal program's PowerShell, which explorer started.
    fn in_a_terminal() -> Vec<Ancestor> {
        vec![
            a(200, 20, "pwsh.exe"),
            a(150, 15, "herdr-core.exe"),
            a(100, 10, "herdr-app.exe"),
            a(50, 5, "explorer.exe"),
        ]
    }

    #[test]
    fn the_program_that_started_the_relay_in_front_keeps_the_window_behind() {
        let up = in_a_terminal();
        assert!(
            front_is_its_program(&up, 100, Some(10)),
            "the terminal program"
        );
        assert!(front_is_its_program(&up, 200, Some(20)), "the parent");
        assert!(
            front_is_its_program(&up, 100, None),
            "a creation time that cannot be read: the PID decides"
        );
        assert!(
            front_is_its_program(&[a(100, 0, "herdr-app.exe")], 100, Some(10)),
            "an ancestor whose creation time could not be read"
        );
    }

    #[test]
    fn any_other_window_in_front_lets_the_window_come_forward() {
        let up = in_a_terminal();
        assert!(!front_is_its_program(&up, 300, Some(30)), "another program");
        assert!(!front_is_its_program(&up, 0, None), "no window in front");
        assert!(
            !front_is_its_program(&up, 100, Some(99)),
            "a later process that reuses the terminal program's PID"
        );
        assert!(
            !front_is_its_program(&up, 50, Some(5)),
            "the shell (the desktop, the taskbar)"
        );
        assert!(
            !front_is_its_program(&[a(50, 5, "Explorer.EXE")], 50, Some(5)),
            "the shell, whatever the case of its name"
        );
        // a relay started from a scheduled task: its parent is the Task Scheduler's host in session 0
        let task = [a(900, 0, "svchost.exe"), a(800, 0, "services.exe")];
        assert!(!front_is_its_program(&task, 100, Some(10)));
        assert!(
            !front_is_its_program(&[], 100, Some(10)),
            "a relay whose parent has ended"
        );
    }
}

//! The length check before starting op.exe.
//!
//! The A' entry carries op's arguments in an environment variable as base64 and refuses above 28,000 characters of
//! it. The exe has no payload: the limit is the command line of `CreateProcessW` (32,767 UTF-16 units with the NUL).

use std::ffi::OsStr;

/// The longest command line `CreateProcessW` accepts, without the terminating NUL.
pub const MAX_COMMAND_LINE: usize = 32_766;

/// The length in UTF-16 units of the command line that Rust's `std::process::Command` builds on Windows: the
/// program in quotes, then each argument, quoted when empty or when it has a space or a tab, with backslashes before
/// a quote doubled and the quote escaped (and trailing backslashes doubled inside quotes).
pub fn command_line_len(program: &OsStr, args: &[&OsStr]) -> usize {
    let mut len = 2 + program.to_string_lossy().encode_utf16().count();
    for a in args {
        len += 1 + quoted_len(a);
    }
    len
}

fn quoted_len(arg: &OsStr) -> usize {
    let s = arg.to_string_lossy();
    let quote = s.is_empty() || s.contains([' ', '\t']);
    let mut len = s.encode_utf16().count();
    let mut backslashes = 0;
    for c in s.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                len += backslashes + 1;
                backslashes = 0;
            }
            _ => backslashes = 0,
        }
    }
    if quote {
        len += 2 + backslashes;
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_follow_rusts_quoting() {
        let p = OsStr::new(r"C:\op.exe");
        assert_eq!(command_line_len(p, &[]), 11);
        // ` a`, ` "b c"`, ` ""`, ` x\"y`
        let args = [
            OsStr::new("a"),
            OsStr::new("b c"),
            OsStr::new(""),
            OsStr::new("x\"y"),
        ];
        assert_eq!(command_line_len(p, &args), 11 + 2 + 6 + 3 + 5);
        // `"a b\\"`: a trailing backslash is doubled inside quotes
        assert_eq!(command_line_len(p, &[OsStr::new("a b\\")]), 11 + 1 + 7);
        // `\\\\\"`: two backslashes before a quote become four, plus the escaping one
        assert_eq!(command_line_len(p, &[OsStr::new("\\\\\"")]), 11 + 1 + 6);
    }
}

//! The WSL distributions (design `2026-11-23-settings-design.md` §10.2): the names that `wsl.exe --list --quiet`
//! prints, in UTF-16 or UTF-8.

use crate::settings::catalog;

/// A WSL distribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distro {
    /// Its name.
    pub name: String,
    /// Whether it runs now (`wsl.exe --list --running`): one that does not is not started to be looked at.
    pub running: bool,
}

/// The names in the output of `wsl.exe --list --quiet`: UTF-16LE (with or without a byte order mark) or UTF-8
/// (`WSL_UTF8=1`), one per line. Lines that are not names (a message) and Docker Desktop's distributions (not shells of
/// the user) are left out.
pub fn names(out: &[u8]) -> Vec<String> {
    decode(out)
        .lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0'))
        .filter(|l| {
            catalog::distro_name(l) && !l.to_ascii_lowercase().starts_with("docker-desktop")
        })
        .map(str::to_owned)
        .collect()
}

fn decode(bytes: &[u8]) -> String {
    let zeros = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
    let utf16 =
        bytes.starts_with(&[0xff, 0xfe]) || (bytes.len() >= 2 && zeros * 2 >= bytes.len() / 2);
    if utf16 {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str, bom: bool) -> Vec<u8> {
        let mut out = if bom { vec![0xff, 0xfe] } else { Vec::new() };
        for u in text.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out
    }

    #[test]
    fn names_in_both_encodings() {
        for bom in [false, true] {
            assert_eq!(
                names(&utf16(
                    "Ubuntu-24.04\r\ndocker-desktop\r\nDebian\r\n\r\n",
                    bom
                )),
                ["Ubuntu-24.04", "Debian"]
            );
        }
        assert_eq!(names(b"Debian\nDocker-Desktop-Data\n"), ["Debian"]);
        assert_eq!(names(b""), Vec::<String>::new());
        assert_eq!(
            names(&utf16(
                "Windows Subsystem for Linux has no installed distributions.\r\n",
                false
            )),
            Vec::<String>::new(),
            "a message is not a name"
        );
    }
}

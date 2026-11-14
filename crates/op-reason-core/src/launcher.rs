//! The embedded launcher and the bootstrap that loads it from an environment variable (design section 5.1).

use crate::base64;

/// `launcher/launcher.ps1` as shipped (UTF-8 with a byte order mark).
pub const SOURCE: &[u8] = include_bytes!("../../../launcher/launcher.ps1");

/// The bootstrap run with `-EncodedCommand`: stop progress records, read and remove `OPREASON_SCRIPT`, decode it as
/// UTF-8 and run it in the same scope, so that the launcher's `exit` becomes the exit code of `powershell.exe`.
pub const BOOTSTRAP: &str = "$ProgressPreference = 'SilentlyContinue'
try {
  $s = $env:OPREASON_SCRIPT
  Remove-Item Env:OPREASON_SCRIPT -ErrorAction SilentlyContinue
  $b = [ScriptBlock]::Create([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($s)))
} catch {
  [Console]::Error.WriteLine('op-reason: launcher: cannot load the script: ' + $_.Exception.Message)
  exit 1
}
. $b
";

/// The launcher source as it travels: the byte order mark, comment lines, blank lines and indentation removed (they
/// make up a large part of the file, and an environment variable holds at most 32767 characters). The launcher has no
/// multi-line strings or here-strings (a test keeps it so), so a line that starts with `#` is always a comment and
/// indentation never matters.
pub fn script_text() -> String {
    let text = String::from_utf8_lossy(SOURCE);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        out.push_str(line.trim());
        out.push('\n');
    }
    out
}

/// `OPREASON_SCRIPT`: base64 of the UTF-8 launcher source.
pub fn script_b64() -> String {
    base64::encode(script_text().as_bytes())
}

/// The `-EncodedCommand` argument: base64 of the bootstrap in UTF-16LE.
pub fn encoded_command() -> String {
    let bytes: Vec<u8> = BOOTSTRAP
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    base64::encode(&bytes)
}

/// The arguments of `powershell.exe` before the encoded command.
pub const POWERSHELL_ARGS: [&str; 7] = [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-OutputFormat",
    "Text",
    "-EncodedCommand",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::SCRIPT_B64_MAX;

    #[test]
    fn fits_in_an_environment_variable() {
        let len = script_b64().len();
        assert!(len < SCRIPT_B64_MAX, "OPREASON_SCRIPT is {len} characters");
    }

    #[test]
    fn source_has_a_byte_order_mark_and_no_comment_lines_travel() {
        assert!(SOURCE.starts_with(b"\xef\xbb\xbf"));
        let text = script_text();
        assert!(!text.starts_with('\u{feff}'));
        assert!(text.lines().all(|l| !l.trim_start().starts_with('#')));
        assert!(text.contains("OPREASON_PAYLOAD"));
    }

    #[test]
    fn launcher_has_no_here_strings() {
        // Stripping indentation and comment lines is safe only without multi-line string literals
        let text = String::from_utf8_lossy(SOURCE);
        for line in text.lines() {
            let t = line.trim_end();
            assert!(
                !(t.ends_with("@'") || t.ends_with("@\"")),
                "here-string in the launcher: {line}"
            );
        }
    }

    #[test]
    fn encoded_command_is_the_bootstrap() {
        let bytes = base64::decode(&encoded_command()).unwrap();
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), BOOTSTRAP);
    }
}

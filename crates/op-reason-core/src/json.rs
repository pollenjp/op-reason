//! JSON helpers.

use std::fmt::Write as _;

/// Rewrites serialized JSON so that it contains only ASCII: every non-ASCII character (which can only occur inside
/// strings) becomes `\uXXXX`, with surrogate pairs above U+FFFF. The payload passes through a Windows environment
/// variable, and ASCII avoids every question of how the value is converted on the way.
pub fn to_ascii(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                let _ = write!(out, "\\u{unit:04x}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_non_ascii() {
        let json = serde_json::to_string(&serde_json::json!({"a": "日本😀", "b": "x"})).unwrap();
        let ascii = to_ascii(&json);
        assert!(ascii.is_ascii());
        let bs = '\\';
        let want = format!("\"a\":\"{bs}u65e5{bs}u672c{bs}ud83d{bs}ude00\"");
        assert!(ascii.contains(&want), "{ascii}");
        let back: serde_json::Value = serde_json::from_str(&ascii).unwrap();
        assert_eq!(back["a"], "日本😀");
    }
}

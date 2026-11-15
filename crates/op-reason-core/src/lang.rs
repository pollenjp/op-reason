//! The language of messages, the window and the log viewer.

/// English or Japanese.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// English (the default).
    En,
    /// Japanese.
    Ja,
}

impl Lang {
    /// `en` or `ja` (the payload's `lang`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ja => "ja",
        }
    }
}

fn from_text(text: &str) -> Lang {
    if text.to_ascii_lowercase().starts_with("ja") {
        Lang::Ja
    } else {
        Lang::En
    }
}

/// `OPREASON_LANG` when set (`ja…` → Japanese, anything else → English); otherwise the first non-empty of
/// `LC_ALL`, `LC_MESSAGES` and `LANG` (`ja…` → Japanese); otherwise English.
pub fn resolve(env: &dyn Fn(&str) -> Option<String>) -> Lang {
    if let Some(v) = env("OPREASON_LANG").filter(|v| !v.is_empty()) {
        return from_text(&v);
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(v) = env(key).filter(|v| !v.is_empty()) {
            return from_text(&v);
        }
    }
    Lang::En
}

/// [`resolve`] from the process environment.
pub fn from_env() -> Lang {
    resolve(&|k| std::env::var(k).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn with(pairs: &[(&str, &str)]) -> Lang {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        resolve(&|k| map.get(k).cloned())
    }

    #[test]
    fn resolves() {
        assert_eq!(with(&[]), Lang::En);
        assert_eq!(with(&[("OPREASON_LANG", "ja")]), Lang::Ja);
        assert_eq!(
            with(&[("OPREASON_LANG", "en"), ("LANG", "ja_JP.UTF-8")]),
            Lang::En
        );
        assert_eq!(with(&[("LANG", "ja_JP.UTF-8")]), Lang::Ja);
        assert_eq!(with(&[("LC_ALL", "C"), ("LANG", "ja_JP.UTF-8")]), Lang::En);
        assert_eq!(with(&[("LC_ALL", ""), ("LANG", "ja_JP.UTF-8")]), Lang::Ja);
        assert_eq!(
            with(&[("OPREASON_LANG", ""), ("LANG", "C.UTF-8")]),
            Lang::En
        );
    }
}

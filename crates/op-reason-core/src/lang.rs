//! The language of messages, the window and the log viewer.
//!
//! English is the default. The language chosen in the window of `op-reason.exe` (remembered in
//! [`crate::settings`]) comes first, then `OPREASON_LANG`; `LANG` and the other locale variables do not count
//! (design `2026-11-15-language-toggle-design.md` §2).

/// English or Japanese.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// English (the default).
    En,
    /// Japanese.
    Ja,
}

impl Lang {
    /// `en` or `ja` (the payload's `lang`, `settings.json`'s `lang`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ja => "ja",
        }
    }

    /// The payload's `lang` (or any language text): `ja…` → Japanese, anything else → English.
    pub fn from_code(code: &str) -> Lang {
        from_text(code)
    }

    /// Exactly `en` or `ja` (a remembered choice), anything else → `None`.
    pub fn parse(code: &str) -> Option<Lang> {
        match code {
            "en" => Some(Lang::En),
            "ja" => Some(Lang::Ja),
            _ => None,
        }
    }

    /// The other language (what the window's switch offers).
    pub fn other(self) -> Lang {
        match self {
            Lang::En => Lang::Ja,
            Lang::Ja => Lang::En,
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

/// The language chosen in the window when there is one; otherwise `OPREASON_LANG` when it is set and not empty
/// (`ja…` → Japanese, anything else → English); otherwise English.
pub fn resolve(chosen: Option<Lang>, env: &dyn Fn(&str) -> Option<String>) -> Lang {
    if let Some(lang) = chosen {
        return lang;
    }
    match env("OPREASON_LANG").filter(|v| !v.is_empty()) {
        Some(v) => from_text(&v),
        None => Lang::En,
    }
}

/// [`resolve`] with the process environment.
pub fn pick(chosen: Option<Lang>) -> Lang {
    resolve(chosen, &|k| std::env::var(k).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn with(chosen: Option<Lang>, pairs: &[(&str, &str)]) -> Lang {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        resolve(chosen, &|k| map.get(k).cloned())
    }

    #[test]
    fn english_unless_opreason_lang_says_otherwise() {
        assert_eq!(with(None, &[]), Lang::En);
        assert_eq!(with(None, &[("OPREASON_LANG", "ja")]), Lang::Ja);
        assert_eq!(with(None, &[("OPREASON_LANG", "ja_JP.UTF-8")]), Lang::Ja);
        assert_eq!(with(None, &[("OPREASON_LANG", "en")]), Lang::En);
        assert_eq!(with(None, &[("OPREASON_LANG", "fr")]), Lang::En);
        assert_eq!(with(None, &[("OPREASON_LANG", "")]), Lang::En);
    }

    #[test]
    fn the_locale_no_longer_chooses_japanese() {
        for key in ["LANG", "LC_ALL", "LC_MESSAGES"] {
            assert_eq!(with(None, &[(key, "ja_JP.UTF-8")]), Lang::En, "{key}");
        }
        assert_eq!(
            with(None, &[("OPREASON_LANG", "en"), ("LANG", "ja_JP.UTF-8")]),
            Lang::En
        );
    }

    #[test]
    fn the_choice_in_the_window_beats_opreason_lang() {
        assert_eq!(with(Some(Lang::Ja), &[]), Lang::Ja);
        assert_eq!(with(Some(Lang::En), &[("OPREASON_LANG", "ja")]), Lang::En);
        assert_eq!(with(Some(Lang::Ja), &[("OPREASON_LANG", "en")]), Lang::Ja);
    }

    #[test]
    fn codes() {
        assert_eq!(Lang::parse("en"), Some(Lang::En));
        assert_eq!(Lang::parse("ja"), Some(Lang::Ja));
        for bad in ["", "EN", "ja_JP", "fr", " ja"] {
            assert_eq!(Lang::parse(bad), None, "{bad:?}");
        }
        assert_eq!(Lang::from_code("ja-JP"), Lang::Ja);
        assert_eq!(Lang::from_code("de"), Lang::En);
        assert_eq!(Lang::En.other(), Lang::Ja);
        assert_eq!(Lang::Ja.other().code(), "en");
    }
}

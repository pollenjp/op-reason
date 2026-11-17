//! The account copy (design `2026-11-17-refs-in-window-design.md` §5): the account ID that the end-of-call
//! `op whoami --format json` reports, kept in `%LOCALAPPDATA%\op-reason\account.json`, so that later windows can turn an
//! ID reference into `onepassword://view-item?a=…&v=…&i=…`. The launcher keeps the same file by the same rules.
//!
//! The file: `{"v":1,"accounts":[{"id":"…","host":"…"}]}`, the account seen last first, at most [`KEEP`]. It holds
//! account IDs and sign-in hosts only: the email address and the user ID of the whoami output are never kept.

use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

/// The file, in `%LOCALAPPDATA%\op-reason`.
pub const ACCOUNT_FILE: &str = "account.json";
/// The version of the format.
pub const VERSION: u64 = 1;
/// How many accounts the file keeps.
pub const KEEP: usize = 5;

/// One remembered account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The account ID (26 letters and digits).
    pub id: String,
    /// The sign-in host (`example.1password.com`), or empty.
    pub host: String,
}

/// Whether `s` can be an account ID: 26 letters and digits.
pub fn is_account_id(s: &str) -> bool {
    s.len() == 26 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The host of a sign-in address (`https://Example.1password.com/` → `example.1password.com`); empty when what is left
/// is not a host name (letters, digits, `.` and `-`).
pub fn host_of(url: &str) -> String {
    let s = url.trim();
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    let s = s.split(['/', '?', '#']).next().unwrap_or("");
    let s = s.split(':').next().unwrap_or("");
    let host = s.to_ascii_lowercase();
    let ok = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-');
    if ok { host } else { String::new() }
}

/// The account in the output of `op whoami --format json`: `account_uuid` (26 letters and digits) and the host of
/// `url`. Nothing else is read.
pub fn from_whoami(json: &str) -> Option<Account> {
    let v: Value = serde_json::from_str(json.trim_start_matches('\u{feff}')).ok()?;
    let id = v.get("account_uuid")?.as_str()?;
    if !is_account_id(id) {
        return None;
    }
    let host = v
        .get("url")
        .and_then(Value::as_str)
        .map(host_of)
        .unwrap_or_default();
    Some(Account {
        id: id.to_string(),
        host,
    })
}

/// The accounts in the file's text, in order; entries without a usable ID are skipped, and a broken file gives none.
pub fn parse(text: &str) -> Vec<Account> {
    let Ok(v) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
        return Vec::new();
    };
    let Some(list) = v.get("accounts").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|a| {
            let id = a.get("id")?.as_str()?;
            is_account_id(id).then(|| Account {
                id: id.to_string(),
                host: a
                    .get("host")
                    .and_then(Value::as_str)
                    .map(host_of)
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// The file's text: `seen` first, then the others without another entry of the same ID, at most [`KEEP`].
pub fn render(old: &[Account], seen: &Account) -> String {
    let mut list = vec![seen];
    list.extend(old.iter().filter(|a| !a.id.eq_ignore_ascii_case(&seen.id)));
    let entries: Vec<String> = list
        .iter()
        .take(KEEP)
        .map(|a| {
            format!(
                r#"{{"id":{},"host":{}}}"#,
                Value::from(a.id.as_str()),
                Value::from(a.host.as_str())
            )
        })
        .collect();
    format!(r#"{{"v":{VERSION},"accounts":[{}]}}"#, entries.join(","))
}

/// The remembered accounts; none when the file is missing or broken.
pub fn read(path: &Path) -> Vec<Account> {
    fs::read(path)
        .map(|b| parse(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
}

/// Puts `seen` first in the file (a temporary file renamed over it, so that a reader never sees half a file).
pub fn remember(path: &Path, seen: &Account) -> io::Result<()> {
    op_reason_core::settings::replace(path, &render(&read(path), seen))
}

/// The account for the links of a call. With `want` (the call's `--account`): the first whose ID, host or host's first
/// label (the shorthand op gives an account by default) equals it, ignoring case, `https://` and a trailing `/`; none
/// when no account matches. Without: the first, the one the last whoami reported.
pub fn pick<'a>(accounts: &'a [Account], want: Option<&str>) -> Option<&'a Account> {
    let Some(want) = want.map(str::trim).filter(|w| !w.is_empty()) else {
        return accounts.first();
    };
    let want = want.to_ascii_lowercase();
    let want = want.strip_prefix("https://").unwrap_or(&want);
    let want = want.trim_end_matches('/');
    accounts.iter().find(|a| {
        a.id.eq_ignore_ascii_case(want)
            || (!a.host.is_empty() && (a.host == want || a.host.split('.').next() == Some(want)))
    })
}

/// The URL that opens an item in the 1Password app, built from the three IDs only: `None` unless the account is 26
/// letters and digits and the vault and the item are 26 lowercase letters and digits.
pub fn item_url(account: &str, vault: &str, item: &str) -> Option<String> {
    let id = |s: &str| {
        s.len() == 26
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    };
    (is_account_id(account) && id(vault) && id(item))
        .then(|| format!("onepassword://view-item?a={account}&v={vault}&i={item}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const B: &str = "ZYXWVUTSRQPONMLKJIHGFEDCBA";
    const V: &str = "abcdefghijklmnopqrstuvwxyz";
    const I: &str = "0123456789abcdefghijklmnop";

    fn acct(id: &str, host: &str) -> Account {
        Account {
            id: id.into(),
            host: host.into(),
        }
    }

    #[test]
    fn whoami_gives_the_id_and_the_host_only() {
        let out = format!(
            r#"{{"url":"https://Example.1password.com/","email":"someone@example.com","user_uuid":"USERUSERUSERUSERUSERUSER12","account_uuid":"{A}","user_type":"HUMAN"}}"#
        );
        assert_eq!(from_whoami(&out), Some(acct(A, "example.1password.com")));
        let text = render(&[], &from_whoami(&out).unwrap());
        assert_eq!(
            text,
            format!(r#"{{"v":1,"accounts":[{{"id":"{A}","host":"example.1password.com"}}]}}"#)
        );
        assert!(!text.contains("someone") && !text.contains("USER"));
        assert_eq!(
            from_whoami(&format!(
                r#"{{"account_uuid":"{A}","url":"my.1password.eu"}}"#
            )),
            Some(acct(A, "my.1password.eu"))
        );
        assert_eq!(
            from_whoami(&format!(r#"{{"account_uuid":"{A}"}}"#)),
            Some(acct(A, ""))
        );
        assert_eq!(
            from_whoami(&format!(
                r#"{{"account_uuid":"{A}","url":"https://ex ample/"}}"#
            )),
            Some(acct(A, "")),
            "not a host"
        );
        assert_eq!(from_whoami(r#"{"account_uuid":"SHORT","url":"x"}"#), None);
        assert_eq!(from_whoami("account is not signed in"), None);
    }

    #[test]
    fn the_file_keeps_the_last_seen_first_and_five_at_most() {
        let old: Vec<Account> = (0..6)
            .map(|k| acct(&format!("{}{k}", &B[..25]), "h.example.com"))
            .collect();
        let text = render(&old, &acct(&format!("{}3", &B[..25]), "new.example.com"));
        let back = parse(&text);
        assert_eq!(back.len(), KEEP);
        assert_eq!(back[0], acct(&format!("{}3", &B[..25]), "new.example.com"));
        assert!(back.iter().filter(|a| a.id.ends_with('3')).count() == 1);
        assert_eq!(parse("not json"), Vec::new());
        assert_eq!(
            parse(&format!(
                r#"{{"v":9,"accounts":[{{"id":"bad"}},{{"id":"{A}","host":"Bad Host"}},{{"id":"{B}"}}],"later":1}}"#
            )),
            vec![acct(A, ""), acct(B, "")]
        );
    }

    #[test]
    fn remember_writes_and_read_reads() {
        let dir = std::env::temp_dir().join(format!("op-reason-account-{}", std::process::id()));
        let file = dir.join("sub").join(ACCOUNT_FILE);
        assert_eq!(read(&file), Vec::new());
        remember(&file, &acct(A, "a.example.com")).unwrap();
        remember(&file, &acct(B, "b.example.com")).unwrap();
        assert_eq!(
            read(&file),
            vec![acct(B, "b.example.com"), acct(A, "a.example.com")]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pick_by_account_id_host_or_shorthand() {
        let list = vec![acct(A, "my.1password.com"), acct(B, "work.1password.eu")];
        assert_eq!(pick(&list, None), Some(&list[0]));
        assert_eq!(pick(&list, Some(" ")), Some(&list[0]));
        assert_eq!(pick(&list, Some(&B.to_ascii_lowercase())), Some(&list[1]));
        assert_eq!(pick(&list, Some("Work.1Password.EU")), Some(&list[1]));
        assert_eq!(
            pick(&list, Some("https://work.1password.eu/")),
            Some(&list[1])
        );
        assert_eq!(pick(&list, Some("work")), Some(&list[1]));
        assert_eq!(pick(&list, Some("other")), None);
        assert_eq!(pick(&[], None), None);
        assert_eq!(pick(&[acct(A, "")], Some("")), Some(&acct(A, "")));
    }

    #[test]
    fn item_urls_are_built_from_ids_only() {
        assert_eq!(
            item_url(A, V, I).as_deref(),
            Some(
                "onepassword://view-item?a=ABCDEFGHIJKLMNOPQRSTUVWXYZ&v=abcdefghijklmnopqrstuvwxyz&i=0123456789abcdefghijklmnop"
            )
        );
        assert_eq!(item_url("A&b=c", V, I), None);
        assert_eq!(item_url(A, &V.to_ascii_uppercase(), I), None);
        assert_eq!(item_url(A, V, &I[..25]), None);
        assert_eq!(item_url(A, V, "0123456789abcdefghijklm/op"), None);
    }
}

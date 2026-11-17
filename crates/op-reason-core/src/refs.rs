//! The `op://` references a call reads (design `2026-11-17-refs-in-window-design.md` §2): the arguments that are
//! references, the template of `op inject` (its `-i` file, else stdin) and the env files of `op run`. The window lists
//! them and links those whose vault and item are IDs.
//!
//! The PowerShell entry implements the same rules (without stdin); `tests/cases/refs.json` keeps the two in step.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use crate::args::clip;
use crate::contract::{RefSource, Refs};

/// References listed in the window (the payload's `list`).
pub const SHOWN: usize = 8;
/// References kept in the log's `start` record.
pub const LOGGED: usize = 1000;
/// The most characters of one reference in the payload.
pub const SHOWN_CHARS: usize = 200;
/// The most characters of one reference in the log.
pub const LOGGED_CHARS: usize = 300;
/// The most characters of a file name in `from`.
pub const NAME_CHARS: usize = 40;
/// The most characters of the `--account` value.
pub const ACCOUNT_CHARS: usize = 100;
/// How much of a template or env file is read.
pub const FILE_MAX: u64 = 8 * 1024 * 1024;

const PREFIX: &str = "op://";

/// What the arguments say about the sources.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The subcommand (`inject`, `run`, `read`, …), when there is one.
    pub subcommand: Option<String>,
    /// `op inject -i <file>`: the template file.
    pub template: Option<String>,
    /// `op inject` without `-i`: op reads the template from stdin.
    pub stdin: bool,
    /// `op run --env-file <file>`: the env files, in order.
    pub env_files: Vec<String>,
    /// The last `--account` value before `--`.
    pub account: Option<String>,
}

/// op's global flags that take a value (`--account` is kept, the others are skipped).
const GLOBAL_VALUE_FLAGS: [&str; 4] = ["--config", "--encoding", "--format", "--session"];

/// Reads the arguments the way op does (pflag): the subcommand is the first argument that is not a flag, skipping the
/// values of global flags; nothing after `--` is op's.
pub fn plan(args: &[String]) -> Plan {
    let mut p = Plan::default();
    let mut template_flag = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--" {
            break;
        }
        if a == "--account" {
            if let Some(v) = args.get(i + 1) {
                p.account = Some(v.clone());
            }
            i += 2;
            continue;
        }
        if let Some(v) = a.strip_prefix("--account=") {
            p.account = Some(v.to_string());
            i += 1;
            continue;
        }
        if GLOBAL_VALUE_FLAGS.contains(&a) {
            i += 2;
            continue;
        }
        let Some(sub) = p.subcommand.as_deref() else {
            if !a.starts_with('-') {
                p.subcommand = Some(a.to_string());
            }
            i += 1;
            continue;
        };
        match sub {
            "inject" => {
                if a == "-i" || a == "--in-file" {
                    p.template = args.get(i + 1).cloned();
                    template_flag = true;
                    i += 2;
                    continue;
                }
                if let Some(v) = a.strip_prefix("--in-file=") {
                    p.template = Some(v.to_string());
                    template_flag = true;
                    i += 1;
                    continue;
                }
                if a == "-o" || a == "--out-file" || a == "--file-mode" {
                    i += 2;
                    continue;
                }
                if a.len() > 1 && a.starts_with('-') && !a.starts_with("--") {
                    // A group of short flags (`-fi x`): `i` and `o` take the rest of the group (after an optional
                    // `=`) or the next argument; the others are booleans.
                    let group: Vec<char> = a[1..].chars().collect();
                    let mut step = 1;
                    for (k, c) in group.iter().enumerate() {
                        if *c != 'i' && *c != 'o' {
                            continue;
                        }
                        let rest: String = group[k + 1..].iter().collect();
                        let value = if rest.is_empty() {
                            step = 2;
                            args.get(i + 1).cloned()
                        } else {
                            Some(rest.strip_prefix('=').unwrap_or(&rest).to_string())
                        };
                        if *c == 'i' {
                            p.template = value;
                            template_flag = true;
                        }
                        break;
                    }
                    i += step;
                    continue;
                }
                i += 1;
            }
            "run" => {
                if a == "--env-file" {
                    if let Some(v) = args.get(i + 1) {
                        p.env_files.push(v.clone());
                    }
                    i += 2;
                    continue;
                }
                if let Some(v) = a.strip_prefix("--env-file=") {
                    p.env_files.push(v.to_string());
                    i += 1;
                    continue;
                }
                if a == "--environment" {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    p.stdin = p.subcommand.as_deref() == Some("inject") && !template_flag;
    p
}

/// Where references came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The arguments.
    Args,
    /// The template file of `op inject` (its last path component).
    File(String),
    /// The template on stdin.
    Stdin,
    /// An env file of `op run` (its last path component).
    EnvFile(String),
}

impl Source {
    fn json(&self) -> RefSource {
        let (kind, name) = match self {
            Source::Args => ("args", None),
            Source::File(n) => ("file", Some(n.clone())),
            Source::Stdin => ("stdin", None),
            Source::EnvFile(n) => ("env-file", Some(n.clone())),
        };
        RefSource {
            kind: kind.to_string(),
            name,
        }
    }
}

/// The references of one call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Collected {
    /// Every distinct reference, as shown ([`shown`]), in the order first seen.
    pub list: Vec<String>,
    /// The sources that gave at least one reference, in the order of the design (§2).
    pub from: Vec<Source>,
    /// The last `--account` value, as shown and cut.
    pub account: Option<String>,
}

impl Collected {
    /// `from` in the payload's and the log's shape.
    pub fn sources(&self) -> Vec<RefSource> {
        self.from.iter().map(Source::json).collect()
    }

    /// The payload's `facts.refs`: the first [`SHOWN`], each cut at [`SHOWN_CHARS`]. `None` without references.
    pub fn facts(&self) -> Option<Refs> {
        self.part(SHOWN, SHOWN_CHARS, self.account.clone())
    }

    /// The log's `start.refs`: up to [`LOGGED`], each cut at [`LOGGED_CHARS`]. `None` without references.
    pub fn log(&self) -> Option<Refs> {
        self.part(LOGGED, LOGGED_CHARS, None)
    }

    fn part(&self, n: usize, chars: usize, account: Option<String>) -> Option<Refs> {
        if self.list.is_empty() {
            return None;
        }
        Some(Refs {
            total: self.list.len(),
            from: self.sources(),
            list: self
                .list
                .iter()
                .take(n)
                .map(|r| clip(r, chars, "…"))
                .collect(),
            account,
        })
    }
}

/// The references of one call. `read` gives the text of a file named in the arguments (`None` when it cannot be
/// read); `stdin` is the template read from stdin, given only when [`Plan::stdin`] says op reads it.
pub fn collect(
    args: &[String],
    read: &dyn Fn(&str) -> Option<String>,
    stdin: Option<&str>,
) -> Collected {
    let plan = plan(args);
    let mut out = Collected {
        account: plan
            .account
            .as_deref()
            .map(|a| clip(&shown(a), ACCOUNT_CHARS, "…"))
            .filter(|a| !a.is_empty()),
        ..Collected::default()
    };
    let mut seen = HashSet::new();
    let mut add = |source: Source, found: Vec<String>| {
        let mut any = false;
        for r in found {
            let r = shown(&r);
            if r.len() <= PREFIX.len() {
                continue;
            }
            any = true;
            if seen.insert(r.clone()) {
                out.list.push(r);
            }
        }
        if any {
            out.from.push(source);
        }
    };
    let in_args: Vec<String> = args
        .iter()
        .take_while(|a| a.as_str() != "--")
        .filter(|a| a.starts_with(PREFIX))
        .cloned()
        .collect();
    add(Source::Args, in_args);
    if let Some(file) = &plan.template {
        if let Some(text) = read(file) {
            add(Source::File(file_name(file)), scan_template(&text));
        }
    } else if plan.stdin
        && let Some(text) = stdin
    {
        add(Source::Stdin, scan_template(text));
    }
    for file in &plan.env_files {
        if let Some(text) = read(file) {
            add(Source::EnvFile(file_name(file)), scan_env_file(&text));
        }
    }
    out
}

/// A file's last path component (`/` or `\`), as shown and cut at [`NAME_CHARS`].
fn file_name(path: &str) -> String {
    let last = path
        .rsplit(['/', '\\'])
        .find(|p| !p.is_empty())
        .unwrap_or(path);
    clip(&shown(last), NAME_CHARS, "…")
}

/// Reads a template or env file named in the arguments (relative to the current directory): a regular file only, at
/// most [`FILE_MAX`] bytes of it, as UTF-8 with invalid bytes replaced. `None` when it cannot be read.
pub fn read_file(path: &str) -> Option<String> {
    if path.is_empty() || !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = std::fs::File::open(Path::new(path)).ok()?;
    let mut bytes = Vec::new();
    file.take(FILE_MAX).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// A reference as the window and the log show it: control characters and the marks that change the direction of
/// text become spaces, so that it cannot rewrite a line or pose as another reference; spaces at both ends go.
pub fn shown(r: &str) -> String {
    r.chars()
        .map(|c| {
            if c.is_control()
                || matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
            {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim_matches(' ')
        .to_string()
}

/// The vault and item IDs of a reference whose vault and item are both IDs (exactly 26 lowercase letters or digits,
/// as written); `None` for names, variables and anything else.
pub fn link_ids(r: &str) -> Option<(&str, &str)> {
    fn id(s: &str) -> bool {
        s.len() == 26
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    }
    let rest = r.strip_prefix(PREFIX)?;
    let vault = rest.get(..26).filter(|v| id(v))?;
    let rest = rest.get(26..)?.strip_prefix('/')?;
    let item = rest.get(..26).filter(|i| id(i))?;
    let tail = rest.get(26..)?;
    (tail.is_empty() || tail.starts_with('/') || tail.starts_with('?')).then_some((vault, item))
}

/// A character that may follow `op://` in an unenclosed reference (besides a variable).
fn ref_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '?' | '_' | '.' | '/' | '=')
}

/// A character that keeps the `op://` after it from starting an unenclosed reference.
fn blocks(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '+' | '\\' | '.')
}

/// Whether `s` starts at `chars[i]`.
fn at(chars: &[char], i: usize, s: &str) -> bool {
    (i..).zip(s.chars()).all(|(k, c)| chars.get(k) == Some(&c))
}

/// The end (exclusive) of the variable at `i` (`$NAME`, `${…}` on one line), or `None` when `$` starts none.
fn variable_end(chars: &[char], i: usize) -> Option<usize> {
    match chars.get(i + 1)? {
        '{' => {
            let close = (i + 2..chars.len())
                .take_while(|k| chars[*k] != '\n')
                .find(|k| chars[*k] == '}')?;
            Some(close + 1)
        }
        c if c.is_ascii_alphabetic() || *c == '_' => {
            let mut k = i + 2;
            while chars
                .get(k)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
            {
                k += 1;
            }
            Some(k)
        }
        _ => None,
    }
}

/// The end (exclusive) of the escape `{{ "…" }}` whose quote is at `q`, or `None` when it is not one.
fn escape_end(chars: &[char], q: usize) -> Option<usize> {
    let mut k = q + 1;
    loop {
        match chars.get(k)? {
            '\n' => return None,
            '\\' => k += 2,
            '"' => break,
            _ => k += 1,
        }
    }
    k += 1;
    while matches!(chars.get(k), Some(' ' | '\t')) {
        k += 1;
    }
    at(chars, k, "}}").then_some(k + 2)
}

/// The references in a template, as 1Password's template syntax finds them (design §2.1): `{{ op://… }}` on one line
/// (spaces inside kept), and unenclosed `op://…` not preceded by a letter, a digit, `-`, `+`, `\` or `.`, running over
/// letters, digits, `-`, `?`, `_`, `.`, `/`, `=` and variables. Escaped text (`{{ "…" }}`) is skipped.
pub fn scan_template(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if at(&chars, i, "{{") {
            let mut j = i + 2;
            while matches!(chars.get(j), Some(' ' | '\t')) {
                j += 1;
            }
            if chars.get(j) == Some(&'"') {
                if let Some(end) = escape_end(&chars, j) {
                    i = end;
                    continue;
                }
            } else if at(&chars, j, PREFIX)
                && let Some(close) = (j..n.saturating_sub(1))
                    .take_while(|k| chars[*k] != '\n')
                    .find(|k| chars[*k] == '}' && chars[*k + 1] == '}')
            {
                let inner: String = chars[j..close].iter().collect();
                out.push(inner.trim_end_matches([' ', '\t']).to_string());
                i = close + 2;
                continue;
            }
            i += 1;
            continue;
        }
        if at(&chars, i, PREFIX) && (i == 0 || !blocks(chars[i - 1])) {
            let mut j = i + PREFIX.len();
            loop {
                match chars.get(j) {
                    Some(c) if ref_char(*c) => j += 1,
                    Some('$') => match variable_end(&chars, j) {
                        Some(end) => j = end,
                        None => break,
                    },
                    _ => break,
                }
            }
            if j > i + PREFIX.len() {
                out.push(chars[i..j].iter().collect());
            }
            i = j;
            continue;
        }
        i += 1;
    }
    out
}

/// The references in an env file of `op run` (design §2.2): one `KEY=VALUE` per line, `#` comments, `export` in front,
/// quoted values; a value that starts with `op://` is one reference, any other value is read as a template.
pub fn scan_env_file(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().is_empty() {
            continue;
        }
        let value = value.trim_start();
        let value = if let Some(rest) = value.strip_prefix('"') {
            let mut v = String::new();
            let mut it = rest.chars();
            while let Some(c) = it.next() {
                match c {
                    '"' => break,
                    '\\' => v.extend(it.next()),
                    _ => v.push(c),
                }
            }
            v
        } else if let Some(rest) = value.strip_prefix('\'') {
            rest.split('\'').next().unwrap_or("").to_string()
        } else {
            let end = value.find(" #").unwrap_or(value.len());
            value[..end].trim_end().to_string()
        };
        if value.starts_with(PREFIX) {
            out.push(value.trim().to_string());
        } else {
            out.extend(scan_template(&value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn stdin_is_the_template_only_for_inject_without_i() {
        assert!(plan(&s(&["inject"])).stdin);
        assert!(plan(&s(&["--account", "a", "inject", "-o", "out"])).stdin);
        assert!(!plan(&s(&["inject", "-i", "t"])).stdin);
        assert!(!plan(&s(&["inject", "-i"])).stdin, "op reads no stdin then");
        assert!(!plan(&s(&["read", "op://a/b/c"])).stdin);
        assert!(!plan(&s(&["run", "--", "inject"])).stdin);
        assert!(
            !plan(&s(&["--format", "inject", "read"])).stdin,
            "inject is the value of --format"
        );
        assert_eq!(
            plan(&s(&["inject", "-o", "-i"])).template,
            None,
            "-i is the value of -o"
        );
    }

    #[test]
    fn eight_shown_all_logged_with_clips() {
        let mut args = s(&["read"]);
        for k in 0..12 {
            args.push(format!("op://v/i/f{k}"));
        }
        args.push(format!("op://v/i/{}", "x".repeat(400)));
        let c = collect(&args, &|_| None, None);
        let f = c.facts().unwrap();
        assert_eq!(f.total, 13);
        assert_eq!(f.list.len(), SHOWN);
        assert_eq!(f.list[0], "op://v/i/f0");
        let log = c.log().unwrap();
        assert_eq!(log.list.len(), 13);
        assert_eq!(log.list[12].chars().count(), LOGGED_CHARS + 1, "cut with …");
        assert!(log.list[12].ends_with('…'));
        assert_eq!(log.account, None);
        assert_eq!(
            collect(&s(&["vault", "list"]), &|_| None, None).facts(),
            None
        );
    }

    #[test]
    fn the_payload_cuts_references_and_names() {
        let long = format!("op://v/i/{}", "y".repeat(300));
        let name = format!("dir/{}.tpl", "n".repeat(60));
        let args = vec![
            "--account".to_string(),
            "a".repeat(150),
            "inject".to_string(),
            "-i".to_string(),
            name.clone(),
        ];
        let text = format!("{{{{ {long} }}}}");
        let c = collect(&args, &|p| (p == name).then(|| text.clone()), None);
        let f = c.facts().unwrap();
        assert_eq!(f.list[0].chars().count(), SHOWN_CHARS + 1);
        assert_eq!(
            f.from[0].name.as_deref().map(|n| n.chars().count()),
            Some(NAME_CHARS + 1)
        );
        assert_eq!(
            f.account.as_deref().map(|a| a.chars().count()),
            Some(ACCOUNT_CHARS + 1)
        );
    }

    #[test]
    fn shown_replaces_control_and_direction_characters() {
        assert_eq!(shown(" op://a/b\tc\u{2067}d "), "op://a/b c d");
        assert_eq!(shown("op://a\u{0085}b"), "op://a b", "C1 controls too");
    }

    #[test]
    fn a_template_that_ends_inside_a_reference_or_escape() {
        assert_eq!(scan_template("x op://a/b/c"), s(&["op://a/b/c"]));
        assert_eq!(
            scan_template("{{ \"op://a/b/c"),
            s(&["op://a/b/c"]),
            "not an escape: the reference is unenclosed"
        );
        assert_eq!(scan_template("{{ op://a/b/c }"), s(&["op://a/b/c"]));
        assert_eq!(scan_template("op://${A"), Vec::<String>::new());
        assert_eq!(scan_template("{{"), Vec::<String>::new());
    }

    #[test]
    fn env_file_values() {
        let text = "A=\"op://v/i/with \\\"quote\\\"\"\nB = op://v/i/b\n=op://v/i/nokey\nC\n";
        assert_eq!(
            scan_env_file(text),
            s(&["op://v/i/with \"quote\"", "op://v/i/b"])
        );
    }

    #[test]
    fn reads_regular_files_only() {
        let dir = std::env::temp_dir().join(format!("op-reason-refs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.tpl");
        std::fs::write(&file, b"a={{ op://v/i/f }}\xff").unwrap();
        let text = read_file(file.to_str().unwrap()).unwrap();
        assert!(text.starts_with("a={{ op://v/i/f }}"));
        assert!(text.ends_with('\u{FFFD}'));
        assert_eq!(read_file(dir.to_str().unwrap()), None, "a directory");
        assert_eq!(read_file(""), None);
        assert_eq!(read_file(dir.join("none").to_str().unwrap()), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

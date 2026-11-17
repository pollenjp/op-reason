//! Command lines as the relay shows and records them: a Windows command line split into arguments, masked with the
//! rules of `op` calls (`name=value` → `name=***`; design §9), and ssh's destination read from them.

use crate::args;

/// Splits a Windows command line into arguments the way `CommandLineToArgvW` and the C runtime do: the program up to
/// the first space or tab (or between the first pair of quotes), then arguments separated by spaces and tabs, where
/// quotes group, `\"` is a quote, backslashes count only before a quote, and `""` inside quotes is a quote.
pub fn split_windows(line: &str) -> Vec<String> {
    let line = line.trim_start_matches([' ', '\t']);
    if line.is_empty() {
        return Vec::new();
    }
    // The program: between quotes, or up to the first space or tab; never unescaped
    let (program, rest) = match line.strip_prefix('"') {
        Some(quoted) => match quoted.find('"') {
            Some(end) => (&quoted[..end], &quoted[end + 1..]),
            None => (quoted, ""),
        },
        None => match line.find([' ', '\t']) {
            Some(end) => (&line[..end], &line[end..]),
            None => (line, ""),
        },
    };
    let mut out = vec![program.to_owned()];
    let mut cur = String::new();
    let mut have = false;
    let mut quoted = false;
    let mut backslashes = 0usize;
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                cur.extend(std::iter::repeat_n('\\', backslashes / 2));
                if backslashes % 2 == 1 {
                    cur.push('"');
                } else if quoted && chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    quoted = !quoted;
                }
                backslashes = 0;
                have = true;
            }
            ' ' | '\t' if !quoted => {
                cur.extend(std::iter::repeat_n('\\', backslashes));
                backslashes = 0;
                if have || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                have = false;
            }
            c => {
                cur.extend(std::iter::repeat_n('\\', backslashes));
                backslashes = 0;
                cur.push(c);
                have = true;
            }
        }
    }
    cur.extend(std::iter::repeat_n('\\', backslashes));
    if have || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The arguments (without the program) as the window and the records show them: the rules of `op` calls (the values of
/// `--session` and `--secret-key`, and of `name=value`, become `***`; options and `op://` references stay), applied
/// to each word of an argument that holds spaces (a remote command), and without the rule for what follows `--`.
pub fn mask(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut secret_next = false;
    for a in args {
        if secret_next {
            out.push("***".to_owned());
            secret_next = false;
        } else if a == "--session" || a == "--secret-key" {
            out.push(a.clone());
            secret_next = true;
        } else if a.contains([' ', '\t']) {
            out.push(a.split(' ').map(mask_word).collect::<Vec<_>>().join(" "));
        } else {
            out.push(mask_word(a));
        }
    }
    out
}

/// One word by the rules of `op` calls.
fn mask_word(a: &str) -> String {
    if let Some(name) = ["--session=", "--secret-key="]
        .into_iter()
        .find(|p| a.starts_with(p))
    {
        return format!("{name}***");
    }
    if a.starts_with('-') || a.starts_with("op://") {
        return a.to_owned();
    }
    match a.split_once('=') {
        Some((name, _)) => format!("{name}=***"),
        None => a.to_owned(),
    }
}

/// The program's file name, then the arguments as `op` calls show them (quoted when they hold anything but
/// `[A-Za-z0-9_./:=@%+,*?-]`; control characters as spaces).
pub fn display(args: &[String]) -> String {
    let Some((program, rest)) = args.split_first() else {
        return String::new();
    };
    let name = program.rsplit(['\\', '/']).next().unwrap_or(program);
    let mut out = args::word(name);
    for a in rest {
        out.push(' ');
        out.push_str(&args::word(a));
    }
    out
}

/// [`display`] of the program and the masked arguments, cut after `max` characters.
pub fn shown(args: &[String], max: usize) -> String {
    let Some((program, rest)) = args.split_first() else {
        return String::new();
    };
    let mut all = vec![program.clone()];
    all.extend(mask(rest));
    args::clip(&display(&all), max, "…")
}

/// Where an ssh command line connects: the destination as given, and the values of `-l` and `-p`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<String>,
}

impl Destination {
    /// `user@host:port`, with only the parts known (a destination that names its user keeps it).
    pub fn text(&self) -> String {
        let mut out = match &self.user {
            Some(user) if !self.host.contains('@') => format!("{user}@{}", self.host),
            _ => self.host.clone(),
        };
        if let Some(port) = &self.port {
            out.push(':');
            out.push_str(port);
        }
        out
    }
}

/// The destination of ssh's arguments (`args[0]` is the program): the first argument that is not an option or an
/// option's value.
pub fn destination(args: &[String]) -> Option<Destination> {
    // ssh's options that take a value (OpenSSH 9)
    const VALUED: &str = "BbcDEeFIiJLlmOoPpQRSWw";
    let mut user = None;
    let mut port = None;
    let mut rest = args.iter().skip(1);
    while let Some(a) = rest.next() {
        if a == "--" {
            break;
        }
        let Some(flags) = a.strip_prefix('-').filter(|f| !f.is_empty()) else {
            return Some(Destination {
                host: a.clone(),
                user,
                port,
            });
        };
        for (i, c) in flags.char_indices() {
            if VALUED.contains(c) {
                let glued = &flags[i + c.len_utf8()..];
                let value = if glued.is_empty() {
                    rest.next()?.clone()
                } else {
                    glued.to_owned()
                };
                match c {
                    'l' => user = Some(value),
                    'p' => port = Some(value),
                    _ => {}
                }
                break;
            }
        }
    }
    rest.next().map(|host| Destination {
        host: host.clone(),
        user,
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn splits_like_commandlinetoargvw() {
        assert_eq!(
            split_windows(r#""C:\Program Files\OpenSSH\ssh.exe" -T git@github.com"#),
            v(&[r"C:\Program Files\OpenSSH\ssh.exe", "-T", "git@github.com"])
        );
        assert_eq!(
            split_windows(r#"ssh.exe host "git-upload-pack 'me/r.git'""#),
            v(&["ssh.exe", "host", "git-upload-pack 'me/r.git'"])
        );
        // backslashes count only before a quote; \" is a quote; \\" is a backslash and a quote that groups
        assert_eq!(
            split_windows(r#"p a\b c\\ "d e\" f" "g\\" h"#),
            v(&["p", r"a\b", r"c\\", r#"d e" f"#, r"g\", "h"])
        );
        // "" inside quotes is a quote; an empty pair is an empty argument
        assert_eq!(
            split_windows(r#"p "a""b" "" x"#),
            v(&["p", r#"a"b"#, "", "x"])
        );
        // the program is never unescaped, and tabs separate
        assert_eq!(split_windows("p\\\"q\ta\t b"), v(&["p\\\"q", "a", "b"]));
        assert_eq!(split_windows(""), Vec::<String>::new());
        assert_eq!(split_windows("   "), Vec::<String>::new());
    }

    #[test]
    fn masks_values_and_keeps_the_remote_command() {
        assert_eq!(
            mask(&v(&[
                "-o",
                "SendEnv=GIT_PROTOCOL",
                "-oBatchMode=yes",
                "git@github.com",
                "git-upload-pack 'me/r.git'",
            ])),
            v(&[
                "-o",
                "SendEnv=***",
                "-oBatchMode=yes",
                "git@github.com",
                "git-upload-pack 'me/r.git'",
            ])
        );
        assert_eq!(
            mask(&v(&["host", "env API_TOKEN=abc deploy.sh", "--", "x=1"])),
            v(&["host", "env API_TOKEN=*** deploy.sh", "--", "x=***"]),
            "each word of a remote command; no rule for what follows --"
        );
        assert_eq!(
            mask(&v(&["--session", "s3cret", "--secret-key=k", "op://v/i/f"])),
            v(&["--session", "***", "--secret-key=***", "op://v/i/f"])
        );
    }

    #[test]
    fn displays_the_program_by_its_name() {
        assert_eq!(
            display(&v(&[
                r"C:\Program Files\OpenSSH\ssh.exe",
                "-T",
                "git@github.com",
                "git-upload-pack 'me/r.git'",
            ])),
            r#"ssh.exe -T git@github.com "git-upload-pack 'me/r.git'""#
        );
        assert_eq!(display(&v(&["/usr/bin/ssh", "a\nb"])), r#"ssh "a b""#);
        assert_eq!(display(&[]), "");
        let long = v(&["ssh.exe", "-o", "SetEnv=TOKEN=abc", &"h".repeat(400)]);
        let s = shown(&long, 300);
        assert!(s.starts_with("ssh.exe -o SetEnv=*** hhh"), "{s}");
        assert_eq!(s.chars().count(), 301, "300 and …");
    }

    #[test]
    fn the_destination_after_the_options() {
        let d = destination(&v(&[
            "ssh.exe",
            "-p",
            "2222",
            "-l",
            "me",
            "-o",
            "ProxyJump=jump",
            "-4A",
            "host",
            "uptime",
        ]))
        .unwrap();
        assert_eq!(
            d,
            Destination {
                host: "host".into(),
                user: Some("me".into()),
                port: Some("2222".into()),
            }
        );
        assert_eq!(d.text(), "me@host:2222");
        let git = destination(&v(&[
            "ssh.exe",
            "-o",
            "SendEnv=GIT_PROTOCOL",
            "git@github.com",
            "git-upload-pack 'me/r.git'",
        ]))
        .unwrap();
        assert_eq!(git.text(), "git@github.com");
        // values glued to their option, -J's value, and -- before the destination (as scp and sftp start ssh)
        let scp = destination(&v(&[
            "ssh",
            "-p22",
            "-Jjump",
            "-lme",
            "-x",
            "-oForwardAgent=no",
            "-s",
            "--",
            "nas",
            "sftp",
        ]))
        .unwrap();
        assert_eq!(scp.text(), "me@nas:22");
        let url = destination(&v(&["ssh", "ssh://u@h:2200"])).unwrap();
        assert_eq!(url.text(), "ssh://u@h:2200");
        assert_eq!(destination(&v(&["ssh", "-V"])), None);
        assert_eq!(destination(&v(&["ssh", "-p"])), None, "a value missing");
    }
}

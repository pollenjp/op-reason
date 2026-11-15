//! The repository and branch of a directory, read from git's files (no `git` executable needed: Windows may not have
//! one, and the PowerShell entry reads the same files).

use std::fs;
use std::path::{Path, PathBuf};

/// Repository and branch of a directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaceInfo {
    /// `owner/name` from the origin URL, or the name of the directory that holds `.git`.
    pub repo: Option<String>,
    /// The branch, the ref, or `(detached <first 7 characters>)`.
    pub branch: Option<String>,
}

struct GitDirs {
    /// The directory that holds `.git`.
    top: PathBuf,
    /// The git directory of this work tree (`HEAD` is here).
    git_dir: PathBuf,
    /// The shared git directory (`config` is here).
    common_dir: PathBuf,
}

fn find_git(start: &Path) -> Option<GitDirs> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let dot_git = d.join(".git");
        if let Ok(meta) = fs::metadata(&dot_git) {
            let git_dir = if meta.is_dir() {
                dot_git
            } else {
                let text = fs::read_to_string(&dot_git).ok()?;
                let target = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
                let target = PathBuf::from(target);
                if target.is_absolute() {
                    target
                } else {
                    d.join(target)
                }
            };
            let common_dir = match fs::read_to_string(git_dir.join("commondir")) {
                Ok(text) => {
                    let c = PathBuf::from(text.trim());
                    if c.is_absolute() { c } else { git_dir.join(c) }
                }
                Err(_) => git_dir.clone(),
            };
            return Some(GitDirs {
                top: d.to_path_buf(),
                git_dir,
                common_dir,
            });
        }
        dir = d.parent();
    }
    None
}

/// Reads the origin URL from a git `config` file: `url` in the `[remote "origin"]` section.
fn origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for raw in config.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            let header = line.trim_start_matches('[').trim_end_matches(']').trim();
            in_origin = header == r#"remote "origin""#;
            continue;
        }
        if !in_origin {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
        {
            let v = value.trim().trim_matches('"').to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// `owner/name` from a remote URL: a trailing `/` and `.git` are removed; the owner is the path segment before the last
/// `/` (after `:` or `/`).
pub fn repo_from_url(url: &str) -> String {
    let url = url.strip_suffix('/').unwrap_or(url);
    let url = url.strip_suffix(".git").unwrap_or(url);
    let (head, name) = match url.rfind('/') {
        Some(i) => (&url[..i], &url[i + 1..]),
        None => ("", url),
    };
    let owner = match head.rfind([':', '/']) {
        Some(i) => &head[i + 1..],
        None => head,
    };
    if owner.is_empty() {
        name.to_string()
    } else {
        format!("{owner}/{name}")
    }
}

/// The repository and branch of `cwd`. Both are `None` outside a repository (or when the directory cannot be read).
pub fn detect(cwd: &Path) -> PlaceInfo {
    let Some(dirs) = find_git(cwd) else {
        return PlaceInfo::default();
    };
    let repo = fs::read_to_string(dirs.common_dir.join("config"))
        .ok()
        .and_then(|c| origin_url(&c))
        .map(|u| repo_from_url(&u))
        .or_else(|| {
            dirs.top
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        });
    let branch = fs::read_to_string(dirs.git_dir.join("HEAD"))
        .ok()
        .and_then(|head| {
            let head = head.trim();
            if let Some(r) = head.strip_prefix("ref:") {
                let r = r.trim();
                Some(r.strip_prefix("refs/heads/").unwrap_or(r).to_string())
            } else if head.len() >= 7 && head.bytes().all(|b| b.is_ascii_hexdigit()) {
                Some(format!("(detached {})", &head[..7]))
            } else {
                None
            }
        });
    PlaceInfo { repo, branch }
}

/// `~` for the home directory at the start of a path.
pub fn tilde(path: &str, home: Option<&str>) -> String {
    match home.filter(|h| !h.is_empty()) {
        Some(h) if path == h => "~".to_string(),
        Some(h) => {
            let h = h.trim_end_matches(['/', '\\']);
            match path.strip_prefix(h) {
                Some(rest) if rest.starts_with('/') || rest.starts_with('\\') => format!("~{rest}"),
                _ => path.to_string(),
            }
        }
        None => path.to_string(),
    }
}

/// The place as the window shows it: `<repo> (<branch>) · <cwd>`, or `<cwd>` outside a repository.
pub fn display(info: &PlaceInfo, cwd_display: &str) -> String {
    match &info.repo {
        Some(repo) => format!(
            "{repo} ({}) · {cwd_display}",
            info.branch.as_deref().unwrap_or("?")
        ),
        None => cwd_display.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "op-reason-place-{name}-{}",
            crate::log::random_u64()
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn repo_names_from_urls() {
        assert_eq!(repo_from_url("git@github.com:o/r.git"), "o/r");
        assert_eq!(repo_from_url("https://github.com/o/r/"), "o/r");
        assert_eq!(repo_from_url("https://github.com/o/r.git"), "o/r");
        assert_eq!(repo_from_url("ssh://git@host:22/group/o/r"), "o/r");
        assert_eq!(repo_from_url("/srv/git/r.git"), "git/r");
        assert_eq!(repo_from_url("r"), "r");
    }

    #[test]
    fn branch_origin_and_subdirectory() {
        let top = temp("plain");
        let git = top.join(".git");
        fs::create_dir_all(&git).unwrap();
        fs::write(git.join("HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        fs::write(
            git.join("config"),
            "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = git@github.com:u/u.git\n[remote \"origin\"]\n\turl = git@github.com:o/r.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n",
        )
        .unwrap();
        let sub = top.join("a/b");
        fs::create_dir_all(&sub).unwrap();
        let info = detect(&sub);
        assert_eq!(info.repo.as_deref(), Some("o/r"));
        assert_eq!(info.branch.as_deref(), Some("feat/x"));
        fs::write(
            git.join("HEAD"),
            "0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();
        assert_eq!(detect(&top).branch.as_deref(), Some("(detached 0123456)"));
        fs::remove_dir_all(&top).unwrap();
    }

    #[test]
    fn worktree_with_gitdir_file_and_commondir() {
        let main = temp("main");
        let git = main.join(".git");
        fs::create_dir_all(git.join("worktrees/wt")).unwrap();
        fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            git.join("config"),
            "[remote \"origin\"]\n  url = https://github.com/o/r\n",
        )
        .unwrap();
        fs::write(git.join("worktrees/wt/HEAD"), "ref: refs/heads/feature\n").unwrap();
        fs::write(git.join("worktrees/wt/commondir"), "../..\n").unwrap();
        let wt = temp("wt");
        fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", git.join("worktrees/wt").display()),
        )
        .unwrap();
        let info = detect(&wt);
        assert_eq!(info.repo.as_deref(), Some("o/r"));
        assert_eq!(info.branch.as_deref(), Some("feature"));
        fs::remove_dir_all(&main).unwrap();
        fs::remove_dir_all(&wt).unwrap();
    }

    #[test]
    fn without_origin_and_outside() {
        let top = temp("noorigin");
        fs::create_dir_all(top.join(".git")).unwrap();
        fs::write(top.join(".git/HEAD"), "ref: refs/heads/b\n").unwrap();
        let info = detect(&top);
        assert_eq!(
            info.repo,
            top.file_name().map(|n| n.to_string_lossy().into_owned())
        );
        fs::remove_dir_all(&top).unwrap();
        assert_eq!(detect(&top), PlaceInfo::default());
        let outside = temp("outside");
        // The temporary directory itself is not in a repository on CI machines; a parent with .git would be found,
        // so only check that a missing directory gives nothing.
        assert_eq!(
            detect(&outside.join("missing/deeper")).branch.is_some(),
            detect(&outside).branch.is_some()
        );
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn tilde_and_display() {
        assert_eq!(tilde("/home/u", Some("/home/u")), "~");
        assert_eq!(tilde("/home/u/x", Some("/home/u")), "~/x");
        assert_eq!(tilde("/home/user2", Some("/home/u")), "/home/user2");
        assert_eq!(tilde("/x", None), "/x");
        assert_eq!(
            display(
                &PlaceInfo {
                    repo: Some("o/r".into()),
                    branch: Some("b".into())
                },
                "~/x"
            ),
            "o/r (b) · ~/x"
        );
        assert_eq!(display(&PlaceInfo::default(), "~/x"), "~/x");
    }
}

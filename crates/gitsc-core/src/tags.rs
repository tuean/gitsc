//! 分支 / stash / tag 的列表读取。

use crate::command::GitError;
use crate::Git;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    /// 短名，例如 `main`、`origin/main`。
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    /// 上游短名（只有本地分支有）。
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub oid: String,
    pub subject: String,
    pub time: i64,
}

impl Branch {
    /// 远端分支的短显示名：`origin/main` → `main`。
    pub fn short(&self) -> &str {
        if self.is_remote {
            self.name.split_once('/').map(|(_, r)| r).unwrap_or(&self.name)
        } else {
            &self.name
        }
    }

    pub fn remote(&self) -> Option<&str> {
        if self.is_remote {
            self.name.split_once('/').map(|(l, _)| l)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stash {
    pub index: usize,
    pub oid: String,
    pub message: String,
    pub time: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub oid: String,
    pub subject: String,
    pub time: i64,
}

pub fn branches(git: &mut Git) -> Result<Vec<Branch>, GitError> {
    let out = git.run_ok(&[
        "for-each-ref",
        "--format=%(refname:short)%1f%(objectname)%1f%(upstream:short)%1f%(HEAD)%1f%(upstream:track)%1f%(contents:subject)%1f%(committerdate:unix)%1f%(refname)",
        "refs/heads",
        "refs/remotes",
    ])?;

    let mut list: Vec<Branch> = out
        .stdout
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\u{1f}').collect();
            if f.len() < 8 {
                return None;
            }
            let full = f[7];
            let is_remote = full.starts_with("refs/remotes/");
            // origin/HEAD 指向默认分支，不单列。
            if is_remote && f[0].ends_with("/HEAD") {
                return None;
            }
            let (ahead, behind) = parse_track(f[4]);
            Some(Branch {
                name: f[0].to_string(),
                is_current: f[3] == "*",
                is_remote,
                upstream: (!f[2].is_empty()).then(|| f[2].to_string()),
                ahead,
                behind,
                oid: f[1].to_string(),
                subject: f[5].to_string(),
                time: f[6].parse().unwrap_or(0),
            })
        })
        .collect();

    // 当前分支在最上，其余本地在前、远端在后。
    list.sort_by_key(|b| (!b.is_current, b.is_remote, b.name.clone()));
    Ok(list)
}

/// `[ahead 2, behind 1]` / `[gone]` / 空。
fn parse_track(track: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    for part in track.trim_matches(['[', ']']).split(',') {
        let part = part.trim();
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

pub fn stashes(git: &mut Git) -> Result<Vec<Stash>, GitError> {
    let out = git.run_ok(&[
        "stash",
        "list",
        "--format=%gd%x1f%H%x1f%gs%x1f%at",
    ])?;
    Ok(out
        .stdout
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\u{1f}').collect();
            if f.len() < 4 {
                return None;
            }
            let index = f[0]
                .trim()
                .trim_start_matches("stash@{")
                .trim_end_matches('}')
                .parse()
                .ok()?;
            Some(Stash {
                index,
                oid: f[1].to_string(),
                message: f[2].to_string(),
                time: f[3].parse().unwrap_or(0),
            })
        })
        .collect())
}

pub fn tags(git: &mut Git) -> Result<Vec<Tag>, GitError> {
    let out = git.run_ok(&[
        "for-each-ref",
        "--sort=-creatordate",
        "--format=%(refname:short)%1f%(objectname)%1f%(contents:subject)%1f%(creatordate:unix)",
        "refs/tags",
    ])?;
    Ok(out
        .stdout
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\u{1f}').collect();
            if f.len() < 4 {
                return None;
            }
            Some(Tag {
                name: f[0].to_string(),
                oid: f[1].to_string(),
                subject: f[2].to_string(),
                time: f[3].parse().unwrap_or(0),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_counts() {
        assert_eq!(parse_track("[ahead 2, behind 1]"), (2, 1));
        assert_eq!(parse_track("[ahead 3]"), (3, 0));
        assert_eq!(parse_track(""), (0, 0));
        assert_eq!(parse_track("[gone]"), (0, 0));
    }

    #[test]
    fn remote_branch_short_name() {
        let b = Branch {
            name: "origin/feature/x".into(),
            is_current: false,
            is_remote: true,
            upstream: None,
            ahead: 0,
            behind: 0,
            oid: "abc".into(),
            subject: "s".into(),
            time: 0,
        };
        assert_eq!(b.short(), "feature/x");
        assert_eq!(b.remote(), Some("origin"));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

/// 已配置的远端（按 `git remote -v` 的 fetch 行去重）。
pub fn remotes(git: &mut Git) -> Result<Vec<RemoteInfo>, GitError> {
    let out = git.run_ok(&["remote", "-v"])?;
    let mut list: Vec<RemoteInfo> = Vec::new();
    for line in out.stdout.lines() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(url), Some(kind)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if kind != "(fetch)" {
            continue;
        }
        if list.iter().any(|r| r.name == name) {
            continue;
        }
        list.push(RemoteInfo {
            name: name.to_string(),
            url: url.to_string(),
        });
    }
    Ok(list)
}

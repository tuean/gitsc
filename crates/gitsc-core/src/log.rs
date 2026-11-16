//! `git log` 读取与提交引用解析。

use crate::command::GitError;
use crate::{Git, GitOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    Head,
    Local,
    Remote,
    Tag,
}

impl RefKind {
    pub fn is_branch(self) -> bool {
        matches!(self, Self::Head | Self::Local | Self::Remote)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitRef {
    pub name: String,
    pub kind: RefKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub oid: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    /// Unix 时间戳（秒）。
    pub time: i64,
    pub refs: Vec<CommitRef>,
    pub summary: String,
}

impl Commit {
    pub fn short(&self) -> &str {
        short_oid(&self.oid)
    }

    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }

    pub fn is_head(&self) -> bool {
        self.refs.iter().any(|r| r.kind == RefKind::Head)
    }

    /// 优先返回分支名（HEAD 指向的分支取名字），供列表右侧显示。
    pub fn primary_ref(&self) -> Option<&CommitRef> {
        self.refs
            .iter()
            .find(|r| r.kind == RefKind::Head)
            .or_else(|| self.refs.iter().find(|r| r.kind == RefKind::Local))
            .or_else(|| self.refs.first())
    }
}

pub fn short_oid(oid: &str) -> &str {
    &oid[..oid.len().min(7)]
}

/// 读取提交历史。`limit` 是硬上限；`all_branches` 为 false 时只看当前分支。
pub fn read(
    git: &mut Git,
    limit: usize,
    all_branches: bool,
) -> Result<Vec<Commit>, GitError> {
    let count = limit.to_string();
    let mut args: Vec<&str> = vec!["log", "--date-order", "-n", &count, "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e"];
    if all_branches {
        // 不用 --all：它会把 refs/stash、notes 一起带进来，看起来像普通提交。
        args.insert(1, "--branches");
        args.insert(2, "--remotes");
        args.insert(3, "--tags");
    }
    let out = git.run_ok(&args)?;
    let remotes: Vec<String> = git
        .run(&["remote"])
        .stdout
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    Ok(parse_with_remotes(&out, &remotes))
}

/// 读取 `from..to` 之间的提交（新 → 旧），用于「拉取前先看会带来什么」。
pub fn read_range(
    git: &mut Git,
    range: &str,
    limit: usize,
) -> Result<Vec<Commit>, GitError> {
    let count = limit.to_string();
    let args = vec![
        "log",
        "--date-order",
        "-n",
        &count,
        range,
        "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e",
    ];
    let out = git.run_ok(&args)?;
    let remotes: Vec<String> = git
        .run(&["remote"])
        .stdout
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    Ok(parse_with_remotes(&out, &remotes))
}

pub fn parse(out: &GitOutput) -> Vec<Commit> {
    parse_with_remotes(out, &["origin".to_string()])
}

/// `remotes` 是 `git remote` 的名字；用来把 `origin/x` 判成远端分支，
/// 而本地分支 `feature/x` 里的 `/` 不会被误判。
pub fn parse_with_remotes(out: &GitOutput, remotes: &[String]) -> Vec<Commit> {
    out.stdout
        .split('\u{1e}')
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            if record.is_empty() {
                return None;
            }
            let fields: Vec<&str> = record.split('\u{1f}').collect();
            if fields.len() < 7 {
                return None;
            }
            let refs = parse_refs_with_remotes(fields[5], remotes);
            Some(Commit {
                oid: fields[0].to_string(),
                parents: fields[1]
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                author: fields[2].to_string(),
                email: fields[3].to_string(),
                time: fields[4].parse().unwrap_or(0),
                refs,
                summary: fields[6].trim_end().to_string(),
            })
        })
        .collect()
}

/// 解析 `%D`：`HEAD -> main, origin/main, tag: v1.0, main`。
pub fn parse_refs(decorations: &str) -> Vec<CommitRef> {
    parse_refs_with_remotes(decorations, &["origin".to_string()])
}

pub fn parse_refs_with_remotes(decorations: &str, remotes: &[String]) -> Vec<CommitRef> {
    let mut refs: Vec<CommitRef> = Vec::new();
    for raw in decorations.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        // 贮藏是内部引用，不进提交历史。
        if raw.starts_with("refs/stash") || raw.starts_with("stash@") {
            continue;
        }
        if let Some(name) = raw.strip_prefix("HEAD -> ") {
            push_ref(&mut refs, name, RefKind::Head);
        } else if raw == "HEAD" {
            push_ref(&mut refs, "HEAD", RefKind::Head);
        } else if let Some(name) = raw.strip_prefix("tag: ") {
            push_ref(&mut refs, name, RefKind::Tag);
        } else if is_remote_ref(raw, remotes) {
            push_ref(&mut refs, raw, RefKind::Remote);
        } else {
            push_ref(&mut refs, raw, RefKind::Local);
        }
    }
    refs
}

fn is_remote_ref(name: &str, remotes: &[String]) -> bool {
    let Some((first, _)) = name.split_once('/') else {
        return false;
    };
    remotes.iter().any(|r| r == first)
}

fn push_ref(refs: &mut Vec<CommitRef>, name: &str, kind: RefKind) {
    if !refs.iter().any(|r| r.name == name && r.kind == kind) {
        refs.push(CommitRef {
            name: name.to_string(),
            kind,
        });
    }
}

/// 提交详情里的文件统计（来自 `git show --stat`）。
#[derive(Clone, Debug, Default)]
pub struct FileStat {
    pub path: String,
    pub added: u32,
    pub removed: u32,
    pub binary: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_record() {
        let text = "abc123\u{1f}p1 p2\u{1f}Alice\u{1f}a@x.com\u{1f}1700000000\u{1f}HEAD -> main, origin/main, tag: v1\u{1f}fix: 修好\u{1e}\n";
        let out = GitOutput {
            argv: vec![],
            cwd: Default::default(),
            stdout: text.into(),
            stderr: String::new(),
            code: Some(0),
            duration_ms: 0,
        };
        let commits = parse(&out);
        assert_eq!(commits.len(), 1);
        let c = &commits[0];
        assert_eq!(c.oid, "abc123");
        assert_eq!(c.parents, vec!["p1", "p2"]);
        assert!(c.is_merge());
        assert_eq!(c.summary, "fix: 修好");
        assert!(c.is_head());
        assert_eq!(c.refs.len(), 3);
        assert_eq!(c.refs[2].kind, RefKind::Tag);
    }

    #[test]
    fn refs_without_decorations_are_empty() {
        assert!(parse_refs("").is_empty());
        let refs = parse_refs("feature/x");
        assert_eq!(refs[0].kind, RefKind::Local);
    }
}

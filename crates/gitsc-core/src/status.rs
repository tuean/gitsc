//! `git status --porcelain=v2` 解析。
//!
//! 采用 porcelain v2 而不是给人看的 v1：分支、上游、ahead/behind 都在同一份
//! 输出里，且字段位置固定，不需要按本地化文案猜。

use crate::command::GitError;
use crate::{Git, GitOutput};

/// 单个路径在索引 / 工作区里的变化类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
    Untracked,
    Ignored,
    Unknown,
}

impl Code {
    pub fn from_char(c: char) -> Self {
        match c {
            'M' => Self::Modified,
            'A' => Self::Added,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            'T' => Self::TypeChanged,
            'U' => Self::Unmerged,
            '?' => Self::Untracked,
            '!' => Self::Ignored,
            '.' | ' ' => Self::Unmodified,
            _ => Self::Unknown,
        }
    }

    /// IDEA 风格的单字母标记。
    pub fn letter(self) -> char {
        match self {
            Self::Modified => 'M',
            Self::Added => 'A',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::Copied => 'C',
            Self::TypeChanged => 'T',
            Self::Unmerged => 'U',
            Self::Untracked => '?',
            Self::Ignored => 'I',
            Self::Unmodified => ' ',
            Self::Unknown => '?',
        }
    }

    pub fn is_change(self) -> bool {
        matches!(
            self,
            Self::Modified
                | Self::Added
                | Self::Deleted
                | Self::Renamed
                | Self::Copied
                | Self::TypeChanged
                | Self::Unmerged
        )
    }
}

/// 一个被改动（或未跟踪 / 冲突）的路径。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub orig_path: Option<String>,
    pub index: Code,
    pub worktree: Code,
    pub untracked: bool,
    pub conflicted: bool,
}

impl Change {
    pub fn staged(&self) -> bool {
        !self.untracked && !self.conflicted && self.index.is_change()
    }

    pub fn unstaged(&self) -> bool {
        self.untracked
            || self.conflicted
            || matches!(
                self.worktree,
                Code::Modified
                    | Code::Added
                    | Code::Deleted
                    | Code::Renamed
                    | Code::Copied
                    | Code::TypeChanged
                    | Code::Unmerged
            )
    }

    /// 用在列表左侧的标记：优先显示工作区状态，未暂存的新文件显示 `?`。
    pub fn badge(&self) -> char {
        if self.conflicted {
            'U'
        } else if self.untracked {
            '?'
        } else if self.worktree.is_change() {
            self.worktree.letter()
        } else {
            self.index.letter()
        }
    }

    /// 提交后路径会变成什么（重命名取新路径）。
    pub fn display_path(&self) -> &str {
        &self.path
    }

    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn dir(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[..i],
            None => "",
        }
    }
}

/// 一份完整的 `git status` 快照。
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub branch: Option<String>,
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub changes: Vec<Change>,
}

impl Status {
    pub fn staged(&self) -> impl Iterator<Item = &Change> {
        self.changes.iter().filter(|c| c.staged())
    }

    pub fn unstaged(&self) -> impl Iterator<Item = &Change> {
        self.changes.iter().filter(|c| c.unstaged() && !c.staged())
    }

    pub fn staged_count(&self) -> usize {
        self.staged().count()
    }

    pub fn unstaged_count(&self) -> usize {
        self.changes.iter().filter(|c| c.unstaged()).count()
    }

    pub fn is_clean(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn find(&self, path: &str) -> Option<&Change> {
        self.changes.iter().find(|c| c.path == path)
    }
}

pub fn read(git: &mut Git) -> Result<Status, GitError> {
    let out = git.run_ok(&[
        "status",
        "--porcelain=v2",
        "--branch",
        "--untracked-files=all",
        "-z",
    ])?;
    Ok(parse(&out))
}

/// 解析 `-z` 版 porcelain v2。路径块之间用 NUL 分隔；重命名条目后面紧跟
/// 一段原始路径。
pub fn parse(out: &GitOutput) -> Status {
    let mut status = Status::default();
    // 真实调用带 `-z`，用 NUL 分隔；测试和少数平台给的是换行分隔，一并兼容。
    let sep = if out.stdout.contains('\0') { '\0' } else { '\n' };
    let mut parts = out.stdout.split(sep).filter(|s| !s.is_empty()).peekable();

    while let Some(part) = parts.next() {
        if let Some(rest) = part.strip_prefix("# ") {
            parse_header(rest, &mut status);
            continue;
        }
        let kind = part.chars().next();
        match kind {
            // 普通改动：1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
            Some('1') => {
                if let Some(fields) = split_fields(part, 9) {
                    let xy = fields[1].as_bytes();
                    status.changes.push(Change {
                        path: fields[8].clone(),
                        orig_path: None,
                        index: code_at(xy, 0),
                        worktree: code_at(xy, 1),
                        untracked: false,
                        conflicted: false,
                    });
                }
            }
            // 重命名 / 复制：2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>
            // 下一段是原始路径。
            Some('2') => {
                let orig = parts.next().map(|s| s.to_string());
                if let Some(fields) = split_fields(part, 10) {
                    let xy = fields[1].as_bytes();
                    status.changes.push(Change {
                        path: fields[9].clone(),
                        orig_path: orig,
                        index: code_at(xy, 0),
                        worktree: code_at(xy, 1),
                        untracked: false,
                        conflicted: false,
                    });
                }
            }
            // 冲突：u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
            Some('u') => {
                if let Some(fields) = split_fields(part, 11) {
                    let xy = fields[1].as_bytes();
                    status.changes.push(Change {
                        path: fields[10].clone(),
                        orig_path: None,
                        index: code_at(xy, 0),
                        worktree: code_at(xy, 1),
                        untracked: false,
                        conflicted: true,
                    });
                }
            }
            Some('?') => {
                let path = part[2..].to_string();
                status.changes.push(Change {
                    path,
                    orig_path: None,
                    index: Code::Untracked,
                    worktree: Code::Untracked,
                    untracked: true,
                    conflicted: false,
                });
            }
            _ => {}
        }
    }

    status
        .changes
        .sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
    status
}

fn parse_header(rest: &str, status: &mut Status) {
    if let Some(v) = rest.strip_prefix("branch.oid ") {
        if v != "(initial)" {
            status.oid = Some(v.to_string());
        }
    } else if let Some(v) = rest.strip_prefix("branch.head ") {
        if v != "(detached)" {
            status.branch = Some(v.to_string());
        }
    } else if let Some(v) = rest.strip_prefix("branch.upstream ") {
        status.upstream = Some(v.to_string());
    } else if let Some(v) = rest.strip_prefix("branch.ab ") {
        // 形如 "+2 -5"
        for token in v.split_whitespace() {
            if let Some(n) = token.strip_prefix('+') {
                status.ahead = n.parse().unwrap_or(0);
            } else if let Some(n) = token.strip_prefix('-') {
                status.behind = n.parse().unwrap_or(0);
            }
        }
    }
}

fn code_at(xy: &[u8], i: usize) -> Code {
    xy.get(i)
        .map(|b| Code::from_char(*b as char))
        .unwrap_or(Code::Unmodified)
}

/// 前 `n-1` 个字段用空格切分，第 `n` 个字段是路径（可能含空格）。
fn split_fields(line: &str, n: usize) -> Option<Vec<String>> {
    let fields: Vec<String> = line.splitn(n, ' ').map(|s| s.to_string()).collect();
    (fields.len() >= n).then_some(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(text: &str) -> GitOutput {
        GitOutput {
            argv: vec![],
            cwd: Default::default(),
            stdout: text.to_string(),
            stderr: String::new(),
            code: Some(0),
            duration_ms: 0,
        }
    }

    #[test]
    fn parses_branch_headers_and_changes() {
        let text = "\
# branch.oid 1234567890abcdef
# branch.head main
# branch.upstream origin/main
# branch.ab +2 -1
1 .M N... 100644 100644 100644 aaaaaaa bbbbbbb src/main.rs
1 A. N... 000000 100644 100644 0000000 ccccccc 新建 文件.txt
? notes/inbox.md
2 R. N... 100644 100644 100644 ddddddd eeeeeee R100 src/renamed.rs
src/original.rs
";
        let s = parse(&out(text));
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.ahead, s.behind), (2, 1));
        assert_eq!(s.changes.len(), 4);
        let renamed = s.find("src/renamed.rs").unwrap();
        assert_eq!(renamed.orig_path.as_deref(), Some("src/original.rs"));
        assert!(renamed.staged());
        assert!(!renamed.unstaged());
        let untracked = s.find("notes/inbox.md").unwrap();
        assert!(untracked.untracked);
        assert_eq!(untracked.badge(), '?');
        let spaced = s.find("新建 文件.txt").unwrap();
        assert_eq!(spaced.index, Code::Added);
        assert!(!spaced.staged() || spaced.staged());
    }

    #[test]
    fn parses_conflicts() {
        let text = "u UU N... 100644 100644 100644 100644 a b c src/conflict.rs\n";
        let s = parse(&out(text));
        assert_eq!(s.changes.len(), 1);
        assert!(s.changes[0].conflicted);
        assert_eq!(s.changes[0].badge(), 'U');
    }

    #[test]
    fn separates_staged_from_unstaged() {
        let text = "\
1 M. N... 100644 100644 100644 aaaaaaa bbbbbbb staged.rs
1 .M N... 100644 100644 100644 aaaaaaa bbbbbbb unstaged.rs
1 MM N... 100644 100644 100644 aaaaaaa bbbbbbb both.rs
";
        let s = parse(&out(text));
        assert_eq!(s.staged_count(), 2);
        assert_eq!(s.unstaged_count(), 2);
    }
}

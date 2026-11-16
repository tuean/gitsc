//! gitsc 的 Git 数据层。
//!
//! 所有能力都建立在 `git` 可执行文件之上——和 IDEA 用 JGit 不同，这里直接复用
//! 用户自己的 Git：凭据助手、hooks、`includeIf`、rebase 配置都原样生效，同时
//! 每条命令都能原样展示在 Console 面板里。

pub mod command;
pub mod diff;
pub mod graph;
pub mod log;
pub mod ops;
pub mod settings;
pub mod status;
pub mod tags;

pub use command::{GitError, GitOutput};
pub use log::{Commit, CommitRef, RefKind};
pub use status::{Change, Code, Status};

use std::path::{Path, PathBuf};

/// 一次 Git 会话：绑定仓库根目录，并记录这次会话里跑过的每条命令。
///
/// UI 的每个后台任务都会新建一个 `Git`，任务结束时把 [`Git::transcript`]
/// 交给 Console 面板。
pub struct Git {
    root: PathBuf,
    transcript: Vec<GitOutput>,
}

impl Git {
    /// 在 `path`（可以是仓库内任意子目录）上建立会话。
    pub fn open(path: &Path) -> Result<Self, GitError> {
        let out = command::run(path, &["rev-parse", "--show-toplevel"])?;
        if !out.ok() {
            return Err(GitError::NotARepository {
                path: path.display().to_string(),
                stderr: out.stderr.trim().to_string(),
            });
        }
        let root = PathBuf::from(out.stdout.trim());
        Ok(Self {
            root,
            transcript: vec![out],
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.display().to_string())
    }

    pub fn transcript(&self) -> &[GitOutput] {
        &self.transcript
    }

    pub fn take_transcript(&mut self) -> Vec<GitOutput> {
        std::mem::take(&mut self.transcript)
    }

    /// 执行一条命令并记录；不检查退出码。
    pub fn run(&mut self, args: &[&str]) -> GitOutput {
        let out = command::run(&self.root, args)
            .unwrap_or_else(|e| GitOutput::failure(&self.root, args, &e.to_string()));
        self.transcript.push(out.clone());
        out
    }

    /// 执行一条命令；失败时返回带 stderr 的错误。
    pub fn run_ok(&mut self, args: &[&str]) -> Result<GitOutput, GitError> {
        let out = self.run(args);
        if out.ok() {
            Ok(out)
        } else {
            Err(GitError::Failed {
                command: out.command_line(),
                code: out.code.unwrap_or(-1),
                stderr: out.stderr.trim().to_string(),
            })
        }
    }

    pub fn status(&mut self) -> Result<Status, GitError> {
        status::read(self)
    }

    pub fn log(&mut self, limit: usize, all_branches: bool) -> Result<Vec<Commit>, GitError> {
        log::read(self, limit, all_branches)
    }

    pub fn diff(&mut self, path: &str, staged: bool) -> Result<diff::DiffDoc, GitError> {
        diff::read_worktree(self, path, staged)
    }

    pub fn commit_diff(&mut self, oid: &str) -> Result<diff::DiffDoc, GitError> {
        diff::read_commit(self, oid)
    }

    pub fn commit_body(&mut self, oid: &str) -> Result<String, GitError> {
        let out = self.run_ok(&["show", "-s", "--format=%B", oid])?;
        Ok(out.stdout.trim_end().to_string())
    }

    pub fn branches(&mut self) -> Result<Vec<tags::Branch>, GitError> {
        tags::branches(self)
    }

    pub fn stashes(&mut self) -> Result<Vec<tags::Stash>, GitError> {
        tags::stashes(self)
    }

    /// 短 hash 对应的完整 oid。
    pub fn resolve(&mut self, rev: &str) -> Result<String, GitError> {
        let out = self.run_ok(&["rev-parse", "--verify", &format!("{rev}^{{commit}}")])?;
        Ok(out.stdout.trim().to_string())
    }
}

/// 应用配置目录：`~/Library/Application Support/gitsc`（macOS）。
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gitsc")
}

pub fn git_version() -> String {
    command::run(Path::new("."), &["--version"])
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_else(|_| "git 不可用".to_string())
}

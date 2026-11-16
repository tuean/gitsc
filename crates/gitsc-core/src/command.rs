//! 执行 `git` 子进程。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// 一次命令运行的结果。stdout / stderr 已按 UTF-8 有损解码。
#[derive(Clone, Debug)]
pub struct GitOutput {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub duration_ms: u128,
}

impl GitOutput {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// Console 里展示的一行命令（不含开头的 `git`）。
    pub fn command_line(&self) -> String {
        let mut s = String::from("git");
        for a in &self.argv {
            if a.is_empty() || a.contains(' ') {
                s.push_str(&format!(" \"{a}\""));
            } else {
                s.push(' ');
                s.push_str(a);
            }
        }
        s
    }

    pub fn failure(cwd: &Path, args: &[&str], message: &str) -> Self {
        Self {
            argv: args.iter().map(|s| s.to_string()).collect(),
            cwd: cwd.to_path_buf(),
            stdout: String::new(),
            stderr: message.to_string(),
            code: None,
            duration_ms: 0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("找不到 git：{0}")]
    NotFound(String),
    #[error("{path} 不在 Git 仓库中：{stderr}")]
    NotARepository { path: String, stderr: String },
    #[error("{command} 失败（退出码 {code}）：{stderr}")]
    Failed {
        command: String,
        code: i32,
        stderr: String,
    },
    #[error("读写失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

/// 在 `dir` 下运行 git。`GIT_TERMINAL_PROMPT=0` 保证网络操作不会卡在密码提示上
/// （没有凭据助手时立刻失败，而不是把后台线程挂死）。
pub fn run(dir: &Path, args: &[&str]) -> Result<GitOutput, GitError> {
    run_with_env(dir, args, &[])
}

pub fn run_with_env(
    dir: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<GitOutput, GitError> {
    let started = Instant::now();
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    #[cfg(windows)]
    {
        // 不弹出黑色控制台窗口；CREATE_NO_WINDOW = 0x08000000。
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(0x0800_0000);
    }
    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GitError::NotFound(e.to_string())
        } else {
            GitError::Io(e)
        }
    })?;
    Ok(GitOutput {
        argv: args.iter().map(|s| s.to_string()).collect(),
        cwd: dir.to_path_buf(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        code: output.status.code(),
        duration_ms: started.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_quotes_arguments_with_spaces() {
        let out = GitOutput {
            argv: vec!["log".into(), "--format=%s".into()],
            cwd: PathBuf::from("."),
            stdout: String::new(),
            stderr: String::new(),
            code: Some(0),
            duration_ms: 0,
        };
        assert_eq!(out.command_line(), "git log --format=%s");
    }

    #[test]
    fn runs_git_version_anywhere() {
        let out = run(Path::new("."), &["--version"]).expect("git installed");
        assert!(out.ok());
        assert!(out.stdout.starts_with("git version"));
    }
}

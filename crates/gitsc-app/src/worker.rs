//! 后台 Git 工作者：单线程串行执行所有 Git 调用。
//!
//! UI 线程永不阻塞：每个操作变成一条 `Job`，结果以 `Payload` 回到界面；
//! 同时把这次执行产生的命令流水（`GitOutput`）带回来给 Console 面板。

use std::path::{Path, PathBuf};

use flume::{Receiver, Sender};
use gitsc_core::command::{GitError, GitOutput};
use gitsc_core::diff::{self, DiffDoc};
use gitsc_core::log::{self, Commit};
use gitsc_core::ops::{self, OpResult};
use gitsc_core::status::Status;
use gitsc_core::tags::{self, Branch, RemoteInfo, Stash, Tag};
use gitsc_core::Git;

pub type FileStat = (String, Option<u32>, Option<u32>);

#[derive(Clone, Debug, PartialEq)]
pub enum DiffKey {
    /// 工作区/暂存区里的一个文件。
    Worktree { path: String, staged: bool },
    /// 某个提交里的单个文件。
    CommitFile { oid: String, path: String },
    /// 某个 stash 的整体改动。
    Stash(usize),
    /// 两个版本之间单个文件的差异。
    RangeFile {
        from: String,
        to: String,
        path: String,
    },
}

#[derive(Clone, Debug)]
pub enum Job {
    OpenRepo(PathBuf),
    InitRepo(PathBuf),
    CloneRepo { url: String, dest: PathBuf },
    Refresh,
    Log { limit: usize, all: bool },
    CommitDetail(String),
    Diff(DiffKey),
    Branches,
    Stashes,
    Tags,
    Remotes,
    /// 两个版本之间的文件列表 + 整段差异（对比页）。
    Compare { from: String, to: String },
    /// 当前分支相对某个远端分支的 ahead/behind。
    AheadBehind { left: String, right: String },
    Act(Action),
}

/// 所有会改动仓库（或远端）的操作。名字直接写进 Console 和 Toast。
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    StageAll,
    UnstageAll,
    Discard(Vec<String>),
    Commit { message: String, amend: bool },
    Checkout { name: String, auto_stash: bool },
    CheckoutDetach(String),
    CreateBranch { name: String, start: String },
    DeleteBranch(String),
    RenameBranch { old: String, new: String },
    Merge(String),
    Rebase(String),
    AbortMerge,
    RebaseContinue,
    RebaseAbort,
    CherryPick(String),
    Revert(String),
    Reset { oid: String, hard: bool },
    Fetch,
    FetchRemote(String),
    Pull,
    PullFrom { remote: String, branch: String, rebase: bool },
    Push,
    StashPush { message: String, include_untracked: bool },
    StashApply(usize),
    StashPop(usize),
    StashDrop(usize),
    CreateTag { name: String, target: String },
    DeleteTag(String),
    PushTag(String),
}

impl Action {
    pub fn label(&self) -> String {
        match self {
            Action::Stage(f) => format!("暂存 {} 个文件", f.len()),
            Action::Unstage(f) => format!("取消暂存 {} 个文件", f.len()),
            Action::StageAll => "暂存全部".into(),
            Action::UnstageAll => "取消暂存全部".into(),
            Action::Discard(f) => format!("丢弃 {} 个文件的改动", f.len()),
            Action::Commit { amend, .. } => {
                if *amend {
                    "修正上次提交".into()
                } else {
                    "提交".into()
                }
            }
            Action::Checkout { name, .. } => format!("切换到 {name}"),
            Action::CheckoutDetach(oid) => format!("检出 {}", short(oid)),
            Action::CreateBranch { name, .. } => format!("新建分支 {name}"),
            Action::DeleteBranch(name) => format!("删除分支 {name}"),
            Action::RenameBranch { old, new } => format!("重命名 {old} → {new}"),
            Action::Merge(name) => format!("合并 {name}"),
            Action::Rebase(onto) => format!("变基到 {onto}"),
            Action::AbortMerge => "中止合并".into(),
            Action::RebaseContinue => "继续变基".into(),
            Action::RebaseAbort => "中止变基".into(),
            Action::CherryPick(oid) => format!("拣选 {}", short(oid)),
            Action::Revert(oid) => format!("还原 {}", short(oid)),
            Action::Reset { oid, hard } => {
                if *hard {
                    format!("硬重置到 {}", short(oid))
                } else {
                    format!("软重置到 {}", short(oid))
                }
            }
            Action::Fetch => "获取远端更新".into(),
            Action::FetchRemote(remote) => format!("从 {remote} 获取更新"),
            Action::Pull => "拉取".into(),
            Action::PullFrom { remote, branch, .. } => format!("拉取 {remote}/{branch}"),
            Action::Push => "推送".into(),
            Action::StashPush { .. } => "贮藏改动".into(),
            Action::StashApply(i) => format!("应用 stash@{{{i}}}"),
            Action::StashPop(i) => format!("弹出 stash@{{{i}}}"),
            Action::StashDrop(i) => format!("删除 stash@{{{i}}}"),
            Action::CreateTag { name, .. } => format!("打标签 {name}"),
            Action::DeleteTag(name) => format!("删除标签 {name}"),
            Action::PushTag(name) => format!("推送标签 {name}"),
        }
    }
}

fn short(oid: &str) -> &str {
    &oid[..oid.len().min(7)]
}

#[derive(Debug)]
pub enum Payload {
    Opened { path: Result<PathBuf, String>, console: Vec<GitOutput> },
    Status(Result<Status, String>),
    Log(Result<Vec<Commit>, String>),
    Detail {
        oid: String,
        result: Result<(String, Vec<FileStat>), String>,
    },
    Diff {
        key: DiffKey,
        doc: Result<DiffDoc, String>,
    },
    Branches(Result<Vec<Branch>, String>),
    Stashes(Result<Vec<Stash>, String>),
    Tags(Result<Vec<Tag>, String>),
    Remotes(Result<Vec<RemoteInfo>, String>),
    Compare {
        from: String,
        to: String,
        result: Result<(Vec<FileStat>, DiffDoc), String>,
    },
    AheadBehind {
        left: String,
        right: String,
        result: Result<(usize, usize), String>,
    },
    Acted { label: String, result: Result<String, String> },
    Console(Vec<GitOutput>),
    Fatal(String),
}

pub fn spawn() -> (Sender<Job>, Receiver<Payload>) {
    let (job_tx, job_rx) = flume::unbounded::<Job>();
    let (out_tx, out_rx) = flume::unbounded::<Payload>();
    std::thread::Builder::new()
        .name("gitsc-git".into())
        .spawn(move || run_loop(job_rx, out_tx))
        .expect("无法启动 Git 后台线程");
    (job_tx, out_rx)
}

fn run_loop(rx: Receiver<Job>, tx: Sender<Payload>) {
    let mut root: Option<PathBuf> = None;
    while let Ok(job) = rx.recv() {
        match job {
            Job::OpenRepo(path) => {
                let mut git = match Git::open(&path) {
                    Ok(git) => git,
                    Err(err) => {
                        let _ = tx.send(Payload::Opened {
                            path: Err(err.to_string()),
                            console: Vec::new(),
                        });
                        continue;
                    }
                };
                root = Some(git.root().to_path_buf());
                let console = trim_open_console(git.take_transcript());
                let _ = tx.send(Payload::Opened {
                    path: Ok(git.root().to_path_buf()),
                    console,
                });
            }
            Job::InitRepo(path) => {
                let result = ops::init(&path).and_then(|_| {
                    Git::open(&path)
                        .map(|g| g.root().to_path_buf())
                        .map_err(|e| GitError::Message(e.to_string()))
                });
                match result {
                    Ok(new_root) => {
                        root = Some(new_root.clone());
                        let _ = tx.send(Payload::Opened {
                            path: Ok(new_root),
                            console: Vec::new(),
                        });
                    }
                    Err(err) => {
                        let _ = tx.send(Payload::Fatal(err.to_string()));
                    }
                }
            }
            Job::CloneRepo { url, dest } => match ops::clone(&url, &dest) {
                Ok(msg) => match Git::open(&dest) {
                    Ok(mut git) => {
                        let console = trim_open_console(git.take_transcript());
                        root = Some(git.root().to_path_buf());
                        let _ = tx.send(Payload::Console(console));
                        let _ = tx.send(Payload::Opened {
                            path: Ok(git.root().to_path_buf()),
                            console: Vec::new(),
                        });
                        let _ = tx.send(Payload::Acted {
                            label: msg,
                            result: Ok("克隆完成".into()),
                        });
                    }
                    Err(err) => {
                        let _ = tx.send(Payload::Fatal(err.to_string()));
                    }
                },
                Err(err) => {
                    let _ = tx.send(Payload::Acted {
                        label: "克隆".into(),
                        result: Err(err.to_string()),
                    });
                }
            },
            Job::Act(action) => {
                let label = action.label();
                let Some(dir) = root.clone() else {
                    let _ = tx.send(Payload::Fatal("还没有打开仓库".into()));
                    continue;
                };
                match Git::open(&dir) {
                    Ok(mut git) => {
                        let _ = git.take_transcript();
                        let result = run_action(&mut git, &action);
                        let console = git.take_transcript();
                        if !console.is_empty() {
                            let _ = tx.send(Payload::Console(console));
                        }
                        let _ = tx.send(Payload::Acted {
                            label,
                            result: result.map_err(|e| e.to_string()),
                        });
                    }
                    Err(err) => {
                        let _ = tx.send(Payload::Acted {
                            label,
                            result: Err(err.to_string()),
                        });
                    }
                }
            }
            other => {
                let Some(dir) = root.clone() else {
                    let _ = tx.send(Payload::Fatal("还没有打开仓库".into()));
                    continue;
                };
                let opened = Git::open(&dir);
                let mut git = match opened {
                    Ok(git) => git,
                    Err(err) => {
                        let _ = tx.send(Payload::Fatal(err.to_string()));
                        continue;
                    }
                };
                let _ = git.take_transcript();
                let payload = match other {
                    Job::Refresh => Payload::Status(git.status().map_err(|e| e.to_string())),
                    Job::Log { limit, all } => {
                        Payload::Log(log::read(&mut git, limit, all).map_err(|e| e.to_string()))
                    }
                    Job::CommitDetail(oid) => {
                        let body = git.commit_body(&oid).unwrap_or_default();
                        let files = diff::numstat(&mut git, &oid).map_err(|e| e.to_string());
                        Payload::Detail {
                            oid,
                            result: files.map(|f| (body, f)),
                        }
                    }
                    Job::Diff(key) => {
                        let doc = match &key {
                            DiffKey::Worktree { path, staged } => diff::read_worktree(&mut git, path, *staged),
                            DiffKey::CommitFile { oid, path } => {
                                diff::read_commit_file(&mut git, oid, path)
                            }
                            DiffKey::Stash(index) => ops::stash_show(&mut git, *index),
                            DiffKey::RangeFile { from, to, path } => {
                                let to_ref = if to == "工作区" { None } else { Some(to.as_str()) };
                                diff::read_range_file(&mut git, from, to_ref, path)
                            }
                        };
                        Payload::Diff {
                            key,
                            doc: doc.map_err(|e| e.to_string()),
                        }
                    }
                    Job::Branches => {
                        Payload::Branches(tags::branches(&mut git).map_err(|e| e.to_string()))
                    }
                    Job::Stashes => {
                        Payload::Stashes(tags::stashes(&mut git).map_err(|e| e.to_string()))
                    }
                    Job::Tags => Payload::Tags(tags::tags(&mut git).map_err(|e| e.to_string())),
                    Job::Remotes => {
                        Payload::Remotes(tags::remotes(&mut git).map_err(|e| e.to_string()))
                    }
                    Job::Compare { from, to } => {
                        // UI 里的「工作区」不是一个 ref：git diff <from> 不带第二参数即可。
                        let to_ref = if to == "工作区" { None } else { Some(to.as_str()) };
                        let files = diff::numstat_range(&mut git, &from, to_ref)
                            .map_err(|e| e.to_string());
                        let doc = diff::read_range(&mut git, &from, to_ref)
                            .map_err(|e| e.to_string());
                        let result = match (files, doc) {
                            (Ok(files), Ok(doc)) => Ok((files, doc)),
                            (Err(err), _) | (_, Err(err)) => Err(err),
                        };
                        Payload::Compare { from, to, result }
                    }
                    Job::AheadBehind { left, right } => Payload::AheadBehind {
                        result: ops::ahead_behind(&mut git, &left, &right)
                            .map_err(|e| e.to_string()),
                        left,
                        right,
                    },
                    _ => continue,
                };
                let console = git.take_transcript();
                let _ = tx.send(payload);
                if !console.is_empty() {
                    let _ = tx.send(Payload::Console(console));
                }
            }
        }
    }
}

/// `Git::open` 的第一条 rev-parse 只是探测仓库，不需要出现在 Console 里。
fn trim_open_console(mut console: Vec<GitOutput>) -> Vec<GitOutput> {
    if console
        .first()
        .map(|o| o.argv.first().map(String::as_str) == Some("rev-parse"))
        .unwrap_or(false)
    {
        console.remove(0);
    }
    console
}

fn run_action(git: &mut Git, action: &Action) -> OpResult {
    match action {
        Action::Stage(files) => ops::stage(git, files),
        Action::Unstage(files) => ops::unstage(git, files),
        Action::StageAll => ops::stage_all(git),
        Action::UnstageAll => ops::unstage_all(git),
        Action::Discard(files) => ops::discard(git, files),
        Action::Commit { message, amend } => ops::commit(git, message, *amend),
        Action::Checkout { name, auto_stash } => ops::checkout_branch_auto(git, name, *auto_stash),
        Action::CheckoutDetach(oid) => ops::checkout_commit(git, oid),
        Action::CreateBranch { name, start } => ops::create_branch(git, name, Some(start)),
        Action::DeleteBranch(name) => ops::delete_branch(git, name, false),
        Action::RenameBranch { old, new } => ops::rename_branch(git, old, new),
        Action::Merge(name) => ops::merge(git, name, false),
        Action::Rebase(onto) => ops::rebase(git, onto),
        Action::AbortMerge => ops::abort_merge(git),
        Action::RebaseContinue => ops::rebase_continue(git),
        Action::RebaseAbort => ops::rebase_abort(git),
        Action::CherryPick(oid) => ops::cherry_pick(git, oid),
        Action::Revert(oid) => ops::revert(git, oid),
        Action::Reset { oid, hard } => ops::reset(git, oid, *hard),
        Action::Fetch => ops::fetch(git, true),
        Action::FetchRemote(remote) => ops::fetch_remote(git, remote, true),
        Action::Pull => ops::pull(git, false),
        Action::PullFrom { remote, branch, rebase } => {
            ops::pull_from(git, remote, branch, *rebase)
        }
        Action::Push => {
            let branch = git
                .status()
                .ok()
                .and_then(|s| s.branch)
                .unwrap_or_default();
            if branch.is_empty() {
                return Err(GitError::Message("当前不在任何分支上，无法推送".into()));
            }
            let has_upstream = git
                .run(&["rev-parse", "--abbrev-ref", &format!("{branch}@{{upstream}}")])
                .code
                == Some(0);
            ops::push(git, &branch, has_upstream, false)
        }
        Action::StashPush { message, include_untracked } => {
            ops::stash_push(git, message, *include_untracked)
        }
        Action::StashApply(i) => ops::stash_apply(git, *i, false),
        Action::StashPop(i) => ops::stash_apply(git, *i, true),
        Action::StashDrop(i) => ops::stash_drop(git, *i),
        Action::CreateTag { name, target } => ops::create_tag(git, name, Some(target), ""),
        Action::DeleteTag(name) => ops::delete_tag(git, name),
        Action::PushTag(name) => ops::push_tag(git, name),
    }
}

/// 供 UI 使用：把任意路径变成展示名。
pub fn repo_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

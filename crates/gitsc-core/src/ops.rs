//! 高层 Git 操作——UI 的每个动作最终都落到这里。
//!
//! 全部参数都直接传给 `git`，不经过 shell，所以路径里有空格、中文、引号都不用转义。
//! 每个函数返回一句可以直接显示在状态栏里的结果。

use std::path::Path;

use crate::command::GitError;
use crate::Git;

pub type OpResult = Result<String, GitError>;

fn ok(msg: impl Into<String>) -> OpResult {
    Ok(msg.into())
}

fn paths<'a>(prefix: &[&'a str], files: &'a [String]) -> Vec<&'a str> {
    let mut args: Vec<&str> = prefix.to_vec();
    args.push("--");
    args.extend(files.iter().map(String::as_str));
    args
}

// ---------------------------------------------------------------- 暂存区

pub fn stage(git: &mut Git, files: &[String]) -> OpResult {
    if files.is_empty() {
        return ok("没有要暂存的文件");
    }
    git.run_ok(&paths(&["add"], files))?;
    ok(count(files.len(), "已暂存"))
}

pub fn unstage(git: &mut Git, files: &[String]) -> OpResult {
    if files.is_empty() {
        return ok("没有要取消暂存的文件");
    }
    if head_exists(git) {
        git.run_ok(&paths(&["restore", "--staged"], files))?;
    } else {
        // 还没有第一次提交时没有 HEAD，restore 不能用。
        git.run_ok(&paths(&["rm", "--cached", "-r", "-q"], files))?;
    }
    ok(count(files.len(), "已取消暂存"))
}

pub fn stage_all(git: &mut Git) -> OpResult {
    git.run_ok(&["add", "-A"])?;
    ok("已暂存全部改动")
}

pub fn unstage_all(git: &mut Git) -> OpResult {
    if head_exists(git) {
        git.run_ok(&["reset", "-q"])?;
    } else {
        git.run_ok(&["rm", "--cached", "-r", "-q", "--ignore-unmatch", "."])?;
    }
    ok("已取消全部暂存")
}

/// 丢弃工作区改动（已跟踪文件恢复成 HEAD，未跟踪文件直接删除）。
pub fn discard(git: &mut Git, files: &[String]) -> OpResult {
    if files.is_empty() {
        return ok("没有要丢弃的改动");
    }
    let tracked: Vec<String> = files
        .iter()
        .filter(|f| is_tracked(git, f))
        .cloned()
        .collect();
    let untracked: Vec<String> = files
        .iter()
        .filter(|f| !tracked.contains(f))
        .cloned()
        .collect();

    if !tracked.is_empty() {
        git.run_ok(&paths(&["restore", "--worktree", "--source=HEAD"], &tracked))?;
    }
    for file in &untracked {
        let full = git.root().join(file);
        // 只删文件或目录本身，路径来自 status，不跟随符号链接。
        if full.is_dir() {
            std::fs::remove_dir_all(&full)
                .map_err(|e| GitError::Message(format!("删除 {} 失败：{e}", full.display())))?;
        } else {
            std::fs::remove_file(&full)
                .map_err(|e| GitError::Message(format!("删除 {} 失败：{e}", full.display())))?;
        }
    }
    ok(count(files.len(), "已丢弃"))
}

fn is_tracked(git: &mut Git, file: &str) -> bool {
    let out = git.run(&["ls-files", "--error-unmatch", "--", file]);
    out.ok()
}

fn head_exists(git: &mut Git) -> bool {
    git.run(&["rev-parse", "--verify", "-q", "HEAD"]).ok()
}

// ---------------------------------------------------------------- 提交

pub fn commit(git: &mut Git, message: &str, amend: bool) -> OpResult {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::Message("提交信息不能为空".into()));
    }
    let mut args = vec!["commit", "-m", message];
    if amend {
        args.push("--amend");
    }
    git.run_ok(&args)?;
    let oid = git.run(&["rev-parse", "--short", "HEAD"]);
    let head = oid.stdout.trim().to_string();
    let subject = if amend { "已修正上一次提交" } else { "已提交" };
    if head.is_empty() {
        return ok(subject);
    }
    ok(format!("{subject} {head}"))
}

pub fn commit_staged_with_message_file(git: &mut Git, path: &Path) -> OpResult {
    let p = path.to_string_lossy().to_string();
    git.run_ok(&["commit", "-F", &p])?;
    ok("已提交")
}

/// 提交正文（`%B`），用于在列表里展开多行信息。
pub fn last_commit_message(git: &mut Git) -> String {
    git.run(&["log", "-1", "--format=%B"]).stdout.trim_end().to_string()
}

// ---------------------------------------------------------------- 分支

pub fn checkout_branch(git: &mut Git, name: &str) -> OpResult {
    let out = git.run_ok(&["checkout", name])?;
    let detail = first_line(&out.stderr);
    ok(if detail.is_empty() {
        format!("已切换到 {name}")
    } else {
        detail
    })
}

/// 切分支；本地改动挡住 checkout 时按需自动 stash，切完再恢复。
pub fn checkout_branch_auto(git: &mut Git, name: &str, auto_stash: bool) -> OpResult {
    let first = git.run(&["checkout", name]);
    if first.ok() {
        return ok(format!("已切换到 {name}"));
    }
    let blocked = first.stderr.contains("would be overwritten")
        || first.stderr.contains("local changes")
        || first.stderr.contains("Please commit your changes or stash them");
    if !(auto_stash && blocked) {
        return Err(GitError::Failed {
            command: first.command_line(),
            code: first.code.unwrap_or(-1),
            stderr: first.stderr.trim().to_string(),
        });
    }
    let stashed = stash_if_dirty(git)?;
    let retry = git.run(&["checkout", name]);
    if !retry.ok() {
        let restore = if stashed.is_some() {
            format!("；{}", pop_auto_stash(git))
        } else {
            String::new()
        };
        return Err(GitError::Message(format!(
            "切换 {name} 失败{restore}：{}",
            first_line(&retry.stderr)
        )));
    }
    let note = if stashed.is_some() {
        format!("；{}", pop_auto_stash(git))
    } else {
        String::new()
    };
    ok(format!("已切换到 {name}（自动贮藏）{note}"))
}

pub fn checkout_commit(git: &mut Git, oid: &str) -> OpResult {
    git.run_ok(&["checkout", "--detach", oid])?;
    ok(format!("已检出 {}", crate::log::short_oid(oid)))
}

pub fn create_branch(git: &mut Git, name: &str, start_point: Option<&str>) -> OpResult {
    let name = name.trim();
    validate_ref_name(name)?;
    let mut args = vec!["branch", name];
    if let Some(start) = start_point {
        args.push(start);
    }
    git.run_ok(&args)?;
    ok(format!("已创建分支 {name}"))
}

pub fn delete_branch(git: &mut Git, name: &str, force: bool) -> OpResult {
    let flag = if force { "-D" } else { "-d" };
    git.run_ok(&["branch", flag, name])?;
    ok(format!("已删除分支 {name}"))
}

pub fn rename_branch(git: &mut Git, old: &str, new: &str) -> OpResult {
    validate_ref_name(new)?;
    git.run_ok(&["branch", "-m", old, new])?;
    ok(format!("已重命名 {old} → {new}"))
}

pub fn merge(git: &mut Git, name: &str, no_ff: bool) -> OpResult {
    let mut args = vec!["merge"];
    if no_ff {
        args.push("--no-ff");
    }
    args.push(name);
    let out = git.run(&args);
    if out.ok() {
        return ok(format!("已合并 {name}"));
    }
    // 冲突是正常状态，不是错误：把冲突文件列出来交给 UI。
    if out.stderr.contains("CONFLICT") || out.stdout.contains("CONFLICT") {
        return ok(format!("合并 {name} 产生冲突，请解决后提交"));
    }
    Err(GitError::Failed {
        command: out.command_line(),
        code: out.code.unwrap_or(-1),
        stderr: out.stderr.trim().to_string(),
    })
}

pub fn abort_merge(git: &mut Git) -> OpResult {
    git.run_ok(&["merge", "--abort"])?;
    ok("已中止合并")
}

pub fn rebase(git: &mut Git, onto: &str) -> OpResult {
    let out = git.run(&["rebase", onto]);
    if out.ok() {
        return ok(format!("已把当前分支变基到 {onto}"));
    }
    if out.stderr.contains("CONFLICT") {
        return ok("变基产生冲突，解决后执行「继续变基」".to_string());
    }
    Err(GitError::Failed {
        command: out.command_line(),
        code: out.code.unwrap_or(-1),
        stderr: out.stderr.trim().to_string(),
    })
}

pub fn rebase_continue(git: &mut Git) -> OpResult {
    git.run_ok(&["rebase", "--continue"])?;
    ok("变基已继续")
}

pub fn rebase_abort(git: &mut Git) -> OpResult {
    git.run_ok(&["rebase", "--abort"])?;
    ok("已中止变基")
}

pub fn cherry_pick(git: &mut Git, oid: &str) -> OpResult {
    let out = git.run(&["cherry-pick", oid]);
    if out.ok() {
        return ok(format!("已拣选 {}", crate::log::short_oid(oid)));
    }
    if out.stderr.contains("CONFLICT") {
        return ok("拣选产生冲突，请解决后提交");
    }
    Err(GitError::Failed {
        command: out.command_line(),
        code: out.code.unwrap_or(-1),
        stderr: out.stderr.trim().to_string(),
    })
}

pub fn revert(git: &mut Git, oid: &str) -> OpResult {
    let out = git.run(&["revert", "--no-edit", oid]);
    if out.ok() {
        return ok(format!("已还原 {}", crate::log::short_oid(oid)));
    }
    if out.stderr.contains("CONFLICT") {
        return ok("还原产生冲突，请解决后提交");
    }
    Err(GitError::Failed {
        command: out.command_line(),
        code: out.code.unwrap_or(-1),
        stderr: out.stderr.trim().to_string(),
    })
}

/// `reset --mixed`（回退提交但保留改动）/ `--hard`（连改动一起丢）。
pub fn reset(git: &mut Git, oid: &str, hard: bool) -> OpResult {
    let mode = if hard { "--hard" } else { "--mixed" };
    git.run_ok(&["reset", mode, oid])?;
    ok(format!("已重置到 {}", crate::log::short_oid(oid)))
}

// ---------------------------------------------------------------- 远端

pub fn fetch(git: &mut Git, prune: bool) -> OpResult {
    let mut args = vec!["fetch", "--all", "--tags"];
    if prune {
        args.push("--prune");
    }
    git.run_ok(&args)?;
    ok("已获取远端更新")
}

/// 只抓取指定远端（不动工作区）。
pub fn fetch_remote(git: &mut Git, remote: &str, prune: bool) -> OpResult {
    let mut args = vec!["fetch", "--tags"];
    if prune {
        args.push("--prune");
    }
    args.push(remote);
    git.run_ok(&args)?;
    ok(format!("已从 {remote} 获取更新"))
}

/// 把别的远端分支拉进当前分支（IDEA 的「Pull from…」）。
pub fn pull_from(git: &mut Git, remote: &str, branch: &str, rebase: bool) -> OpResult {
    let args = if rebase {
        vec!["pull", "--rebase", "--autostash", remote, branch]
    } else {
        vec!["pull", "--no-rebase", "--autostash", remote, branch]
    };
    let out = git.run(&args);
    if out.ok() {
        // --autostash 在「拉取成功、但贮藏的改动 pop 回来冲突」时 git 仍返回 0，
        // 只有工作区里留着未合并条目。别把它报成干净的成功。
        if has_unmerged(git) {
            return ok(format!(
                "已拉起 {remote}/{branch}，但自动贮藏的改动产生冲突，请解决后再提交"
            ));
        }
        return ok(format!("已把 {remote}/{branch} 拉进当前分支"));
    }
    if out.stderr.contains("CONFLICT") || out.stdout.contains("CONFLICT") {
        return ok(format!("{remote}/{branch} 与当前分支冲突，请解决后提交"));
    }
    if out.stderr.contains("would be overwritten") || out.stderr.contains("local changes") {
        return Err(GitError::Message(format!(
            "{remote}/{branch} 与本地改动冲突；可先提交或贮藏，再试一次"
        )));
    }
    Err(GitError::Failed {
        command: out.command_line(),
        code: out.code.unwrap_or(-1),
        stderr: out.stderr.trim().to_string(),
    })
}

fn has_unmerged(git: &mut Git) -> bool {
    let out = git.run(&["diff", "--name-only", "--diff-filter=U"]);
    out.ok() && !out.stdout.trim().is_empty()
}

/// 当前分支相对某个 ref 的 (ahead, behind)。
pub fn ahead_behind(git: &mut Git, left: &str, right: &str) -> Result<(usize, usize), GitError> {
    let spec = format!("{left}...{right}");
    let out = git.run_ok(&["rev-list", "--left-right", "--count", &spec])?;
    let mut parts = out.stdout.split_whitespace();
    let ahead = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let behind = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Ok((ahead, behind))
}

/// 工作区有未提交改动时压一条 stash，返回它的 oid；工作区干净则返回 None。
pub fn stash_if_dirty(git: &mut Git) -> Result<Option<String>, GitError> {
    if git.status()?.changes.is_empty() {
        return Ok(None);
    }
    git.run_ok(&["stash", "push", "-m", "gitsc: 自动贮藏"])?;
    let oid = git
        .run(&["rev-parse", "refs/stash"])
        .stdout
        .trim()
        .to_string();
    Ok(Some(oid))
}

/// 恢复自动 stash：成功后 pop 掉，冲突时保留在 stash 列表里。
fn pop_auto_stash(git: &mut Git) -> String {
    let out = git.run(&["stash", "pop"]);
    if out.ok() {
        "已恢复自动贮藏".to_string()
    } else {
        "自动贮藏恢复时冲突，改动仍留在贮藏列表里".to_string()
    }
}

pub fn pull(git: &mut Git, rebase: bool) -> OpResult {
    let args = if rebase {
        vec!["pull", "--rebase", "--autostash"]
    } else {
        vec!["pull", "--no-rebase", "--autostash"]
    };
    let out = git.run(&args);
    if out.ok() {
        return ok("已拉取");
    }
    if out.stderr.contains("CONFLICT") || out.stdout.contains("CONFLICT") {
        return ok("拉取产生冲突，请解决后提交");
    }
    Err(GitError::Failed {
        command: out.command_line(),
        code: out.code.unwrap_or(-1),
        stderr: out.stderr.trim().to_string(),
    })
}

/// 没有 upstream 时用 `-u origin <branch>` 建立跟踪。
pub fn push(git: &mut Git, branch: &str, has_upstream: bool, force_with_lease: bool) -> OpResult {
    let mut args = vec!["push"];
    if force_with_lease {
        args.push("--force-with-lease");
    }
    if !has_upstream {
        args.extend(["-u", "origin", branch]);
    }
    let out = git.run(&args);
    if out.ok() {
        return ok(if has_upstream {
            "已推送".to_string()
        } else {
            format!("已推送并跟踪 origin/{branch}")
        });
    }
    let stderr = out.stderr.trim();
    if stderr.contains("non-fast-forward") || stderr.contains("fetch first") {
        return ok("远端有新提交，请先拉取");
    }
    Err(GitError::Failed {
        command: out.command_line(),
        code: out.code.unwrap_or(-1),
        stderr: stderr.to_string(),
    })
}

// ---------------------------------------------------------------- stash

pub fn stash_push(git: &mut Git, message: &str, include_untracked: bool) -> OpResult {
    let mut args = vec!["stash", "push"];
    if include_untracked {
        args.push("--include-untracked");
    }
    if !message.trim().is_empty() {
        args.extend(["-m", message.trim()]);
    }
    let out = git.run_ok(&args)?;
    if out.stdout.contains("No local changes") {
        return ok("没有可暂存的改动");
    }
    ok("已暂存改动")
}

pub fn stash_apply(git: &mut Git, index: usize, pop: bool) -> OpResult {
    let reference = format!("stash@{{{index}}}");
    let verb = if pop { "pop" } else { "apply" };
    git.run_ok(&["stash", verb, &reference])?;
    ok(format!("已{} {}", if pop { "弹出" } else { "应用" }, reference))
}

pub fn stash_drop(git: &mut Git, index: usize) -> OpResult {
    git.run_ok(&["stash", "drop", &format!("stash@{{{index}}}")])?;
    ok("已删除该 stash")
}

pub fn stash_show(git: &mut Git, index: usize) -> Result<crate::diff::DiffDoc, GitError> {
    let reference = format!("stash@{{{index}}}");
    let out = git.run_ok(&[
        "stash",
        "show",
        "-p",
        "--no-color",
        "--no-ext-diff",
        "-U3",
        &reference,
    ])?;
    Ok(crate::diff::parse(&out))
}

// ---------------------------------------------------------------- tag

pub fn create_tag(git: &mut Git, name: &str, target: Option<&str>, message: &str) -> OpResult {
    validate_ref_name(name)?;
    let mut args = vec!["tag"];
    if !message.trim().is_empty() {
        args.extend(["-a", "-m", message.trim()]);
    }
    args.push(name);
    if let Some(target) = target {
        args.push(target);
    }
    git.run_ok(&args)?;
    ok(format!("已创建标签 {name}"))
}

pub fn delete_tag(git: &mut Git, name: &str) -> OpResult {
    git.run_ok(&["tag", "-d", name])?;
    ok(format!("已删除标签 {name}"))
}

pub fn push_tag(git: &mut Git, name: &str) -> OpResult {
    git.run_ok(&["push", "origin", name])?;
    ok(format!("已推送标签 {name}"))
}

// ---------------------------------------------------------------- 杂项

pub fn init(path: &Path) -> OpResult {
    let out = crate::command::run(path, &["init"])?;
    if out.ok() {
        ok("已初始化仓库")
    } else {
        Err(GitError::Failed {
            command: out.command_line(),
            code: out.code.unwrap_or(-1),
            stderr: out.stderr.trim().to_string(),
        })
    }
}

pub fn clone(url: &str, dest: &Path) -> OpResult {
    let dest_s = dest.to_string_lossy().to_string();
    let out = crate::command::run(Path::new("."), &["clone", url, &dest_s])?;
    if out.ok() {
        ok(format!("已克隆到 {dest_s}"))
    } else {
        Err(GitError::Failed {
            command: out.command_line(),
            code: out.code.unwrap_or(-1),
            stderr: out.stderr.trim().to_string(),
        })
    }
}

fn validate_ref_name(name: &str) -> Result<(), GitError> {
    if name.is_empty() {
        return Err(GitError::Message("名字不能为空".into()));
    }
    if name.starts_with('-') || name.contains(char::is_whitespace) {
        return Err(GitError::Message("名字里不能有空格或以 - 开头".into()));
    }
    Ok(())
}

fn count(n: usize, verb: &str) -> String {
    format!("{verb} {n} 个文件")
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_repo(name: &str) -> (PathBuf, Git) {
        let dir = std::env::temp_dir().join(format!("gitsc-ops-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        init(&dir).unwrap();
        let mut git = Git::open(&dir).unwrap();
        git.run_ok(&["config", "user.email", "test@example.com"]).unwrap();
        git.run_ok(&["config", "user.name", "gitsc test"]).unwrap();
        (dir, git)
    }

    #[test]
    fn stash_roundtrip_accepts_braced_reference() {
        let (dir, mut git) = temp_repo("stash");
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        stage_all(&mut git).unwrap();
        commit(&mut git, "first", false).unwrap();

        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        stash_push(&mut git, "wip", false).unwrap();
        let list = crate::tags::stashes(&mut git).unwrap();
        assert_eq!(list.len(), 1);

        // 回归：这里必须是 `stash@{0}`；写成 `stash@0` 会报 not a valid reference。
        let doc = stash_show(&mut git, 0).unwrap();
        assert!(doc.files.iter().any(|f| f.path() == "a.txt"));

        stash_apply(&mut git, 0, false).unwrap();
        let content = std::fs::read_to_string(dir.join("a.txt")).unwrap();
        assert_eq!(content, "one\ntwo\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 从另一个远端拉取：落后时能快进，脏工作区由 --autostash 保护。
    #[test]
    fn pull_from_other_remote_keeps_local_changes() {
        let base = std::env::temp_dir().join(format!("gitsc-pull-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let origin = base.join("origin.git");
        let g = |cwd: &std::path::Path, args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(cwd)
                .env("GIT_TERMINAL_PROMPT", "0")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).to_string()
        };

        g(&base, &["init", "--bare", "-q", "-b", "main", origin.to_str().unwrap()]);
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        g(&a, &["init", "-q", "-b", "main"]);
        g(&a, &["config", "user.email", "a@example.com"]);
        g(&a, &["config", "user.name", "A"]);
        std::fs::write(a.join("f.txt"), "one\n").unwrap();
        g(&a, &["add", "."]);
        g(&a, &["commit", "-qm", "first"]);
        g(&a, &["remote", "add", "origin", origin.to_str().unwrap()]);
        g(&a, &["push", "-qu", "origin", "main"]);

        let b = base.join("b");
        g(&base, &["clone", "-q", origin.to_str().unwrap(), b.to_str().unwrap()]);
        g(&b, &["config", "user.email", "b@example.com"]);
        g(&b, &["config", "user.name", "B"]);
        std::fs::write(b.join("f.txt"), "one\ntwo\n").unwrap();
        g(&b, &["commit", "-qam", "second"]);
        g(&b, &["push", "-q"]);

        let mut git = Git::open(&a).unwrap();
        // 距离是按本地的远端跟踪分支算的：先抓取，才看得到 b 新推的提交。
        fetch_remote(&mut git, "origin", false).unwrap();
        let (_, behind) = ahead_behind(&mut git, "HEAD", "origin/main").unwrap();
        assert!(behind >= 1, "本地应该落后远端: behind={behind}");

        // 工作区有未提交的改动，拉取时自动贮藏、拉完再恢复。
        std::fs::write(a.join("local.txt"), "keep me\n").unwrap();
        pull_from(&mut git, "origin", "main", false).unwrap();

        assert_eq!(std::fs::read_to_string(a.join("local.txt")).unwrap(), "keep me\n");
        assert_eq!(std::fs::read_to_string(a.join("f.txt")).unwrap(), "one\ntwo\n");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn remotes_are_listed_with_urls() {
        let (dir, mut git) = temp_repo("remotes");
        git.run_ok(&["remote", "add", "origin", "https://example.com/repo.git"]).unwrap();
        git.run_ok(&["remote", "add", "upstream", "https://example.com/up.git"]).unwrap();
        let list = crate::tags::remotes(&mut git).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "origin");
        assert!(list[0].url.contains("example.com/repo"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

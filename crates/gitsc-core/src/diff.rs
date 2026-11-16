//! Unified diff 解析与读取。

use std::path::Path;

use crate::command::GitError;
use crate::{Git, GitOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    /// `@@ ... @@` 或 `@@@ ... @@@`
    Hunk,
    /// `\ No newline at end of file`
    Meta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Hunk {
    pub header: String,
    pub section: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, Default)]
pub struct DiffFile {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub header: Vec<String>,
    pub hunks: Vec<Hunk>,
    pub binary: bool,
    pub untracked: bool,
    pub note: Option<String>,
}

impl DiffFile {
    pub fn path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("")
    }

    pub fn added(&self) -> u32 {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == LineKind::Added)
            .count() as u32
    }

    pub fn removed(&self) -> u32 {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == LineKind::Removed)
            .count() as u32
    }

    /// 文件头里的状态提示（new file / deleted / mode 100755 等）。
    pub fn status_hint(&self) -> Option<String> {
        for line in &self.header {
            if let Some(mode) = line.strip_prefix("new file mode ") {
                return Some(format!("新文件 {mode}"));
            }
            if line.starts_with("deleted file mode ") {
                return Some("已删除".to_string());
            }
            if line.starts_with("rename from ") {
                return Some("重命名".to_string());
            }
            if line.starts_with("old mode ") {
                return Some("权限变化".to_string());
            }
        }
        None
    }
}

#[derive(Clone, Debug, Default)]
pub struct DiffDoc {
    pub files: Vec<DiffFile>,
    /// 没有内容时的原因（例如工作区干净、文件被忽略）。
    pub empty: Option<String>,
}

impl DiffDoc {
    pub fn is_empty(&self) -> bool {
        self.files.iter().all(|f| f.hunks.is_empty() && !f.binary)
    }

    pub fn added(&self) -> u32 {
        self.files.iter().map(DiffFile::added).sum()
    }

    pub fn removed(&self) -> u32 {
        self.files.iter().map(DiffFile::removed).sum()
    }

    pub fn line_count(&self) -> usize {
        self.files
            .iter()
            .map(|f| f.hunks.iter().map(|h| h.lines.len() + 1).sum::<usize>() + f.header.len())
            .sum()
    }
}

/// 工作区（或索引）里某个路径的 diff。未跟踪文件没有 diff，直接读文件内容
/// 合成一段“全部新增”，这样预览和 IDEA 的 New File 视图一致。
pub fn read_worktree(git: &mut Git, path: &str, staged: bool) -> Result<DiffDoc, GitError> {
    let mut args: Vec<&str> = vec!["diff", "--no-color", "--no-ext-diff", "-U3"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(path);
    let out = git.run_ok(&args)?;
    let mut doc = parse(&out);

    if doc.files.is_empty() {
        if let Some(diff) = read_untracked(git.root(), path) {
            doc.files.push(diff);
            return Ok(doc);
        }
    }
    if doc.is_empty() && doc.files.is_empty() {
        doc.empty = Some(if staged {
            "索引里没有该文件的改动".into()
        } else {
            "工作区没有该文件的改动".into()
        });
    }
    Ok(doc)
}

pub fn read_commit(git: &mut Git, oid: &str) -> Result<DiffDoc, GitError> {
    let out = git.run_ok(&[
        "show",
        "--format=",
        "--no-color",
        "--no-ext-diff",
        "-U3",
        "--first-parent",
        "-m",
        oid,
    ])?;
    Ok(parse(&out))
}

/// 任意两个版本之间的差异。`to` 为 `None` 时表示「版本 → 工作区」。
pub fn read_range(git: &mut Git, from: &str, to: Option<&str>) -> Result<DiffDoc, GitError> {
    let mut args: Vec<&str> = vec!["diff", "--no-color", "--no-ext-diff", "-U3"];
    args.push(from);
    if let Some(to) = to {
        args.push(to);
    }
    args.push("--");
    let out = git.run_ok(&args)?;
    Ok(parse(&out))
}

/// 两个版本之间单个文件的差异。
pub fn read_range_file(
    git: &mut Git,
    from: &str,
    to: Option<&str>,
    path: &str,
) -> Result<DiffDoc, GitError> {
    let mut args: Vec<&str> = vec!["diff", "--no-color", "--no-ext-diff", "-U3"];
    args.push(from);
    if let Some(to) = to {
        args.push(to);
    }
    args.push("--");
    args.push(path);
    let out = git.run_ok(&args)?;
    Ok(parse(&out))
}

/// 两个版本之间每个文件的增删行数，用于变更文件列表。
pub fn numstat_range(
    git: &mut Git,
    from: &str,
    to: Option<&str>,
) -> Result<Vec<(String, Option<u32>, Option<u32>)>, GitError> {
    let mut args: Vec<&str> = vec!["diff", "--numstat", "--no-color"];
    args.push(from);
    if let Some(to) = to {
        args.push(to);
    }
    args.push("--");
    let out = git.run_ok(&args)?;
    Ok(parse_numstat(&out.stdout))
}

/// 两个版本之间的提交列表（`from..to`），用于「这次拉取会带进来什么」。
pub fn commits_between(
    git: &mut Git,
    from: &str,
    to: &str,
    limit: usize,
) -> Result<Vec<crate::log::Commit>, GitError> {
    let range = format!("{from}..{to}");
    crate::log::read_range(git, &range, limit)
}

pub fn read_commit_file(git: &mut Git, oid: &str, path: &str) -> Result<DiffDoc, GitError> {
    let out = git.run_ok(&[
        "show",
        "--format=",
        "--no-color",
        "--no-ext-diff",
        "-U3",
        "--first-parent",
        "-m",
        oid,
        "--",
        path,
    ])?;
    let mut doc = parse(&out);
    if doc.files.is_empty() {
        doc.empty = Some(format!("该提交没有改动 {path}"));
    }
    Ok(doc)
}

/// `git show --numstat`：提交涉及的文件及增删行数（二进制为 `-`）。
pub fn numstat(git: &mut Git, oid: &str) -> Result<Vec<(String, Option<u32>, Option<u32>)>, GitError> {
    let out = git.run_ok(&[
        "show",
        "--format=",
        "--numstat",
        "--first-parent",
        "-m",
        oid,
    ])?;
    Ok(parse_numstat(&out.stdout))
}

/// 解析 `--numstat`：`增	删	路径`，二进制文件的增删是 `-`。
fn parse_numstat(stdout: &str) -> Vec<(String, Option<u32>, Option<u32>)> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut it = line.splitn(3, '\t');
            let added = it.next()?;
            let removed = it.next()?;
            let path = it.next()?.trim();
            if path.is_empty() {
                return None;
            }
            Some((path.to_string(), added.parse().ok(), removed.parse().ok()))
        })
        .collect()
}

fn read_untracked(root: &Path, path: &str) -> Option<DiffFile> {
    let full = root.join(path);
    let meta = std::fs::metadata(&full).ok()?;
    if meta.len() > 2 * 1024 * 1024 {
        return Some(DiffFile {
            new_path: Some(path.to_string()),
            note: Some(format!("文件过大（{}），不预览", human_size(meta.len()))),
            untracked: true,
            ..Default::default()
        });
    }
    let bytes = std::fs::read(&full).ok()?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return Some(DiffFile {
            new_path: Some(path.to_string()),
            binary: true,
            untracked: true,
            ..Default::default()
        });
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = Vec::new();
    let total = text.lines().count();
    for (i, line) in text.lines().take(4000).enumerate() {
        lines.push(DiffLine {
            kind: LineKind::Added,
            old_no: None,
            new_no: Some(i as u32 + 1),
            text: line.to_string(),
        });
    }
    let mut hunks = Vec::new();
    if !lines.is_empty() {
        hunks.push(Hunk {
            header: format!("@@ -0,0 +1,{total} @@"),
            section: String::new(),
            lines,
        });
    }
    Some(DiffFile {
        old_path: None,
        new_path: Some(path.to_string()),
        header: vec!["new file".into()],
        hunks,
        binary: false,
        untracked: true,
        note: (total > 4000).then(|| format!("只显示前 4000 行（共 {total} 行）")),
    })
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    } else {
        format!("{} KB", bytes / 1024)
    }
}

/// 解析多文件 unified diff。
pub fn parse(out: &GitOutput) -> DiffDoc {
    let mut doc = DiffDoc::default();
    let mut current: Option<DiffFile> = None;
    let mut hunk: Option<Hunk> = None;
    let mut old_no = 0u32;
    let mut new_no = 0u32;

    for line in out.stdout.lines() {
        if line.starts_with("diff --git ") {
            flush_hunk(&mut current, &mut hunk);
            if let Some(file) = current.take() {
                doc.files.push(file);
            }
            let mut file = DiffFile::default();
            if let Some((a, b)) = parse_diff_git_paths(line) {
                file.old_path = Some(a);
                file.new_path = Some(b);
            }
            current = Some(file);
            continue;
        }

        let Some(file) = current.as_mut() else {
            continue;
        };

        if let Some(h) = hunk.as_mut() {
            // hunk 内部
            if line.starts_with("@@") {
                flush_hunk(&mut current, &mut hunk);
                if let Some(new_hunk) = start_hunk(line) {
                    let (o, n) = hunk_starts(&new_hunk.header);
                    old_no = o;
                    new_no = n;
                    hunk = Some(new_hunk);
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix('\\') {
                h.lines.push(DiffLine {
                    kind: LineKind::Meta,
                    old_no: None,
                    new_no: None,
                    text: rest.trim().to_string(),
                });
                continue;
            }
            let (kind, text, has_old, has_new) = match line.as_bytes().first() {
                Some(b'+') => (LineKind::Added, &line[1..], false, true),
                Some(b'-') => (LineKind::Removed, &line[1..], true, false),
                Some(b' ') => (LineKind::Context, &line[1..], true, true),
                _ => (LineKind::Context, line, false, false),
            };
            h.lines.push(DiffLine {
                kind,
                old_no: has_old.then_some(old_no),
                new_no: has_new.then_some(new_no),
                text: text.to_string(),
            });
            if has_old {
                old_no += 1;
            }
            if has_new {
                new_no += 1;
            }
            continue;
        }

        // 文件头
        if line.starts_with("@@") {
            if let Some(new_hunk) = start_hunk(line) {
                let (o, n) = hunk_starts(&new_hunk.header);
                old_no = o;
                new_no = n;
                hunk = Some(new_hunk);
            }
            continue;
        }
        if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            file.binary = true;
        }
        if let Some(rest) = line.strip_prefix("--- ") {
            file.old_path = parse_path(rest);
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            file.new_path = parse_path(rest).or(file.new_path.take());
            continue;
        }
        file.header.push(line.to_string());
    }

    flush_hunk(&mut current, &mut hunk);
    if let Some(file) = current {
        doc.files.push(file);
    }
    doc
}

fn flush_hunk(current: &mut Option<DiffFile>, hunk: &mut Option<Hunk>) {
    if let (Some(file), Some(h)) = (current.as_mut(), hunk.take()) {
        file.hunks.push(h);
    }
}

fn start_hunk(line: &str) -> Option<Hunk> {
    let end = line[2..].find("@@").map(|i| i + 2)? + 2;
    let header = line[..end].to_string();
    let section = line[end..].trim().to_string();
    Some(Hunk {
        header,
        section,
        lines: Vec::new(),
    })
}

/// 从 `@@ -a,b +c,d @@` 里取起始行号。
fn hunk_starts(header: &str) -> (u32, u32) {
    let mut old = 1;
    let mut new = 1;
    for part in header.split_whitespace() {
        if let Some(rest) = part.strip_prefix('-') {
            old = rest
                .split(',')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
        } else if let Some(rest) = part.strip_prefix('+') {
            new = rest
                .split(',')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
        }
    }
    (old, new)
}

/// `--- a/path` / `+++ b/path` / 带引号与 `/dev/null` 的变体。
fn parse_path(raw: &str) -> Option<String> {
    let raw = raw.split('\t').next().unwrap_or(raw).trim();
    if raw == "/dev/null" {
        return None;
    }
    let unquoted = if raw.starts_with('"') && raw.ends_with('"') && raw.len() > 1 {
        raw[1..raw.len() - 1].replace("\\\"", "\"")
    } else {
        raw.to_string()
    };
    Some(
        unquoted
            .strip_prefix("a/")
            .or_else(|| unquoted.strip_prefix("b/"))
            .unwrap_or(&unquoted)
            .to_string(),
    )
}

/// 从 `diff --git a/x b/x` 里取两个路径；路径本身可能带空格，用 ` b/` 定位。
fn parse_diff_git_paths(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("diff --git ")?;
    let rest = rest.strip_prefix("a/").unwrap_or(rest);
    if let Some(idx) = rest.rfind(" b/") {
        let a = rest[..idx].to_string();
        let b = rest[idx + 3..].to_string();
        return Some((a, b));
    }
    let mut it = rest.splitn(2, ' ');
    let a = it.next()?.to_string();
    let b = it.next()?.trim_start_matches("b/").to_string();
    Some((a, b))
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

    const SAMPLE: &str = "\
diff --git a/src/main.rs b/src/main.rs
index 1111111..2222222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@ fn main() {
 fn main() {
-    println!(\"old\");
+    println!(\"new\");
+    println!(\"extra\");
 }
 // done
";

    #[test]
    fn parses_single_file_diff() {
        let doc = parse(&out(SAMPLE));
        assert_eq!(doc.files.len(), 1);
        let f = &doc.files[0];
        assert_eq!(f.path(), "src/main.rs");
        assert_eq!(f.hunks.len(), 1);
        assert_eq!(f.added(), 2);
        assert_eq!(f.removed(), 1);
        let h = &f.hunks[0];
        assert_eq!(h.header, "@@ -1,4 +1,5 @@");
        assert_eq!(h.section, "fn main() {");
        assert_eq!(h.lines[0].old_no, Some(1));
        assert_eq!(h.lines[1].kind, LineKind::Removed);
        assert_eq!(h.lines[2].new_no, Some(2));
        assert_eq!(h.lines[2].text, "    println!(\"new\");");
    }

    #[test]
    fn tracks_line_numbers_across_two_hunks() {
        let text = "\
diff --git a/x b/x
--- a/x
+++ b/x
@@ -2,2 +2,2 @@
 a
-b
+B
@@ -10,2 +10,3 @@
 c
+d
 e
";
        let doc = parse(&out(text));
        let f = &doc.files[0];
        assert_eq!(f.hunks.len(), 2);
        assert_eq!(f.hunks[0].lines[0].old_no, Some(2));
        assert_eq!(f.hunks[0].lines[1].old_no, Some(3));
        assert_eq!(f.hunks[1].lines[0].old_no, Some(10));
        assert_eq!(f.hunks[1].lines[1].new_no, Some(11));
        assert_eq!(f.hunks[1].lines[2].new_no, Some(12));
    }

    #[test]
    fn handles_new_and_binary_files() {
        let text = "\
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..1111111
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
diff --git a/img.png b/img.png
index 1111111..2222222 100644
Binary files a/img.png and b/img.png differ
";
        let doc = parse(&out(text));
        assert_eq!(doc.files.len(), 2);
        assert!(doc.files[0].old_path.is_none());
        assert_eq!(doc.files[0].path(), "new.txt");
        assert_eq!(doc.files[0].header[0], "new file mode 100644");
        assert!(doc.files[1].binary);
    }

    #[test]
    fn parses_paths_with_spaces() {
        let text = "\
diff --git a/docs/中文 文档.md b/docs/中文 文档.md
--- a/docs/中文 文档.md
+++ b/docs/中文 文档.md
@@ -1 +1 @@
-a
+b
";
        let doc = parse(&out(text));
        assert_eq!(doc.files[0].path(), "docs/中文 文档.md");
        assert_eq!(doc.files[0].hunks.len(), 1);
    }
}

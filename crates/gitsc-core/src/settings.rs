//! 应用设置：最近打开的仓库、窗口偏好、提交模板等，存在
//! `~/Library/Application Support/gitsc/settings.json`。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config_dir;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentRepo {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub pinned: bool,
    /// Unix 毫秒。
    #[serde(default)]
    pub opened_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recents: Vec<RecentRepo>,
    /// 上次打开的仓库。
    pub last_repo: Option<String>,
    /// 点「提交」后是否清空提交信息框（IDEA 默认清空）。
    pub clear_message_after_commit: bool,
    /// 拉取时默认用 rebase —— 和 IDEA 的 Update Type 对应。
    pub pull_rebase: bool,
    /// 列表里是否显示远端分支。
    pub show_remote_branches: bool,
    /// 提交列表条数上限。
    pub log_limit: usize,
    /// 切换分支/合并且工作区有改动时，先自动 stash，完成后再恢复。
    pub auto_stash: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recents: Vec::new(),
            last_repo: None,
            clear_message_after_commit: true,
            pull_rebase: false,
            show_remote_branches: true,
            log_limit: 400,
            auto_stash: true,
        }
    }
}

impl Settings {
    pub fn file() -> PathBuf {
        config_dir().join("settings.json")
    }

    pub fn load() -> Self {
        let path = Self::file();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)
    }

    /// 记录一次打开，把仓库提到最近列表最前面。
    pub fn touch(&mut self, path: &Path) {
        let path_s = path.to_string_lossy().to_string();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path_s.clone());
        let now = chrono_now_ms();
        let pinned = self
            .recents
            .iter()
            .find(|r| r.path == path_s)
            .map(|r| r.pinned)
            .unwrap_or(false);
        self.recents.retain(|r| r.path != path_s);
        self.recents.insert(
            0,
            RecentRepo {
                path: path_s.clone(),
                name,
                pinned,
                opened_at: now,
            },
        );
        self.last_repo = Some(path_s);
        self.recents.truncate(40);
    }

    pub fn forget(&mut self, path: &str) {
        self.recents.retain(|r| r.path != path);
        if self.last_repo.as_deref() == Some(path) {
            self.last_repo = None;
        }
    }

    pub fn toggle_pin(&mut self, path: &str) {
        if let Some(r) = self.recents.iter_mut().find(|r| r.path == path) {
            r.pinned = !r.pinned;
        }
    }

    /// 置顶的在前，其余按打开时间倒序。
    pub fn sorted_recents(&self) -> Vec<RecentRepo> {
        let mut list = self.recents.clone();
        list.sort_by_key(|r| (!r.pinned, -r.opened_at));
        list
    }
}

fn chrono_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_moves_repo_to_front_and_keeps_pin() {
        let mut s = Settings::default();
        s.touch(Path::new("/tmp/a"));
        s.toggle_pin("/tmp/a");
        s.touch(Path::new("/tmp/b"));
        s.touch(Path::new("/tmp/a"));
        assert_eq!(s.recents.len(), 2);
        assert_eq!(s.recents[0].path, "/tmp/a");
        assert!(s.recents[0].pinned);
        assert_eq!(s.last_repo.as_deref(), Some("/tmp/a"));
        assert_eq!(s.sorted_recents()[0].path, "/tmp/a");
    }

    #[test]
    fn forget_clears_last_repo() {
        let mut s = Settings::default();
        s.touch(Path::new("/tmp/a"));
        s.forget("/tmp/a");
        assert!(s.recents.is_empty());
        assert!(s.last_repo.is_none());
    }
}

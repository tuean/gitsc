//! 应用状态与页面骨架。
//!
//! 结构对齐 IDEA 的 Git 工具窗：顶栏（仓库 / 分支 / 常用操作）、左侧竖排页签、
//! 主工作区、底部状态栏；所有写操作走 `worker`，UI 线程只处理状态。

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState, TextareaState};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;
use gitsc_core::diff::{DiffDoc, LineKind};
use gitsc_core::graph::{self, GraphRow};
use gitsc_core::log::Commit;
use gitsc_core::settings::Settings;
use gitsc_core::status::Status;
use gitsc_core::tags::{Branch, RemoteInfo, Stash, Tag};
use gitsc_core::GitOutput;

use crate::panels;
use crate::theme::{self, fs};
use crate::widgets::{self, fmt_datetime, fmt_time};
use crate::worker::{self, Action, DiffKey, FileStat, Job, Payload};

pub const MAX_DIFF_ROWS: usize = 6000;
pub const ROW_H: f32 = 26.0;
pub const GRAPH_LANE_W: f32 = 14.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Commit,
    Compare,
    Log,
    Branches,
    Stash,
    Console,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Commit,
        Tab::Compare,
        Tab::Log,
        Tab::Branches,
        Tab::Stash,
        Tab::Console,
    ];
    pub fn index(self) -> usize {
        match self {
            Tab::Commit => 0,
            Tab::Compare => 1,
            Tab::Log => 2,
            Tab::Branches => 3,
            Tab::Stash => 4,
            Tab::Console => 5,
        }
    }
}

/// 对比页的状态：两个版本 + 变更文件 + 当前文件的差异。
#[derive(Clone, Default)]
pub struct CompareState {
    pub from: String,
    pub to: String,
    pub files: Vec<FileStat>,
    pub doc: Option<DiffPane>,
    pub selected: Option<String>,
    pub file_doc: Option<DiffPane>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    FileHeader,
    Hunk,
    Add,
    Del,
    Ctx,
    Meta,
}

#[derive(Clone, Debug)]
pub struct DiffRow {
    pub kind: RowKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct DiffPane {
    pub title: String,
    pub subtitle: String,
    pub added: u32,
    pub removed: u32,
    pub rows: Vec<DiffRow>,
    pub truncated: bool,
    pub empty: Option<String>,
    pub serial: u64,
}

impl DiffPane {
    pub fn from_doc(title: String, subtitle: String, doc: &DiffDoc) -> Self {
        let mut rows = Vec::new();
        let mut truncated = false;
        let show_file_headers = doc.files.len() > 1;
        'outer: for file in &doc.files {
            let path = file.path().to_string();
            let hint = file.status_hint().unwrap_or_default();
            // 单文件时标题栏已经写了路径，不再重复一行。
            if show_file_headers {
                rows.push(DiffRow {
                    kind: RowKind::FileHeader,
                    old: None,
                    new: None,
                    text: if hint.is_empty() {
                        path.clone()
                    } else {
                        format!("{path}    {hint}")
                    },
                });
            }
            if file.binary {
                rows.push(DiffRow {
                    kind: RowKind::Meta,
                    old: None,
                    new: None,
                    text: "二进制文件，无法显示文本差异".into(),
                });
                continue;
            }
            for hunk in &file.hunks {
                rows.push(DiffRow {
                    kind: RowKind::Hunk,
                    old: None,
                    new: None,
                    text: hunk.header.clone(),
                });
                for line in &hunk.lines {
                    if rows.len() >= MAX_DIFF_ROWS {
                        truncated = true;
                        break 'outer;
                    }
                    let kind = match line.kind {
                        LineKind::Context => RowKind::Ctx,
                        LineKind::Added => RowKind::Add,
                        LineKind::Removed => RowKind::Del,
                        LineKind::Hunk => RowKind::Hunk,
                        LineKind::Meta => RowKind::Meta,
                    };
                    rows.push(DiffRow {
                        kind,
                        old: line.old_no,
                        new: line.new_no,
                        text: line.text.replace('\t', "    "),
                    });
                }
            }
            rows.push(DiffRow {
                kind: RowKind::Meta,
                old: None,
                new: None,
                text: String::new(),
            });
        }
        Self {
            title,
            subtitle,
            added: doc.added(),
            removed: doc.removed(),
            rows,
            truncated,
            empty: doc.empty.clone(),
            serial: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CommitDetail {
    pub oid: String,
    pub body: String,
    pub files: Vec<FileStat>,
    pub error: Option<String>,
    pub file_diff: Option<DiffPane>,
    pub loading: bool,
    pub file_loading: bool,
}

#[derive(Default)]
pub struct RepoState {
    pub root: PathBuf,
    pub name: String,
    pub status: Option<Status>,
    pub all_commits: Vec<Commit>,
    pub view: Vec<Commit>,
    pub graph: Vec<GraphRow>,
    pub graph_lanes: usize,
    pub branches: Vec<Branch>,
    pub stashes: Vec<Stash>,
    pub tags: Vec<Tag>,
    pub selected_commit: Option<usize>,
    pub detail: Option<CommitDetail>,
    pub selected_branch: Option<String>,
    pub selected_stash: Option<usize>,
    pub selected_change: Option<(bool, String)>,
    pub diff_key: Option<DiffKey>,
    pub diff: Option<DiffPane>,
    pub diff_loading: bool,
    pub status_loading: bool,
}

impl RepoState {
    fn new(root: PathBuf, name: String) -> Self {
        Self {
            root,
            name,
            status_loading: true,
            ..Default::default()
        }
    }

    pub fn branch(&self) -> String {
        self.status
            .as_ref()
            .and_then(|s| s.branch.clone())
            .unwrap_or_else(|| "detached HEAD".to_string())
    }

    pub fn ahead(&self) -> usize {
        self.status.as_ref().map(|s| s.ahead).unwrap_or(0)
    }

    pub fn behind(&self) -> usize {
        self.status.as_ref().map(|s| s.behind).unwrap_or(0)
    }

    pub fn selected_commit(&self) -> Option<&Commit> {
        self.selected_commit.and_then(|i| self.view.get(i))
    }

    pub fn selected_commit_oid(&self) -> Option<String> {
        self.selected_commit().map(|c| c.oid.clone())
    }

    pub fn changes(&self) -> &[gitsc_core::status::Change] {
        self.status
            .as_ref()
            .map(|s| s.changes.as_slice())
            .unwrap_or(&[])
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Error,
}

pub struct Toast {
    pub text: String,
    pub tone: Tone,
    pub id: u64,
}

pub struct ConsoleEntry {
    pub out: GitOutput,
    pub id: u64,
}

pub enum Modal {
    Prompt {
        title: String,
        label: String,
        hint: String,
        input: Entity<InputState>,
        kind: PromptKind,
        danger: bool,
    },
    Confirm {
        title: String,
        body: String,
        confirm: String,
        danger: bool,
        kind: ConfirmKind,
    },
    Pull(PullDialog),
}

/// 「从其他远端拉取」对话框。
#[derive(Clone)]
pub struct PullDialog {
    pub remote: String,
    pub branch: String,
    pub rebase: bool,
    pub auto_stash: bool,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    pub loading: bool,
}

pub enum PromptKind {
    NewBranch { start: String },
    RenameBranch { old: String },
    NewTag { target: String },
    Stash,
    Clone,
    InitRepo,
}

pub enum ConfirmKind {
    DeleteBranch(String),
    DeleteTag(String),
    DropStash(usize),
    Discard(Vec<String>),
    Reset { oid: String, hard: bool },
    RemoveRecent(String),
}

pub struct MenuItem {
    pub label: String,
    pub icon: Option<IconName>,
    pub danger: bool,
    pub action: MenuAction,
}

pub const MENU_W: f32 = 250.0;
pub const MENU_H: f32 = 340.0;
pub const MENU_MIN_W: f32 = 190.0;
pub const MENU_MIN_H: f32 = 110.0;
pub const MENU_MAX_W: f32 = 720.0;
pub const MENU_MAX_H: f32 = 860.0;

pub struct MenuState {
    pub x: f32,
    pub y: f32,
    pub items: Vec<MenuItem>,
    /// 弹出菜单的宽高：右下角可拖拽调整，内容超出高度时滚动。
    pub w: f32,
    pub h: f32,
    /// 用户拖过高度之后，菜单用固定高度而不是「内容多高就多高」。
    pub fixed_h: bool,
}

/// 正在拖拽调整菜单尺寸。
pub struct MenuResize {
    pub start_x: f32,
    pub start_y: f32,
    pub w0: f32,
    pub h0: f32,
}

#[derive(Clone, Debug)]
pub enum MenuAction {
    Checkout(String),
    Merge(String),
    Rebase(String),
    NewBranch(String),
    RenameBranch(String),
    DeleteBranch(String),
    CheckoutDetach(String),
    CherryPick(String),
    Revert(String),
    Reset { oid: String, hard: bool },
    Tag(String),
    CopyHash(String),
    Push,
    Pull,
    Fetch,
    Stage(String),
    Unstage(String),
    Discard(String),
    ApplyStash(usize),
    PopStash(usize),
    DropStash(usize),
    ShowStash(usize),
    DeleteTag(String),
    PushTag(String),
    NewTagHere,
    NewBranchHere(String),
    /// 对比页选一个版本：target 0=左侧（from） 1=右侧（to）。
    PickCompareRef { target: u8, value: String },
    /// 对比页选「工作区」作为右侧。
    PickWorktree,
    SwapCompare,
    /// 拉取对话框：选远端。
    PickRemote(String),
    /// 拉取对话框：选远端分支。
    PickPullBranch(String),
    /// 拉取对话框切换合并/变基。
    PickPullStrategy(bool),
    /// 拉取对话框切换自动贮藏。
    ToggleAutoStash,
    /// 对某个远端单独抓取。
    FetchRemote(String),
    /// 把某个提交设为对比的左侧。
    CompareFrom(String),
}

pub struct GitApp {
    pub settings: Settings,
    pub jobs: flume::Sender<Job>,
    pub events: flume::Receiver<Payload>,
    pub focus: FocusHandle,
    pub repo: Option<RepoState>,
    pub tab: Tab,
    pub busy: usize,
    pub modal: Option<Modal>,
    pub menu: Option<MenuState>,
    pub menu_resize: Option<MenuResize>,
    pub toast: Option<Toast>,
    pub toast_seq: u64,
    pub console: Vec<ConsoleEntry>,
    pub console_seq: u64,
    pub message: Entity<TextareaState>,
    pub message_text: String,
    pub amend: bool,
    pub filter: Entity<InputState>,
    pub filter_text: String,
    pub graph_all: bool,
    pub git_version: String,
    pub clear_message_next: bool,
    pub diff_serial: u64,
    pub welcome_recent: Vec<PathBuf>,
    pub remotes: Vec<RemoteInfo>,
    pub compare: CompareState,
    pub compare_seq: u64,
}

impl GitApp {
    pub fn new(
        initial: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (jobs, events) = worker::spawn();
        let settings = Settings::load();
        let message = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder("提交信息（首行是标题，⌘↩ 提交）")
        });
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("搜索提交、作者或哈希…"));
        let version = gitsc_core::git_version();

        cx.subscribe(&message, |this: &mut GitApp, state, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { secondary, .. } = event {
                if *secondary {
                    this.commit(cx);
                } else {
                    this.message_text = state.read(cx).value().to_string();
                    cx.notify();
                }
                return;
            }
            this.message_text = state.read(cx).value().to_string();
            cx.notify();
        })
        .detach();

        cx.subscribe(&filter, |this: &mut GitApp, state, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                this.filter_text = state.read(cx).value().to_string();
                this.rebuild_log();
            }
            cx.notify();
        })
        .detach();

        let events_for_pump = events.clone();
        cx.spawn(async move |weak, cx| {
            loop {
                let first = match events_for_pump.recv_async().await {
                    Ok(payload) => payload,
                    Err(_) => break,
                };
                let mut batch = vec![first];
                while let Ok(payload) = events_for_pump.try_recv() {
                    batch.push(payload);
                }
                let updated = weak.update(cx, |this, cx| {
                    for payload in batch {
                        this.handle(payload, cx);
                    }
                    cx.notify();
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();

        let mut app = Self {
            settings,
            jobs,
            events,
            focus: cx.focus_handle(),
            repo: None,
            tab: Tab::Commit,
            busy: 0,
            modal: None,
            menu: None,
            menu_resize: None,
            toast: None,
            toast_seq: 0,
            console: Vec::new(),
            console_seq: 0,
            message,
            message_text: String::new(),
            amend: false,
            filter,
            filter_text: String::new(),
            graph_all: true,
            git_version: version,
            clear_message_next: false,
            diff_serial: 0,
            welcome_recent: Vec::new(),
            remotes: Vec::new(),
            compare: CompareState::default(),
            compare_seq: 0,
        };

        // 根元素拿到焦点，⌘ 快捷键在窗口刚打开时就能用，不必先点一下。
        window.focus(&app.focus, cx);

        // 窗口失去激活时收起弹出菜单（点别的 app、切窗口都算）。
        cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() && this.menu.take().is_some() {
                this.menu_resize = None;
                cx.notify();
            }
        })
        .detach();

        let open = initial.or_else(|| {
            app.settings
                .last_repo
                .as_ref()
                .map(PathBuf::from)
                .filter(|p| p.exists())
        });
        if let Some(path) = open {
            app.open_repo(path, cx);
        } else {
            app.refresh_welcome();
        }
        app
    }

    // ---- 通用 -------------------------------------------------------------

    pub fn send(&mut self, job: Job, cx: &mut Context<Self>) {
        self.busy += 1;
        let _ = self.jobs.send(job);
        cx.notify();
    }

    pub fn open_repo(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.repo = None;
        self.send(Job::OpenRepo(path), cx);
    }

    pub fn refresh_welcome(&mut self) {
        self.welcome_recent = self
            .settings
            .sorted_recents()
            .into_iter()
            .map(|r| PathBuf::from(&r.path))
            .collect();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.repo.is_none() {
            return;
        }
        self.send(
            Job::Log {
                limit: self.settings.log_limit,
                all: self.graph_all,
            },
            cx,
        );
        self.send(Job::Refresh, cx);
        self.send(Job::Branches, cx);
        self.send(Job::Stashes, cx);
        self.send(Job::Tags, cx);
        self.send(Job::Remotes, cx);
    }

    /// 点到菜单外面就收起；点在里面（包括菜单项）不动，交给菜单项自己的处理器。
    pub fn dismiss_menu_outside(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(menu) = &self.menu else { return };
        let content_h = (menu.items.len() as f32 * 26. + 8.)
            .min(menu.h)
            .max(menu.h.min(MENU_MIN_H));
        let x = f32::from(pos.x);
        let y = f32::from(pos.y);
        let pad = 4.;
        let inside = x >= menu.x - pad
            && y >= menu.y - pad
            && x <= menu.x + menu.w + pad
            && y <= menu.y + content_h + pad;
        if !inside {
            self.menu = None;
            self.menu_resize = None;
            cx.notify();
        }
    }

    pub fn set_toast(&mut self, text: impl Into<String>, tone: Tone, cx: &mut Context<Self>) {
        self.toast_seq += 1;
        let id = self.toast_seq;
        self.toast = Some(Toast {
            text: text.into(),
            tone,
            id,
        });
        let executor = cx.background_executor().clone();
        cx.spawn(async move |weak, cx| {
            executor.timer(Duration::from_secs(4)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.toast.as_ref().map(|t| t.id) == Some(id) {
                    this.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn append_console(&mut self, outputs: Vec<GitOutput>) {
        for out in outputs {
            self.console_seq += 1;
            self.console.push(ConsoleEntry {
                out,
                id: self.console_seq,
            });
        }
        if self.console.len() > 300 {
            let drop = self.console.len() - 300;
            self.console.drain(0..drop);
        }
    }

    pub fn run(&mut self, action: Action, cx: &mut Context<Self>) {
        self.send(Job::Act(action), cx);
    }

    // ---- 提交 -------------------------------------------------------------

    pub fn can_commit(&self) -> bool {
        !self.message_text.trim().is_empty()
    }

    pub fn commit(&mut self, cx: &mut Context<Self>) {
        let message = self.message_text.trim_end().to_string();
        if message.trim().is_empty() {
            self.set_toast("先写提交信息", Tone::Error, cx);
            return;
        }
        self.run(
            Action::Commit {
                message,
                amend: self.amend,
            },
            cx,
        );
    }

    pub fn stage(&mut self, path: String, cx: &mut Context<Self>) {
        self.run(Action::Stage(vec![path]), cx);
    }

    pub fn unstage(&mut self, path: String, cx: &mut Context<Self>) {
        self.run(Action::Unstage(vec![path]), cx);
    }

    pub fn stage_all(&mut self, cx: &mut Context<Self>) {
        self.run(Action::StageAll, cx);
    }

    pub fn unstage_all(&mut self, cx: &mut Context<Self>) {
        self.run(Action::UnstageAll, cx);
    }

    /// 丢弃改动前统一走确认框；untracked 文件等价于删除文件。
    pub fn ask_discard(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let body = if paths.len() == 1 {
            format!("{} 的本地改动会被永久丢弃，无法撤销。", paths[0])
        } else {
            format!("{} 个文件的本地改动会被永久丢弃，无法撤销。", paths.len())
        };
        self.open_confirm(ConfirmKind::Discard(paths), "丢弃改动", body, "丢弃", true);
        cx.notify();
    }

    pub fn open_change_menu(
        &mut self,
        staged: bool,
        path: String,
        ev: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let (x, y) = self.menu_now(ev);
        let items = if staged {
            vec![
                MenuItem {
                    label: "取消暂存".into(),
                    icon: Some(IconName::Minus),
                    danger: false,
                    action: MenuAction::Unstage(path.clone()),
                },
                MenuItem {
                    label: "丢弃改动".into(),
                    icon: Some(IconName::Undo2),
                    danger: true,
                    action: MenuAction::Discard(path),
                },
            ]
        } else {
            vec![
                MenuItem {
                    label: "暂存".into(),
                    icon: Some(IconName::Plus),
                    danger: false,
                    action: MenuAction::Stage(path.clone()),
                },
                MenuItem {
                    label: "丢弃改动".into(),
                    icon: Some(IconName::Undo2),
                    danger: true,
                    action: MenuAction::Discard(path),
                },
            ]
        };
        self.menu = Some(MenuState { x, y, items, w: MENU_W, h: MENU_H, fixed_h: false });
        self.modal = None;
        cx.notify();
    }

    pub fn select_branch(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(repo) = &mut self.repo {
            repo.selected_branch = Some(name);
        }
        cx.notify();
    }

    pub fn select_commit_by_oid(&mut self, oid: String, cx: &mut Context<Self>) {
        let index = self
            .repo
            .as_ref()
            .and_then(|repo| repo.view.iter().position(|c| c.oid == oid));
        if let Some(index) = index {
            self.select_commit(index, cx);
        }
    }

    pub fn copy_commit_hash(&mut self, oid: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(oid));
        self.set_toast("已复制提交哈希", Tone::Info, cx);
    }

    pub fn select_change(&mut self, staged: bool, path: String, cx: &mut Context<Self>) {
        let key = DiffKey::Worktree {
            path: path.clone(),
            staged,
        };
        if let Some(repo) = &self.repo {
            if repo.diff_key.as_ref() == Some(&key) {
                return;
            }
        }
        if let Some(repo) = &mut self.repo {
            repo.selected_change = Some((staged, path.clone()));
            repo.diff_key = Some(key.clone());
            repo.diff = None;
            repo.diff_loading = true;
        }
        self.send(Job::Diff(key), cx);
    }

    /// 当前页签里「上/下」要移动的那一列，返回是否真的动了。
    pub fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        match self.tab {
            Tab::Commit => {
                let rows = self.change_rows();
                if rows.is_empty() {
                    return false;
                }
                let current = self.repo.as_ref().and_then(|r| r.selected_change.clone());
                let index = current.and_then(|c| rows.iter().position(|r| *r == c));
                let next = match index {
                    Some(i) => (i as isize + delta).clamp(0, rows.len() as isize - 1) as usize,
                    None if delta > 0 => 0,
                    None => rows.len() - 1,
                };
                let (staged, path) = rows[next].clone();
                self.select_change(staged, path, cx);
                true
            }
            Tab::Log => {
                let len = self
                    .repo
                    .as_ref()
                    .map(|r| r.view.len())
                    .unwrap_or(0);
                if len == 0 {
                    return false;
                }
                let current = self.repo.as_ref().and_then(|r| r.selected_commit);
                let next = match current {
                    Some(i) => (i as isize + delta).clamp(0, len as isize - 1) as usize,
                    None if delta > 0 => 0,
                    None => len - 1,
                };
                self.select_commit(next, cx);
                true
            }
            Tab::Branches => {
                let names: Vec<String> = self
                    .repo
                    .as_ref()
                    .map(|r| {
                        r.branches
                            .iter()
                            .filter(|b| self.settings.show_remote_branches || !b.is_remote)
                            .map(|b| b.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                if names.is_empty() {
                    return false;
                }
                let current = self.repo.as_ref().and_then(|r| r.selected_branch.clone());
                let index = current.and_then(|c| names.iter().position(|n| *n == c));
                let next = match index {
                    Some(i) => (i as isize + delta).clamp(0, names.len() as isize - 1) as usize,
                    None if delta > 0 => 0,
                    None => names.len() - 1,
                };
                let name = names[next].clone();
                self.select_branch(name, cx);
                true
            }
            Tab::Stash => {
                let len = self
                    .repo
                    .as_ref()
                    .map(|r| r.stashes.len())
                    .unwrap_or(0);
                if len == 0 {
                    return false;
                }
                let current = self.repo.as_ref().and_then(|r| r.selected_stash);
                let next = match current {
                    Some(i) => (i as isize + delta).clamp(0, len as isize - 1) as usize,
                    None if delta > 0 => 0,
                    None => len - 1,
                };
                self.select_stash(next, cx);
                true
            }
            Tab::Compare | Tab::Console => false,
        }
    }

    fn change_rows(&self) -> Vec<(bool, String)> {
        let Some(status) = self.repo.as_ref().and_then(|r| r.status.clone()) else {
            return Vec::new();
        };
        let mut rows: Vec<(bool, String)> = status
            .staged()
            .map(|c| (true, c.path.clone()))
            .collect();
        rows.extend(status.unstaged().map(|c| (false, c.path.clone())));
        rows
    }

    /// 空格：把当前选中的变更暂存 / 取消暂存。
    pub fn toggle_selected_stage(&mut self, cx: &mut Context<Self>) -> bool {
        if self.tab != Tab::Commit {
            return false;
        }
        let Some((staged, path)) = self.repo.as_ref().and_then(|r| r.selected_change.clone())
        else {
            return false;
        };
        if staged {
            self.unstage(path, cx);
        } else {
            self.stage(path, cx);
        }
        true
    }

    pub fn select_commit(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(repo) = &mut self.repo else { return };
        let Some(commit) = repo.view.get(index) else { return };
        let oid = commit.oid.clone();
        repo.selected_commit = Some(index);
        repo.detail = Some(CommitDetail {
            oid: oid.clone(),
            body: String::new(),
            files: Vec::new(),
            error: None,
            file_diff: None,
            loading: true,
            file_loading: false,
        });
        self.send(Job::CommitDetail(oid), cx);
    }

    pub fn open_commit_file(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(repo) = &mut self.repo else { return };
        let Some(oid) = repo.selected_commit_oid() else { return };
        if let Some(detail) = &mut repo.detail {
            detail.file_loading = true;
        }
        self.send(
            Job::Diff(DiffKey::CommitFile {
                oid,
                path: path.clone(),
            }),
            cx,
        );
    }

    pub fn close_commit_file(&mut self) {
        if let Some(repo) = &mut self.repo {
            if let Some(detail) = &mut repo.detail {
                detail.file_diff = None;
            }
        }
    }

    pub fn select_stash(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(repo) = &mut self.repo {
            repo.selected_stash = Some(index);
        }
        self.send(Job::Diff(DiffKey::Stash(index)), cx);
    }

    // ---- 日志视图 ---------------------------------------------------------

    pub fn rebuild_log(&mut self) {
        let query = self.filter_text.trim().to_lowercase();
        let Some(repo) = &mut self.repo else { return };
        let mut view: Vec<Commit> = if query.is_empty() {
            repo.all_commits.clone()
        } else {
            repo.all_commits
                .iter()
                .filter(|c| {
                    c.summary.to_lowercase().contains(&query)
                        || c.author.to_lowercase().contains(&query)
                        || c.oid.to_lowercase().starts_with(&query)
                })
                .cloned()
                .collect()
        };
        view.shrink_to_fit();
        let (graph, lanes) = graph::layout(&view);
        repo.view = view;
        repo.graph = graph;
        repo.graph_lanes = lanes;
        let len = repo.view.len();
        repo.selected_commit = match repo.selected_commit {
            Some(i) if i < len => Some(i),
            _ if len > 0 => Some(0),
            _ => None,
        };
    }

    // ---- 弹层 -------------------------------------------------------------

    pub fn close_overlays(&mut self) {
        self.modal = None;
        self.menu = None;
    }

    pub fn open_prompt(
        &mut self,
        kind: PromptKind,
        title: &str,
        label: &str,
        hint: &str,
        placeholder: &str,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder.to_string()));
        if !initial.is_empty() {
            input.update(cx, |state, cx| state.set_value(initial, window, cx));
        }
        cx.subscribe(&input, |this: &mut GitApp, _, event, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.submit_prompt(cx);
                cx.notify();
            }
        })
        .detach();
        let handle = input.read(cx).focus_handle(cx).clone();
        self.modal = Some(Modal::Prompt {
            title: title.to_string(),
            label: label.to_string(),
            hint: hint.to_string(),
            input,
            kind,
            danger: false,
        });
        self.menu = None;
        window.focus(&handle, cx);
    }

    pub fn open_confirm(&mut self, kind: ConfirmKind, title: &str, body: String, confirm: &str, danger: bool) {
        self.modal = Some(Modal::Confirm {
            title: title.to_string(),
            body,
            confirm: confirm.to_string(),
            danger,
            kind,
        });
        self.menu = None;
    }

    pub fn submit_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(Modal::Prompt { input, kind, .. }) = &self.modal else {
            return;
        };
        let value = input.read(cx).value().to_string();
        let value = value.trim().to_string();
        let kind = match kind {
            PromptKind::NewBranch { start } => PromptKind::NewBranch { start: start.clone() },
            PromptKind::RenameBranch { old } => PromptKind::RenameBranch { old: old.clone() },
            PromptKind::NewTag { target } => PromptKind::NewTag { target: target.clone() },
            PromptKind::Stash => PromptKind::Stash,
            PromptKind::Clone => PromptKind::Clone,
            PromptKind::InitRepo => PromptKind::InitRepo,
        };
        if value.is_empty() {
            if matches!(kind, PromptKind::Stash) {
                // 贮藏允许空信息
            } else {
                self.set_toast("名字不能为空", Tone::Error, cx);
                return;
            }
        }
        self.modal = None;
        match kind {
            PromptKind::NewBranch { start } => {
                self.run(Action::CreateBranch { name: value, start }, cx);
            }
            PromptKind::RenameBranch { old } => {
                self.run(Action::RenameBranch { old, new: value }, cx);
            }
            PromptKind::NewTag { target } => {
                self.run(Action::CreateTag { name: value, target }, cx);
            }
            PromptKind::Stash => {
                self.run(
                    Action::StashPush {
                        message: value,
                        include_untracked: true,
                    },
                    cx,
                );
            }
            PromptKind::Clone => {
                self.open_clone_dest(value, cx);
            }
            PromptKind::InitRepo => {
                let path = expand_path(&value);
                self.send(Job::InitRepo(path), cx);
            }
        }
    }

    fn open_clone_dest(&mut self, url: String, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择克隆到哪个目录".into()),
        });
        let entity = cx.entity();
        cx.spawn(async move |_weak, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                if let Some(dest) = paths.into_iter().next() {
                    let name = url
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("repo")
                        .trim_end_matches(".git")
                        .to_string();
                    let _ = entity.update(cx, |this, cx| {
                        this.send(
                            Job::CloneRepo {
                                url: url.clone(),
                                dest: dest.join(name),
                            },
                            cx,
                        );
                    });
                }
            }
        })
        .detach();
    }

    pub fn confirm_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Modal::Confirm { kind, .. }) = &self.modal else {
            return;
        };
        let kind = match kind {
            ConfirmKind::DeleteBranch(n) => ConfirmKind::DeleteBranch(n.clone()),
            ConfirmKind::DeleteTag(n) => ConfirmKind::DeleteTag(n.clone()),
            ConfirmKind::DropStash(i) => ConfirmKind::DropStash(*i),
            ConfirmKind::Discard(p) => ConfirmKind::Discard(p.clone()),
            ConfirmKind::Reset { oid, hard } => ConfirmKind::Reset {
                oid: oid.clone(),
                hard: *hard,
            },
            ConfirmKind::RemoveRecent(p) => ConfirmKind::RemoveRecent(p.clone()),
        };
        self.modal = None;
        let _ = window;
        match kind {
            ConfirmKind::DeleteBranch(name) => self.run(Action::DeleteBranch(name), cx),
            ConfirmKind::DeleteTag(name) => self.run(Action::DeleteTag(name), cx),
            ConfirmKind::DropStash(index) => self.run(Action::StashDrop(index), cx),
            ConfirmKind::Discard(paths) => self.run(Action::Discard(paths), cx),
            ConfirmKind::Reset { oid, hard } => self.run(Action::Reset { oid, hard }, cx),
            ConfirmKind::RemoveRecent(path) => {
                self.settings.forget(&path);
                let _ = self.settings.save();
                self.refresh_welcome();
            }
        }
    }

    pub fn open_branch_prompt(&mut self, start: String, window: &mut Window, cx: &mut Context<Self>) {
        self.open_prompt(
            PromptKind::NewBranch { start },
            "新建分支",
            "从当前选中的提交创建分支",
            "分支名，例如 feature/graph-view",
            "分支名",
            "",
            window,
            cx,
        );
    }

    // ---- 菜单 -------------------------------------------------------------

    fn menu_now(&self, ev: &MouseDownEvent) -> (f32, f32) {
        (ev.position.x.into(), ev.position.y.into())
    }

    pub fn open_branch_menu(&mut self, branch: Branch, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let (x, y) = self.menu_now(ev);
        let mut items = Vec::new();
        if !branch.is_current {
            items.push(MenuItem {
                label: format!("检出 {}", branch.name),
                icon: Some(IconName::GitBranch),
                danger: false,
                action: MenuAction::Checkout(branch.name.clone()),
            });
            items.push(MenuItem {
                label: format!("合并到当前分支"),
                icon: Some(IconName::GitMerge),
                danger: false,
                action: MenuAction::Merge(branch.name.clone()),
            });
            items.push(MenuItem {
                label: "把当前分支变基到它上面".into(),
                icon: Some(IconName::GitMerge),
                danger: false,
                action: MenuAction::Rebase(branch.name.clone()),
            });
        }
        items.push(MenuItem {
            label: "从此分支新建分支…".into(),
            icon: Some(IconName::GitBranch),
            danger: false,
            action: MenuAction::NewBranch(branch.name.clone()),
        });
        if !branch.is_remote {
            items.push(MenuItem {
                label: "重命名…".into(),
                icon: Some(IconName::Tag),
                danger: false,
                action: MenuAction::RenameBranch(branch.name.clone()),
            });
            if !branch.is_current {
                items.push(MenuItem {
                    label: "删除".into(),
                    icon: Some(IconName::Trash),
                    danger: true,
                    action: MenuAction::DeleteBranch(branch.name.clone()),
                });
            }
        }
        self.menu = Some(MenuState { x, y, items, w: MENU_W, h: MENU_H, fixed_h: false });
        cx.notify();
    }

    pub fn open_stash_menu(&mut self, stash: Stash, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let (x, y) = self.menu_now(ev);
        self.menu = Some(MenuState {
            x,
            y,
            w: MENU_W,
            h: MENU_H,
            fixed_h: false,
            items: vec![
                MenuItem {
                    label: "应用".into(),
                    icon: Some(IconName::ArrowDownToLine),
                    danger: false,
                    action: MenuAction::ApplyStash(stash.index),
                },
                MenuItem {
                    label: "弹出（应用并删除）".into(),
                    icon: Some(IconName::ArrowUpFromLine),
                    danger: false,
                    action: MenuAction::PopStash(stash.index),
                },
                MenuItem {
                    label: "查看改动".into(),
                    icon: Some(IconName::Search),
                    danger: false,
                    action: MenuAction::ShowStash(stash.index),
                },
                MenuItem {
                    label: "删除".into(),
                    icon: Some(IconName::Trash),
                    danger: true,
                    action: MenuAction::DropStash(stash.index),
                },
            ],
        });
        cx.notify();
    }

    pub fn open_commit_menu(&mut self, commit: Commit, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let (x, y) = self.menu_now(ev);
        let label = format!("{} · {}", commit.short(), commit.summary);
        let _ = label;
        self.menu = Some(MenuState {
            x,
            y,
            w: MENU_W,
            h: MENU_H,
            fixed_h: false,
            items: vec![
                MenuItem {
                    label: "检出这个提交（detached）".into(),
                    icon: Some(IconName::GitCommitHorizontal),
                    danger: false,
                    action: MenuAction::CheckoutDetach(commit.oid.clone()),
                },
                MenuItem {
                    label: "从这里新建分支…".into(),
                    icon: Some(IconName::GitBranch),
                    danger: false,
                    action: MenuAction::NewBranchHere(commit.oid.clone()),
                },
                MenuItem {
                    label: "拣选（cherry-pick）".into(),
                    icon: Some(IconName::GitMerge),
                    danger: false,
                    action: MenuAction::CherryPick(commit.oid.clone()),
                },
                MenuItem {
                    label: "还原（revert）".into(),
                    icon: Some(IconName::Undo2),
                    danger: false,
                    action: MenuAction::Revert(commit.oid.clone()),
                },
                MenuItem {
                    label: "重置当前分支到这里（软）".into(),
                    icon: Some(IconName::RefreshCw),
                    danger: true,
                    action: MenuAction::Reset {
                        oid: commit.oid.clone(),
                        hard: false,
                    },
                },
                MenuItem {
                    label: "重置当前分支到这里（硬，丢弃改动）".into(),
                    icon: Some(IconName::Trash),
                    danger: true,
                    action: MenuAction::Reset {
                        oid: commit.oid.clone(),
                        hard: true,
                    },
                },
                MenuItem {
                    label: "在这里打标签…".into(),
                    icon: Some(IconName::Tag),
                    danger: false,
                    action: MenuAction::Tag(commit.oid.clone()),
                },
                MenuItem {
                    label: "复制完整哈希".into(),
                    icon: None,
                    danger: false,
                    action: MenuAction::CopyHash(commit.oid.clone()),
                },
            ],
        });
        cx.notify();
    }

    pub fn open_tag_menu(&mut self, tag: Tag, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let (x, y) = self.menu_now(ev);
        self.menu = Some(MenuState {
            x,
            y,
            w: MENU_W,
            h: MENU_H,
            fixed_h: false,
            items: vec![
                MenuItem {
                    label: "检出这个标签".into(),
                    icon: Some(IconName::Tag),
                    danger: false,
                    action: MenuAction::CheckoutDetach(tag.oid.clone()),
                },
                MenuItem {
                    label: "推送标签".into(),
                    icon: Some(IconName::ArrowUpFromLine),
                    danger: false,
                    action: MenuAction::PushTag(tag.name.clone()),
                },
                MenuItem {
                    label: "删除标签".into(),
                    icon: Some(IconName::Trash),
                    danger: true,
                    action: MenuAction::DeleteTag(tag.name.clone()),
                },
            ],
        });
        cx.notify();
    }

    pub fn run_menu_action(
        &mut self,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        match action {
            MenuAction::Checkout(name) => self.run(
                Action::Checkout {
                    name,
                    auto_stash: self.settings.auto_stash,
                },
                cx,
            ),
            MenuAction::Merge(name) => self.run(Action::Merge(name), cx),
            MenuAction::Rebase(name) => self.run(Action::Rebase(name), cx),
            MenuAction::NewBranch(start) | MenuAction::NewBranchHere(start) => {
                self.open_branch_prompt(start, window, cx);
            }
            MenuAction::RenameBranch(old) => self.open_prompt(
                PromptKind::RenameBranch { old: old.clone() },
                "重命名分支",
                "输入新的分支名",
                &format!("当前：{old}"),
                "新分支名",
                &old,
                window,
                cx,
            ),
            MenuAction::DeleteBranch(name) => self.open_confirm(
                ConfirmKind::DeleteBranch(name.clone()),
                "删除分支",
                format!("确定要删除分支 {name} 吗？未合并的提交会丢失（等价 git branch -D）。"),
                "删除",
                true,
            ),
            MenuAction::CheckoutDetach(oid) => self.run(Action::CheckoutDetach(oid), cx),
            MenuAction::CherryPick(oid) => self.run(Action::CherryPick(oid), cx),
            MenuAction::Revert(oid) => self.run(Action::Revert(oid), cx),
            MenuAction::Reset { oid, hard } => self.open_confirm(
                ConfirmKind::Reset {
                    oid: oid.clone(),
                    hard,
                },
                if hard { "硬重置" } else { "软重置" },
                format!(
                    "把当前分支{}移动到 {}。",
                    if hard { "硬" } else { "软" },
                    &oid[..oid.len().min(7)]
                ),
                "重置",
                hard,
            ),
            MenuAction::Tag(target) => self.open_prompt(
                PromptKind::NewTag { target },
                "新建标签",
                "标签名",
                "轻量标签，例如 v0.1.0",
                "标签名",
                "",
                window,
                cx,
            ),
            MenuAction::CopyHash(oid) => {
                cx.write_to_clipboard(ClipboardItem::new_string(oid.clone()));
                self.set_toast("已复制哈希", Tone::Info, cx);
            }
            MenuAction::Push => self.run(Action::Push, cx),
            MenuAction::Pull => self.run(Action::Pull, cx),
            MenuAction::Fetch => self.run(Action::Fetch, cx),
            MenuAction::Stage(path) => self.stage(path, cx),
            MenuAction::Unstage(path) => self.unstage(path, cx),
            MenuAction::Discard(path) => self.open_confirm(
                ConfirmKind::Discard(vec![path.clone()]),
                "丢弃改动",
                format!("{path} 的本地改动会被永久丢弃，无法撤销。"),
                "丢弃",
                true,
            ),
            MenuAction::ApplyStash(i) => self.run(Action::StashApply(i), cx),
            MenuAction::PopStash(i) => self.run(Action::StashPop(i), cx),
            MenuAction::DropStash(i) => self.open_confirm(
                ConfirmKind::DropStash(i),
                "删除贮藏",
                format!("stash@{{{i}}} 将被删除。"),
                "删除",
                true,
            ),
            MenuAction::ShowStash(i) => {
                if let Some(repo) = &mut self.repo {
                    repo.selected_stash = Some(i);
                    repo.diff_key = Some(DiffKey::Stash(i));
                    repo.diff = None;
                    repo.diff_loading = true;
                }
                self.send(Job::Diff(DiffKey::Stash(i)), cx);
            }
            MenuAction::DeleteTag(name) => self.open_confirm(
                ConfirmKind::DeleteTag(name.clone()),
                "删除标签",
                format!("确定删除标签 {name}？"),
                "删除",
                true,
            ),
            MenuAction::PushTag(name) => self.run(Action::PushTag(name), cx),
            MenuAction::NewTagHere => {}
            MenuAction::PickCompareRef { target, value } => {
                if target == 0 {
                    self.compare.from = value;
                    self.compare.selected = None;
                    self.compare.file_doc = None;
                    self.compare_refresh(cx);
                } else {
                    self.compare.to = value;
                    self.compare.selected = None;
                    self.compare.file_doc = None;
                    self.compare_refresh(cx);
                }
            }
            MenuAction::PickWorktree => {
                self.compare.to = "工作区".into();
                self.compare.selected = None;
                self.compare.file_doc = None;
                self.compare_refresh(cx);
            }
            MenuAction::SwapCompare => self.swap_compare(cx),
            MenuAction::PickRemote(remote) => {
                if let Some(Modal::Pull(dialog)) = &mut self.modal {
                    dialog.remote = remote;
                    dialog.loading = true;
                }
                let branch = self
                    .repo
                    .as_ref()
                    .map(|r| r.branch())
                    .unwrap_or_default();
                if let Some(Modal::Pull(dialog)) = &mut self.modal {
                    let short_current = branch;
                    let branches: Vec<String> = self
                        .repo
                        .as_ref()
                        .map(|r| {
                            r.branches
                                .iter()
                                .filter(|b| {
                                    b.is_remote && b.remote() == Some(dialog.remote.as_str())
                                })
                                .map(|b| b.short().to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    if !branches.contains(&dialog.branch) {
                        dialog.branch = branches
                            .iter()
                            .find(|b| **b == short_current)
                            .cloned()
                            .or_else(|| branches.first().cloned())
                            .unwrap_or_default();
                    }
                }
                let mut dialog = match self.modal.take() {
                    Some(Modal::Pull(dialog)) => dialog,
                    other => {
                        self.modal = other;
                        return;
                    }
                };
                self.compute_pull_distance(&mut dialog, cx);
                self.modal = Some(Modal::Pull(dialog));
                cx.notify();
            }
            MenuAction::PickPullBranch(branch) => {
                if let Some(Modal::Pull(dialog)) = &mut self.modal {
                    dialog.branch = branch;
                    dialog.loading = true;
                }
                let mut dialog = match self.modal.take() {
                    Some(Modal::Pull(dialog)) => dialog,
                    other => {
                        self.modal = other;
                        return;
                    }
                };
                self.compute_pull_distance(&mut dialog, cx);
                self.modal = Some(Modal::Pull(dialog));
                cx.notify();
            }
            MenuAction::PickPullStrategy(rebase) => {
                if let Some(Modal::Pull(dialog)) = &mut self.modal {
                    dialog.rebase = rebase;
                }
                cx.notify();
            }
            MenuAction::ToggleAutoStash => {
                if let Some(Modal::Pull(dialog)) = &mut self.modal {
                    dialog.auto_stash = !dialog.auto_stash;
                }
                cx.notify();
            }
            MenuAction::FetchRemote(remote) => self.run(Action::FetchRemote(remote), cx),
            MenuAction::CompareFrom(oid) => {
                self.compare.from = oid;
                self.compare.to = "HEAD".into();
                self.compare.selected = None;
                self.compare.file_doc = None;
                self.compare_refresh(cx);
            }
        }
    }

    // ---- 版本对比 ---------------------------------------------------------

    /// 对比页第一次打开时给一对合理默认值：上一个提交 → 工作区。
    fn ensure_compare_defaults(&mut self) {
        if !self.compare.from.is_empty() && !self.compare.to.is_empty() {
            return;
        }
        let (from, to) = match &self.repo {
            Some(repo) => {
                let from = repo
                    .view
                    .get(1)
                    .map(|c| c.oid.clone())
                    .or_else(|| repo.view.first().map(|c| c.oid.clone()))
                    .unwrap_or_else(|| "HEAD".to_string());
                (from, "工作区".to_string())
            }
            None => ("HEAD".to_string(), "工作区".to_string()),
        };
        self.compare.from = from;
        self.compare.to = to;
    }

    pub fn compare_refresh(&mut self, cx: &mut Context<Self>) {
        self.ensure_compare_defaults();
        if self.repo.is_none() {
            return;
        }
        let (from, to) = (self.compare.from.clone(), self.compare.to.clone());
        self.compare.loading = true;
        self.compare.error = None;
        self.send(Job::Compare { from, to }, cx);
    }

    pub fn select_compare_file(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        self.compare.selected = path.clone();
        self.compare.file_doc = None;
        match path {
            None => {
                self.compare.file_doc = None;
                cx.notify();
            }
            Some(path) => {
                let key = DiffKey::RangeFile {
                    from: self.compare.from.clone(),
                    to: self.compare.to.clone(),
                    path,
                };
                self.send(Job::Diff(key), cx);
            }
        }
    }

    pub fn swap_compare(&mut self, cx: &mut Context<Self>) {
        if self.compare.to == "工作区" {
            self.set_toast("工作区不能作为起点", Tone::Info, cx);
            return;
        }
        std::mem::swap(&mut self.compare.from, &mut self.compare.to);
        self.compare.files.clear();
        self.compare.doc = None;
        self.compare.file_doc = None;
        self.compare.selected = None;
        self.compare_refresh(cx);
    }

    /// 打开「选择版本」菜单；target 0=起点，1=终点。
    pub fn open_compare_ref_menu(
        &mut self,
        target: u8,
        ev: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let (x, y) = self.menu_now(ev);
        let Some(repo) = &self.repo else { return };
        let mut items: Vec<MenuItem> = Vec::new();
        if target == 1 {
            items.push(MenuItem {
                label: "工作区（未提交的改动）".into(),
                icon: Some(IconName::FolderGit),
                danger: false,
                action: MenuAction::PickWorktree,
            });
        }
        items.push(MenuItem {
            label: "HEAD（当前提交）".into(),
            icon: Some(IconName::GitCommitHorizontal),
            danger: false,
            action: MenuAction::PickCompareRef {
                target,
                value: "HEAD".into(),
            },
        });
        let locals: Vec<&Branch> = repo.branches.iter().filter(|b| !b.is_remote).collect();
        for branch in locals {
            items.push(MenuItem {
                label: format!("分支  {}", branch.name),
                icon: Some(IconName::GitBranch),
                danger: false,
                action: MenuAction::PickCompareRef {
                    target,
                    value: branch.name.clone(),
                },
            });
        }
        for branch in repo.branches.iter().filter(|b| b.is_remote) {
            items.push(MenuItem {
                label: format!("远端  {}", branch.name),
                icon: Some(IconName::GitBranch),
                danger: false,
                action: MenuAction::PickCompareRef {
                    target,
                    value: branch.name.clone(),
                },
            });
        }
        for tag in &repo.tags {
            items.push(MenuItem {
                label: format!("标签  {}", tag.name),
                icon: Some(IconName::Tag),
                danger: false,
                action: MenuAction::PickCompareRef {
                    target,
                    value: tag.name.clone(),
                },
            });
        }
        for commit in repo.view.iter().take(20) {
            items.push(MenuItem {
                label: format!("{}  {}", commit.short(), commit.summary),
                icon: Some(IconName::GitCommitHorizontal),
                danger: false,
                action: MenuAction::PickCompareRef {
                    target,
                    value: commit.oid.clone(),
                },
            });
        }
        self.menu = Some(MenuState { x, y, items, w: MENU_W, h: MENU_H, fixed_h: false });
        cx.notify();
    }

    // ---- 从其他远端拉取 ---------------------------------------------------

    pub fn open_pull_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = &self.repo else { return };
        let current = repo.branch();
        let upstream = repo
            .branches
            .iter()
            .find(|b| b.is_current)
            .and_then(|b| b.upstream.clone());
        let default_remote = upstream
            .as_ref()
            .and_then(|u| u.split_once('/').map(|(r, _)| r.to_string()))
            .or_else(|| self.remotes.first().map(|r| r.name.clone()))
            .unwrap_or_else(|| "origin".to_string());
        let default_branch = upstream
            .as_ref()
            .and_then(|u| u.split_once('/').map(|(_, b)| b.to_string()))
            .unwrap_or_else(|| current.clone());

        let mut dialog = PullDialog {
            remote: default_remote,
            branch: default_branch,
            rebase: self.settings.pull_rebase,
            auto_stash: self.settings.auto_stash,
            ahead: None,
            behind: None,
            loading: true,
        };
        self.compute_pull_distance(&mut dialog, cx);
        self.modal = Some(Modal::Pull(dialog));
        cx.notify();
    }

    fn compute_pull_distance(&mut self, dialog: &mut PullDialog, cx: &mut Context<Self>) {
        let current = self
            .repo
            .as_ref()
            .map(|r| r.branch())
            .unwrap_or_default();
        if current.is_empty() || dialog.remote.is_empty() || dialog.branch.is_empty() {
            dialog.loading = false;
            return;
        }
        dialog.loading = true;
        self.send(
            Job::AheadBehind {
                left: current,
                right: format!("{}/{}", dialog.remote, dialog.branch),
            },
            cx,
        );
    }

    fn pull_dialog_branches(&self) -> Vec<String> {
        let Some(repo) = &self.repo else { return Vec::new() };
        let remote = match &self.modal {
            Some(Modal::Pull(dialog)) => dialog.remote.clone(),
            _ => return Vec::new(),
        };
        repo.branches
            .iter()
            .filter(|b| b.is_remote && b.remote() == Some(remote.as_str()))
            .map(|b| b.short().to_string())
            .collect()
    }

    pub fn open_pull_remote_menu(&mut self, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let (x, y) = self.menu_now(ev);
        let items: Vec<MenuItem> = self
            .remotes
            .iter()
            .map(|remote| MenuItem {
                label: format!("{}  {}", remote.name, remote.url),
                icon: Some(IconName::FolderGit2),
                danger: false,
                action: MenuAction::PickRemote(remote.name.clone()),
            })
            .collect();
        if items.is_empty() {
            self.set_toast("这个仓库没有配置远端", Tone::Error, cx);
            return;
        }
        self.menu = Some(MenuState { x, y, items, w: MENU_W, h: MENU_H, fixed_h: false });
        cx.notify();
    }

    pub fn open_pull_branch_menu(&mut self, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let (x, y) = self.menu_now(ev);
        let items: Vec<MenuItem> = self
            .pull_dialog_branches()
            .into_iter()
            .map(|branch| MenuItem {
                label: branch.clone(),
                icon: Some(IconName::GitBranch),
                danger: false,
                action: MenuAction::PickPullBranch(branch),
            })
            .collect();
        if items.is_empty() {
            self.set_toast("先选一个有分支的远端", Tone::Info, cx);
            return;
        }
        self.menu = Some(MenuState { x, y, items, w: MENU_W, h: MENU_H, fixed_h: false });
        cx.notify();
    }

    pub fn pull_now(&mut self, cx: &mut Context<Self>) {
        let Some(Modal::Pull(dialog)) = self.modal.take() else {
            return;
        };
        self.settings.pull_rebase = dialog.rebase;
        self.settings.auto_stash = dialog.auto_stash;
        let _ = self.settings.save();
        self.run(
            Action::PullFrom {
                remote: dialog.remote,
                branch: dialog.branch,
                rebase: dialog.rebase,
            },
            cx,
        );
    }

    // ---- 键盘 -------------------------------------------------------------

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let key = ev.keystroke.key.as_str();
        let m = ev.keystroke.modifiers;
        if key == "escape" {
            if self.modal.is_some() || self.menu.is_some() {
                self.close_overlays();
                cx.notify();
                return true;
            }
            return false;
        }
        if self.modal.is_some() {
            if key == "enter" {
                match &self.modal {
                    Some(Modal::Prompt { .. }) => self.submit_prompt(cx),
                    Some(Modal::Confirm { .. }) => self.confirm_modal(window, cx),
                    Some(Modal::Pull(_)) => self.pull_now(cx),
                    None => {}
                }
                cx.notify();
                return true;
            }
            return false;
        }
        // 不用修饰键的导航键：列表里上下移动，空格切换暂存。
        if !m.platform && !m.control && !m.alt {
            match key {
                "up" => {
                    if self.move_selection(-1, cx) {
                        return true;
                    }
                }
                "down" => {
                    if self.move_selection(1, cx) {
                        return true;
                    }
                }
                "space" => {
                    if self.toggle_selected_stage(cx) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        if !m.platform {
            return false;
        }
        if m.shift {
            match key {
                "k" => {
                    self.run(Action::Push, cx);
                    return true;
                }
                "f" => {
                    self.run(Action::Fetch, cx);
                    return true;
                }
                _ => {}
            }
        }
        match key {
            "r" => {
                self.refresh(cx);
                true
            }
            "t" => {
                self.open_pull_dialog(cx);
                true
            }
            "d" => {
                self.tab = Tab::Compare;
                self.compare_refresh(cx);
                true
            }
            "enter" => {
                self.commit(cx);
                true
            }
            "o" => {
                self.pick_repo(window, cx);
                true
            }
            "n" => {
                let start = self
                    .repo
                    .as_ref()
                    .map(|r| r.branch())
                    .unwrap_or_else(|| "HEAD".into());
                self.open_branch_prompt(start, window, cx);
                true
            }
            "1" | "2" | "3" | "4" | "5" | "6" => {
                let index = key.parse::<usize>().unwrap_or(1) - 1;
                self.tab = Tab::ALL[index];
                cx.notify();
                true
            }
            _ => false,
        }
    }

    // ---- 事件处理 ---------------------------------------------------------

    fn handle(&mut self, payload: Payload, cx: &mut Context<Self>) {
        match payload {
            Payload::Opened { path, console } => {
                self.busy = self.busy.saturating_sub(1);
                self.append_console(console);
                match path {
                    Ok(root) => {
                        let name = worker::repo_name(&root);
                        self.settings.touch(&root);
                        let _ = self.settings.save();
                        self.repo = Some(RepoState::new(root, name));
                        self.compare = CompareState::default();
                        self.set_toast("仓库已打开", Tone::Info, cx);
                        self.refresh(cx);
                    }
                    Err(err) => self.set_toast(err, Tone::Error, cx),
                }
            }
            Payload::Status(result) => {
                self.busy = self.busy.saturating_sub(1);
                match result {
                    Ok(status) => {
                        if let Some(repo) = &mut self.repo {
                            repo.status = Some(status);
                            repo.status_loading = false;
                        }
                        // 选中项被提交/丢弃后，自动落到下一个可看的变更上。
                        let next = self.repo.as_ref().and_then(|repo| {
                            let status = repo.status.as_ref()?;
                            if let Some((staged, path)) = &repo.selected_change {
                                let (list, staged) = if *staged {
                                    (status.staged().map(|c| &c.path).collect::<Vec<_>>(), true)
                                } else {
                                    (status.unstaged().map(|c| &c.path).collect::<Vec<_>>(), false)
                                };
                                if staged && list.iter().any(|p| *p == path) {
                                    return None;
                                }
                                if !staged && list.iter().any(|p| *p == path) {
                                    return None;
                                }
                            }
                            if let Some(change) = status.staged().next() {
                                return Some((true, change.path.clone()));
                            }
                            status.unstaged().next().map(|c| (false, c.path.clone()))
                        });
                        if let Some((staged, path)) = next {
                            self.select_change(staged, path, cx);
                        }
                    }
                    Err(err) => self.set_toast(err, Tone::Error, cx),
                }
            }
            Payload::Log(result) => {
                self.busy = self.busy.saturating_sub(1);
                match result {
                    Ok(commits) => {
                        if let Some(repo) = &mut self.repo {
                            repo.all_commits = commits;
                        }
                        let first_load = self
                            .repo
                            .as_ref()
                            .map(|r| r.view.is_empty())
                            .unwrap_or(false);
                        self.rebuild_log();
                        if self.compare.from.trim().is_empty() {
                            // 默认对比 HEAD 的父提交与工作区：一进对比页就看到最近的改动。
                            if let Some(repo) = &self.repo {
                                let from = repo
                                    .view
                                    .get(1)
                                    .map(|c| c.oid.clone())
                                    .or_else(|| repo.view.first().map(|c| c.oid.clone()))
                                    .unwrap_or_else(|| "HEAD".to_string());
                                self.compare.from = from;
                                if self.compare.to.trim().is_empty() {
                                    self.compare.to = "工作区".to_string();
                                }
                            }
                        }
                        if self.tab == Tab::Compare
                            && self.compare.doc.is_none()
                            && !self.compare.loading
                            && !self.compare.from.trim().is_empty()
                        {
                            self.compare_refresh(cx);
                        }
                        if first_load {
                            if let Some(repo) = &self.repo {
                                if !repo.view.is_empty() && repo.selected_commit.is_none() {
                                    self.select_commit(0, cx);
                                }
                            }
                        }
                    }
                    Err(err) => self.set_toast(err, Tone::Error, cx),
                }
            }
            Payload::Detail { oid, result } => {
                self.busy = self.busy.saturating_sub(1);
                match result {
                    Ok((body, files)) => {
                        if let Some(repo) = &mut self.repo {
                            let matches = repo
                                .detail
                                .as_ref()
                                .map(|d| d.oid == oid)
                                .unwrap_or(false);
                            if matches {
                                if let Some(detail) = &mut repo.detail {
                                    detail.body = body;
                                    detail.files = files;
                                    detail.loading = false;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        if let Some(repo) = &mut self.repo {
                            if let Some(detail) = &mut repo.detail {
                                if detail.oid == oid {
                                    detail.error = Some(err);
                                    detail.loading = false;
                                }
                            }
                        }
                    }
                }
            }
            Payload::Diff { key, doc } => {
                if let DiffKey::RangeFile { path, .. } = &key {
                    self.busy = self.busy.saturating_sub(1);
                    match doc {
                        Ok(doc) => {
                            let subtitle = format!("{} → {}", self.compare.from, self.compare.to);
                            let title = path.clone();
                            self.compare.file_doc =
                                Some(DiffPane::from_doc(title, subtitle, &doc));
                        }
                        Err(err) => {
                            self.compare.file_doc = Some(DiffPane {
                                title: path.clone(),
                                subtitle: err,
                                added: 0,
                                removed: 0,
                                rows: Vec::new(),
                                truncated: false,
                                empty: Some("读取失败".into()),
                                serial: 0,
                            });
                        }
                    }
                    return;
                }
                self.busy = self.busy.saturating_sub(1);
                self.diff_serial += 1;
                let serial = self.diff_serial;
                match (&key, doc) {
                    (DiffKey::CommitFile { path, .. }, doc) => {
                        let pane = match doc {
                            Ok(doc) => {
                                let mut pane = DiffPane::from_doc(path.clone(), "提交中的文件".into(), &doc);
                                pane.serial = serial;
                                pane
                            }
                            Err(err) => DiffPane {
                                title: path.clone(),
                                subtitle: "读取失败".into(),
                                added: 0,
                                removed: 0,
                                rows: Vec::new(),
                                truncated: false,
                                empty: Some(err),
                                serial,
                            },
                        };
                        if let Some(repo) = &mut self.repo {
                            if let Some(detail) = &mut repo.detail {
                                detail.file_diff = Some(pane);
                                detail.file_loading = false;
                            }
                        }
                    }
                    (other_key, doc) => {
                        let title = match other_key {
                            DiffKey::Worktree { path, .. } => path.clone(),
                            DiffKey::Stash(i) => format!("stash@{{{i}}}"),
                            DiffKey::CommitFile { path, .. } => path.clone(),
                            DiffKey::RangeFile { path, .. } => path.clone(),
                        };
                        let staged = matches!(other_key, DiffKey::Worktree { staged: true, .. });
                        let subtitle = match other_key {
                            DiffKey::Worktree { staged, .. } => {
                                if *staged {
                                    "已暂存".to_string()
                                } else {
                                    "工作区".to_string()
                                }
                            }
                            DiffKey::Stash(_) => "贮藏内容".to_string(),
                            _ => String::new(),
                        };
                        let _ = staged;
                        let pane = match doc {
                            Ok(doc) => {
                                let mut pane = DiffPane::from_doc(title, subtitle, &doc);
                                pane.serial = serial;
                                pane
                            }
                            Err(err) => DiffPane {
                                title,
                                subtitle: "读取失败".into(),
                                added: 0,
                                removed: 0,
                                rows: Vec::new(),
                                truncated: false,
                                empty: Some(err),
                                serial,
                            },
                        };
                        if let Some(repo) = &mut self.repo {
                            if repo.diff_key.as_ref() == Some(&key) {
                                repo.diff = Some(pane);
                                repo.diff_loading = false;
                            }
                        }
                    }
                }
            }
            Payload::Branches(result) => {
                self.busy = self.busy.saturating_sub(1);
                match result {
                    Ok(branches) => {
                        if let Some(repo) = &mut self.repo {
                            let current = branches
                                .iter()
                                .find(|b| b.is_current)
                                .map(|b| b.name.clone());
                            let has_selection = repo
                                .selected_branch
                                .as_ref()
                                .map(|n| branches.iter().any(|b| &b.name == n))
                                .unwrap_or(false);
                            if !has_selection {
                                repo.selected_branch = current.or_else(|| {
                                    branches.iter().find(|b| !b.is_remote).map(|b| b.name.clone())
                                });
                            }
                            repo.branches = branches;
                        }
                    }
                    Err(err) => self.set_toast(err, Tone::Error, cx),
                }
            }
            Payload::Stashes(result) => {
                self.busy = self.busy.saturating_sub(1);
                match result {
                    Ok(stashes) => {
                        if let Some(repo) = &mut self.repo {
                            repo.stashes = stashes;
                            repo.selected_stash = repo
                                .selected_stash
                                .filter(|i| *i < repo.stashes.len());
                        }
                        let auto = self
                            .repo
                            .as_ref()
                            .and_then(|repo| (!repo.stashes.is_empty() && repo.selected_stash.is_none()).then_some(0));
                        if let Some(index) = auto {
                            self.select_stash(index, cx);
                        }
                    }
                    Err(err) => self.set_toast(err, Tone::Error, cx),
                }
            }
            Payload::Tags(result) => {
                self.busy = self.busy.saturating_sub(1);
                if let Ok(tags) = result {
                    if let Some(repo) = &mut self.repo {
                        repo.tags = tags;
                    }
                }
            }
            Payload::Remotes(result) => {
                self.busy = self.busy.saturating_sub(1);
                match result {
                    Ok(remotes) => self.remotes = remotes,
                    Err(err) => self.set_toast(err, Tone::Error, cx),
                }
            }
            Payload::Compare { from, to, result } => {
                self.busy = self.busy.saturating_sub(1);
                if self.compare.from != from || self.compare.to != to {
                    return;
                }
                self.compare.loading = false;
                match result {
                    Ok((files, doc)) => {
                        self.compare_seq += 1;
                        self.compare.files = files;
                        self.compare.error = None;
                        if self.compare.selected.is_none() {
                            let subtitle = format!("{from} → {to}");
                            self.compare.doc =
                                Some(DiffPane::from_doc(subtitle, String::new(), &doc));
                        }
                    }
                    Err(err) => {
                        self.compare.error = Some(err);
                        self.compare.files.clear();
                        self.compare.doc = None;
                    }
                }
            }
            Payload::AheadBehind { left, right, result } => {
                self.busy = self.busy.saturating_sub(1);
                if let Some(Modal::Pull(dialog)) = &mut self.modal {
                    dialog.loading = false;
                    match result {
                        Ok((ahead, behind)) if !left.is_empty() => {
                            dialog.ahead = Some(ahead);
                            dialog.behind = Some(behind);
                            let _ = right;
                        }
                        _ => {}
                    }
                }
            }
            Payload::Acted { label, result } => {
                self.busy = self.busy.saturating_sub(1);
                match result {
                    Ok(text) => {
                        let has_conflict = text.contains("冲突")
                            || text.contains("CONFLICT")
                            || text.contains("失败");
                        let tone = if has_conflict { Tone::Error } else { Tone::Success };
                        let msg = if text.trim().is_empty() {
                            format!("{label} 完成")
                        } else {
                            text
                        };
                        self.set_toast(msg, tone, cx);
                        if label.starts_with("提交") || label.starts_with("修正") {
                            self.clear_message_next = self.settings.clear_message_after_commit;
                            self.amend = false;
                        }
                        self.refresh(cx);
                    }
                    Err(err) => {
                        self.set_toast(format!("{label}失败：{err}"), Tone::Error, cx);
                    }
                }
            }
            Payload::Console(outputs) => {
                self.append_console(outputs);
            }
            Payload::Fatal(err) => {
                self.busy = self.busy.saturating_sub(1);
                self.set_toast(err, Tone::Error, cx);
            }
        }
    }

    // ---- 顶栏 / 侧栏 / 状态栏 ---------------------------------------------

    fn title_bar(&self, cx: &mut Context<Self>) -> Div {
        let (name, path, branch, ahead, behind) = match &self.repo {
            Some(repo) => (
                repo.name.clone(),
                repo.root.display().to_string(),
                repo.branch(),
                repo.ahead(),
                repo.behind(),
            ),
            None => ("没有打开的仓库".into(), String::new(), String::new(), 0, 0),
        };
        let has_repo = self.repo.is_some();

        let mut bar = div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(46.))
            .pt(px(6.))
            .gap(px(8.))
            .bg(theme::panel())
            .border_b(px(1.))
            .border_color(theme::border());

        // macOS 上给窗口左上角红绿灯留位置；Windows 用自绘的窗口按钮，放右侧。
        if cfg!(target_os = "macos") {
            bar = bar.pl(px(78.)).pr(px(8.));
        } else {
            bar = bar.pl(px(10.)).pr(px(0.));
        }

        bar = bar.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .min_w(px(0.))
                .on_mouse_down(MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
                .child(Icon::new(IconName::FolderGit2).small().text_color(theme::accent()))
                .child(
                    div()
                        .text_size(fs(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(name),
                )
                .when(!path.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(fs(11.))
                            .text_color(theme::faint())
                            .child(path),
                    )
                }),
        );

        if has_repo {
            bar = bar.child(widgets::vdivider());
            let branch_color = if branch == "detached HEAD" {
                theme::amber()
            } else {
                theme::green()
            };
            let mut pill = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.))
                .h(px(22.))
                .px(px(7.))
                .rounded(px(4.))
                .bg(theme::panel_alt())
                .border_1()
                .border_color(theme::border())
                .child(Icon::new(IconName::GitBranch).xsmall().text_color(branch_color))
                .child(
                    div()
                        .text_size(fs(11.5))
                        .text_color(theme::text())
                        .child(branch),
                );
            if ahead > 0 {
                pill = pill.child(widgets::ref_pill(format!("↑{ahead}"), theme::blue()));
            }
            if behind > 0 {
                pill = pill.child(widgets::ref_pill(format!("↓{behind}"), theme::amber()));
            }
            bar = bar.child(pill);

            bar = bar.child(
                div()
                    .flex_1()
                    .h_full()
                    .on_mouse_down(MouseButton::Left, |_, window, _| {
                        window.start_window_move();
                    }),
            );

            bar = bar.child(
                widgets::toolbar_button("fetch", IconName::RefreshCw, "抓取")
                    .on_click(cx.listener(|this, _, _, cx| this.run(Action::Fetch, cx))),
            );
            bar = bar.child(
                widgets::toolbar_button("pull", IconName::ArrowDownToLine, "拉取…")
                    .on_click(cx.listener(|this, _, _, cx| this.open_pull_dialog(cx))),
            );
            bar = bar.child(
                widgets::toolbar_button("push", IconName::ArrowUpFromLine, "推送")
                    .on_click(cx.listener(|this, _, _, cx| this.run(Action::Push, cx))),
            );
            bar = bar.child(widgets::vdivider());
            bar = bar.child(
                widgets::toolbar_button("refresh", IconName::RefreshCw, "刷新")
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            );
            bar = bar.child(
                widgets::primary_button("topcommit", IconName::Check, "提交")
                    .on_click(cx.listener(|this, _, _, cx| this.commit(cx))),
            );
            bar = bar.child(
                widgets::icon_button("repo-switch", IconName::FolderGit, theme::dim())
                    .on_click(cx.listener(|this, _, window, cx| this.pick_repo(window, cx))),
            );
        } else {
            bar = bar.child(
                div()
                    .flex_1()
                    .h_full()
                    .on_mouse_down(MouseButton::Left, |_, window, _| {
                        window.start_window_move();
                    }),
            );
            bar = bar.child(
                widgets::primary_button("open-repo", IconName::FolderGit2, "打开仓库")
                    .on_click(cx.listener(|this, _, window, cx| this.pick_repo(window, cx))),
            );
        }
        #[cfg(target_os = "windows")]
        {
            bar = bar.child(div().w(px(6.)).flex_none());
            bar = bar.child(window_button("win-min", IconName::WindowMinimize, false).on_click(
                cx.listener(|_, _, window, _| window.minimize_window()),
            ));
            bar = bar.child(window_button("win-max", IconName::WindowMaximize, false).on_click(
                cx.listener(|_, _, window, _| window.zoom_window()),
            ));
            bar = bar.child(window_button("win-close", IconName::X, true).on_click(
                cx.listener(|_, _, window, _| window.remove_window()),
            ));
        }
        bar
    }

    pub fn pick_repo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择 Git 仓库目录".into()),
        });
        let entity = cx.entity();
        cx.spawn(async move |_weak, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = entity.update(cx, |this, cx| this.open_repo(path, cx));
                }
            }
        })
        .detach();
    }

    fn rail(&self, cx: &mut Context<Self>) -> Div {
        let mut rail = div()
            .flex()
            .flex_col()
            .items_center()
            .w(px(58.))
            .h_full()
            .py(px(6.))
            .bg(theme::panel())
            .border_r(px(1.))
            .border_color(theme::border());

        for tab in Tab::ALL {
            let active = self.tab == tab;
            rail = rail.child(
                widgets::rail_button(
                    match tab {
                        Tab::Commit => "rail-commit",
                        Tab::Compare => "rail-compare",
                        Tab::Log => "rail-log",
                        Tab::Branches => "rail-branches",
                        Tab::Stash => "rail-stash",
                        Tab::Console => "rail-console",
                    },
                    widgets::tab_icon(tab),
                    widgets::tab_label(tab),
                    active,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = tab;
                    if tab == Tab::Compare
                        && this.repo.is_some()
                        && this.compare.doc.is_none()
                        && !this.compare.loading
                    {
                        this.compare_refresh(cx);
                    }
                    cx.notify();
                })),
            );
        }
        rail = rail.child(div().flex_1());
        rail = rail.child(
            widgets::rail_button("rail-repo", IconName::FolderGit2, "仓库", false)
                .on_click(cx.listener(|this, _, window, cx| this.pick_repo(window, cx))),
        );
        rail
    }

    fn status_bar(&self, cx: &mut Context<Self>) -> Div {
        let branch = self
            .repo
            .as_ref()
            .map(|r| r.branch())
            .unwrap_or_else(|| "—".to_string());
        let changes = self
            .repo
            .as_ref()
            .map(|r| r.changes().len())
            .unwrap_or(0);
        let busy = self.busy > 0;

        let mut bar = div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(24.))
            .px(px(10.))
            .gap(px(8.))
            .bg(theme::panel())
            .border_t(px(1.))
            .border_color(theme::border())
            .text_size(fs(11.))
            .text_color(theme::dim());

        bar = bar.child(Icon::new(IconName::GitBranch).xsmall().text_color(theme::faint()));
        bar = bar.child(
            div()
                .text_color(theme::text())
                .child(branch),
        );
        if let Some(repo) = &self.repo {
            if !repo.root.as_os_str().is_empty() {
                bar = bar.child(
                    div()
                        .text_color(theme::faint())
                        .child(repo.root.display().to_string()),
                );
            }
        }
        if changes > 0 {
            bar = bar.child(
                div()
                    .text_color(theme::amber())
                    .child(format!("{changes} 个改动")),
            );
        }
        bar = bar.child(div().flex_1());
        if busy {
            bar = bar.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(5.))
                    .text_color(theme::blue())
                    .child(Icon::new(IconName::RefreshCw).xsmall().text_color(theme::blue()))
                    .child("Git 运行中…"),
            );
        }
        bar = bar.child(div().text_color(theme::faint()).child(self.git_version.clone()));
        let _ = cx;
        bar
    }

    // ---- 浮层 -------------------------------------------------------------

    fn overlay_layers(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut layers = Vec::new();
        // 菜单单独留着，等模态层入栈后再推：拉取对话框里的远端/分支菜单要浮在遮罩之上。
        let mut menu_layer: Option<AnyElement> = None;

        if let Some(menu) = &self.menu {
            let mut list = div()
                .id("menu-list")
                .flex()
                .flex_col()
                .max_h(px((menu.h - 8.).max(MENU_MIN_H - 8.)))
                .overflow_y_scroll();
            for (index, item) in menu.items.iter().enumerate() {
                let action = item.action.clone();
                let color = if item.danger { theme::red() } else { theme::text() };
                let mut row = div()
                    .id(("menu-item", index))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .h(px(26.))
                    .px(px(10.))
                    .text_size(fs(12.))
                    .text_color(color)
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme::selection()));
                if let Some(icon) = item.icon {
                    row = row.child(Icon::new(icon).xsmall().text_color(color));
                }
                row = row.child(item.label.clone());
                list = list.child(row.on_click(cx.listener(move |this, _, window, cx| {
                    this.menu = None;
                    this.run_menu_action(action.clone(), window, cx);
                })));
            }

            // 右下角拖拽手柄：调整菜单宽高，长列表可拉大。
            let handle = div()
                .id("menu-resize")
                .absolute()
                .right(px(0.))
                .bottom(px(0.))
                .w(px(14.))
                .h(px(14.))
                .cursor(CursorStyle::ResizeUpLeftDownRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                        if let Some(menu) = &mut this.menu {
                            if !menu.fixed_h {
                                // 第一次拖高度：从当前内容高度开始，避免菜单突然跳到最大高度。
                                menu.h = (menu.items.len() as f32 * 26. + 8.)
                                    .clamp(MENU_MIN_H, MENU_MAX_H);
                                menu.fixed_h = true;
                            }
                            this.menu_resize = Some(MenuResize {
                                start_x: ev.position.x.into(),
                                start_y: ev.position.y.into(),
                                w0: menu.w,
                                h0: menu.h,
                            });
                            cx.notify();
                        }
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .right(px(2.))
                        .bottom(px(2.))
                        .w(px(7.))
                        .h(px(7.))
                        .border_r(px(1.))
                        .border_b(px(1.))
                        .border_color(theme::dim()),
                );

            let mut panel = div()
                .absolute()
                .left(px(menu.x))
                .top(px(menu.y))
                .w(px(menu.w))
                .max_h(px(menu.h))
                .flex()
                .flex_col()
                .rounded(px(6.))
                .bg(theme::panel())
                .border_1()
                .border_color(theme::border())
                .shadow_lg()
                .py(px(4.))
                .occlude()
                .child(list)
                .child(handle);
            if menu.fixed_h {
                panel = panel.h(px(menu.h));
            }
            menu_layer = Some(panel.into_any_element());
        }

        if let Some(modal) = &self.modal {
            let scrim = div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme::scrim())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.modal = None;
                        this.menu = None;
                        cx.notify();
                    }),
                );

            let card = match modal {
                Modal::Prompt {
                    title,
                    label,
                    hint,
                    input,
                    ..
                } => div()
                    .occlude()
                    .w(px(420.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(16.))
                    .rounded(px(8.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::border())
                    .shadow_lg()
                    .child(
                        div()
                            .text_size(fs(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .text_size(fs(11.5))
                            .text_color(theme::dim())
                            .child(label.clone()),
                    )
                    .child(Input::new(input))
                    .child(
                        div()
                            .text_size(fs(10.5))
                            .text_color(theme::faint())
                            .child(hint.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap(px(8.))
                            .child(widgets::toolbar_button("cancel", IconName::X, "取消").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.modal = None;
                                    cx.notify();
                                }),
                            ))
                            .child(
                                widgets::primary_button("ok", IconName::Check, "确定").on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.submit_prompt(cx);
                                        cx.notify();
                                    }),
                                ),
                            ),
                    ),
                Modal::Confirm {
                    title,
                    body,
                    confirm,
                    danger,
                    ..
                } => {
                    let confirm_label = confirm.clone();
                    let danger = *danger;
                    let mut card = div()
                        .occlude()
                        .w(px(430.))
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .p(px(16.))
                        .rounded(px(8.))
                        .bg(theme::panel())
                        .border_1()
                        .border_color(theme::border())
                        .shadow_lg()
                        .child(
                            div()
                                .text_size(fs(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::text())
                                .child(title.clone()),
                        )
                        .child(
                            div()
                                .text_size(fs(11.5))
                                .text_color(theme::dim())
                                .child(body.clone()),
                        );
                    let mut ok = div()
                        .id("confirm-ok")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .h(px(26.))
                        .px(px(12.))
                        .rounded(px(5.))
                        .text_size(fs(12.))
                        .text_color(rgb(0xffffff))
                        .cursor_pointer()
                        .child(confirm_label);
                    ok = if danger {
                        ok.bg(theme::red()).hover(|s| s.bg(rgb(0xe06a6a)))
                    } else {
                        ok.bg(theme::accent()).hover(|s| s.bg(theme::accent_hover()))
                    };
                    card = card.child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap(px(8.))
                            .child(widgets::toolbar_button("cancel2", IconName::X, "取消").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.modal = None;
                                    cx.notify();
                                }),
                            ))
                            .child(ok.on_click(cx.listener(|this, _, window, cx| {
                                this.confirm_modal(window, cx);
                                cx.notify();
                            }))),
                    );
                    card
                }
                Modal::Pull(dialog) => {
                    let dialog = dialog.clone();
                    let distance = match (dialog.ahead, dialog.behind) {
                        (Some(ahead), Some(behind)) => format!(
                            "当前分支与 {}/{} 相比：领先 {}，落后 {}",
                            dialog.remote, dialog.branch, ahead, behind
                        ),
                        _ if dialog.loading => "正在计算与目标的差异…".to_string(),
                        _ => "无法计算与目标的差异".to_string(),
                    };
                    let remote_value = dialog.remote.clone();
                    let branch_value = dialog.branch.clone();
                    let rebase = dialog.rebase;
                    let auto_stash = dialog.auto_stash;
                    let mut merge_chip = div()
                        .id("pull-merge")
                        .flex()
                        .items_center()
                        .h(px(24.))
                        .px(px(9.))
                        .rounded(px(4.))
                        .text_size(fs(11.5))
                        .cursor_pointer()
                        .child("合并");
                    merge_chip = if rebase {
                        merge_chip
                            .bg(theme::panel_alt())
                            .text_color(theme::dim())
                            .hover(|s| s.bg(theme::hover()))
                    } else {
                        merge_chip.bg(theme::accent_soft()).text_color(theme::accent())
                    };
                    let mut rebase_chip = div()
                        .id("pull-rebase")
                        .flex()
                        .items_center()
                        .h(px(24.))
                        .px(px(9.))
                        .rounded(px(4.))
                        .text_size(fs(11.5))
                        .cursor_pointer()
                        .child("变基");
                    rebase_chip = if rebase {
                        rebase_chip.bg(theme::accent_soft()).text_color(theme::accent())
                    } else {
                        rebase_chip
                            .bg(theme::panel_alt())
                            .text_color(theme::dim())
                            .hover(|s| s.bg(theme::hover()))
                    };
                    let check_color = if auto_stash {
                        theme::green()
                    } else {
                        theme::faint()
                    };
                    div()
                        .occlude()
                        .w(px(460.))
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .p(px(16.))
                        .rounded(px(8.))
                        .bg(theme::panel())
                        .border_1()
                        .border_color(theme::border())
                        .shadow_lg()
                        .child(
                            div()
                                .text_size(fs(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::text())
                                .child("从其他远端拉取"),
                        )
                        .child(
                            div()
                                .text_size(fs(11.))
                                .text_color(theme::dim())
                                .child("选一个远端分支，把它的提交合进当前分支。"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(
                                            div()
                                                .w(px(36.))
                                                .text_size(fs(11.))
                                                .text_color(theme::faint())
                                                .child("远端"),
                                        )
                                        .child(
                                            div()
                                                .id("pull-remote")
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .justify_between()
                                                .flex_1()
                                                .h(px(28.))
                                                .px(px(8.))
                                                .rounded(px(4.))
                                                .bg(theme::panel_alt())
                                                .border_1()
                                                .border_color(theme::border())
                                                .cursor_pointer()
                                                .hover(|s| s.bg(theme::hover()))
                                                .child(
                                                    div()
                                                        .text_size(fs(11.5))
                                                        .text_color(theme::text())
                                                        .child(remote_value.clone()),
                                                )
                                                .child(
                                                    Icon::new(IconName::ChevronsUpDown)
                                                        .xsmall()
                                                        .text_color(theme::faint()),
                                                )
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                                                        this.open_pull_remote_menu(ev, cx);
                                                    }),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(
                                            div()
                                                .w(px(36.))
                                                .text_size(fs(11.))
                                                .text_color(theme::faint())
                                                .child("分支"),
                                        )
                                        .child(
                                            div()
                                                .id("pull-branch")
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .justify_between()
                                                .flex_1()
                                                .h(px(28.))
                                                .px(px(8.))
                                                .rounded(px(4.))
                                                .bg(theme::panel_alt())
                                                .border_1()
                                                .border_color(theme::border())
                                                .cursor_pointer()
                                                .hover(|s| s.bg(theme::hover()))
                                                .child(
                                                    div()
                                                        .text_size(fs(11.5))
                                                        .text_color(theme::text())
                                                        .child(branch_value.clone()),
                                                )
                                                .child(
                                                    Icon::new(IconName::ChevronsUpDown)
                                                        .xsmall()
                                                        .text_color(theme::faint()),
                                                )
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                                                        this.open_pull_branch_menu(ev, cx);
                                                    }),
                                                ),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .text_size(fs(10.5))
                                .text_color(if dialog.loading {
                                    theme::faint()
                                } else {
                                    theme::dim()
                                })
                                .child(distance),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .w(px(36.))
                                        .text_size(fs(11.))
                                        .text_color(theme::faint())
                                        .child("方式"),
                                )
                                .child(merge_chip.on_click(cx.listener(|this, _, window, cx| {
                                    this.run_menu_action(
                                        MenuAction::PickPullStrategy(false),
                                        window,
                                        cx,
                                    );
                                })))
                                .child(rebase_chip.on_click(cx.listener(|this, _, window, cx| {
                                    this.run_menu_action(
                                        MenuAction::PickPullStrategy(true),
                                        window,
                                        cx,
                                    );
                                }))),
                        )
                        .child(
                            div()
                                .id("pull-autostash")
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.))
                                .h(px(24.))
                                .cursor_pointer()
                                .child(
                                    Icon::new(if auto_stash {
                                        IconName::Check
                                    } else {
                                        IconName::X
                                    })
                                    .xsmall()
                                    .text_color(check_color),
                                )
                                .child(
                                    div()
                                        .text_size(fs(11.))
                                        .text_color(theme::dim())
                                        .child("自动贮藏本地改动（拉取后恢复）"),
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(Modal::Pull(dialog)) = &mut this.modal {
                                        dialog.auto_stash = !dialog.auto_stash;
                                        this.settings.auto_stash = dialog.auto_stash;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_end()
                                .gap(px(8.))
                                .child(
                                    widgets::toolbar_button("pull-cancel", IconName::X, "取消")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.modal = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    widgets::primary_button(
                                        "pull-ok",
                                        IconName::ArrowDownToLine,
                                        "拉取",
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.pull_now(cx);
                                    })),
                                ),
                        )
                }
            };
            layers.push(scrim.child(card).into_any_element());
        }

        if let Some(layer) = menu_layer {
            layers.push(layer);
        }

        // 拖拽调整大小时，用一层全屏覆盖层接住鼠标移动，菜单本身不会被移出指针范围。
        if let Some(resize) = &self.menu_resize {
            let start_x = resize.start_x;
            let start_y = resize.start_y;
            let w0 = resize.w0;
            let h0 = resize.h0;
            layers.push(
                div()
                    .id("menu-resize-overlay")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .cursor(CursorStyle::ResizeUpLeftDownRight)
                    .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, window, cx| {
                        let dx = f32::from(ev.position.x) - start_x;
                        let dy = f32::from(ev.position.y) - start_y;
                        let bounds = window.bounds();
                        if let Some(menu) = &mut this.menu {
                            let max_w = (f32::from(bounds.size.width) - menu.x - 8.)
                                .clamp(MENU_MIN_W, MENU_MAX_W);
                            let max_h = (f32::from(bounds.size.height) - menu.y - 8.)
                                .clamp(MENU_MIN_H, MENU_MAX_H);
                            menu.w = (w0 + dx).clamp(MENU_MIN_W, max_w);
                            menu.h = (h0 + dy).clamp(MENU_MIN_H, max_h);
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.menu_resize = None;
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
            );
        }

        if let Some(toast) = &self.toast {
            let color = match toast.tone {
                Tone::Info => theme::blue(),
                Tone::Success => theme::green(),
                Tone::Error => theme::red(),
            };
            let text = toast.text.clone();
            let layer = div()
                .absolute()
                .bottom(px(34.))
                .left_0()
                .w_full()
                .flex()
                .justify_center()
                .child(
                    div()
                        .occlude()
                        .max_w(px(640.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.))
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(6.))
                        .bg(theme::panel())
                        .border_1()
                        .border_color(color)
                        .shadow_lg()
                        .text_size(fs(11.5))
                        .text_color(theme::text())
                        .child(text),
                );
            layers.push(layer.into_any_element());
        }

        let _ = window;
        let _ = cx;
        layers
    }
}

#[cfg(target_os = "windows")]
fn window_button(id: &'static str, icon: IconName, danger: bool) -> Stateful<Div> {
    let mut button = div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(40.))
        .h(px(32.))
        .rounded(px(4.))
        .cursor_pointer();
    if danger {
        button = button.hover(|s| s.bg(theme::red()));
    } else {
        button = button.hover(|s| s.bg(theme::hover()));
    }
    button.child(Icon::new(icon).xsmall().text_color(theme::dim()))
}

impl Render for GitApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.clear_message_next {
            self.clear_message_next = false;
            self.message_text.clear();
            self.message.update(cx, |state, cx| state.set_value("", window, cx));
        }

        let main: AnyElement = if self.repo.is_some() {
            match self.tab {
                Tab::Commit => panels::commit::render(self, window, cx).into_any_element(),
                Tab::Compare => panels::compare::render(self, window, cx).into_any_element(),
                Tab::Log => panels::log::render(self, window, cx).into_any_element(),
                Tab::Branches => panels::branches::render(self, window, cx).into_any_element(),
                Tab::Stash => panels::stash::render(self, window, cx).into_any_element(),
                Tab::Console => panels::console::render(self, window, cx).into_any_element(),
            }
        } else {
            panels::welcome::render(self, window, cx).into_any_element()
        };

        let overlays = self.overlay_layers(window, cx);

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .text_color(theme::text())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if this.on_key(ev, window, cx) {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    this.dismiss_menu_outside(ev.position, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    this.dismiss_menu_outside(ev.position, cx);
                }),
            )
            .child(self.title_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.rail(cx))
                    .child(div().flex().flex_1().min_w(px(0.)).child(main)),
            )
            .child(self.status_bar(cx))
            .children(overlays)
    }
}

pub fn expand_path(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(value)
}

/// 仓库名或路径的短显示。
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn fmt_commit_time(commit: &Commit) -> String {
    format!("{} · {}", commit.author, fmt_datetime(commit.time))
}

pub fn fmt_change_time(ts: i64) -> String {
    fmt_time(ts)
}

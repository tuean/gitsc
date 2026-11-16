//! 提交页：提交信息 + 变更列表（已暂存 / 未暂存）+ 差异视图。

use gpui_kit::component::input::Textarea;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;
use gitsc_core::status::{Change, Status};

use crate::app::{GitApp, RepoState};
use crate::panels::diff;
use crate::theme::{self, fs};
use crate::widgets::{self, split_path};

pub fn render(app: &GitApp, _window: &mut Window, cx: &mut Context<GitApp>) -> Div {
    let Some(repo) = app.repo.as_ref() else {
        return div();
    };
    let status = repo.status.clone().unwrap_or_else(Status::default);
    let staged: Vec<Change> = status.staged().cloned().collect();
    let unstaged: Vec<Change> = status.unstaged().cloned().collect();

    let selected = repo.selected_change.clone();
    let diff_loading = repo.diff_loading;
    let status_loading = repo.status_loading;

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .w_full()
        .child(editor(app, cx))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .child(changes(app, &staged, &unstaged, selected.as_ref(), status_loading, cx))
                .child(diff_area(repo, &selected, &diff_loading, cx)),
        )
}

fn editor(app: &GitApp, cx: &mut Context<GitApp>) -> Div {
    let can_commit = app.can_commit();
    let amend = app.amend;

    let amend_chip = div()
        .id("amend-toggle")
        .flex()
        .flex_row()
        .items_center()
        .gap(px(5.))
        .px(px(7.))
        .h(px(20.))
        .rounded(px(4.))
        .cursor_pointer()
        .text_size(fs(10.5))
        .text_color(if amend { theme::accent() } else { theme::dim() })
        .bg(if amend { theme::accent_soft() } else { theme::panel_alt() })
        .child(Icon::new(IconName::PenLine).xsmall())
        .child("修补上次提交")
        .on_click(cx.listener(|this, _, _, cx| {
            this.amend = !this.amend;
            cx.notify();
        }));

    let mut commit_btn = widgets::primary_button("commit-do", IconName::Check, "提交");
    if can_commit {
        commit_btn = commit_btn.on_click(cx.listener(|this, _, _, cx| this.commit(cx)));
    } else {
        commit_btn = commit_btn.opacity(0.45);
    }

    div()
        .flex()
        .flex_col()
        .flex_none()
        .bg(theme::panel())
        .border_b(px(1.))
        .border_color(theme::border())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .px(px(10.))
                .h(px(32.))
                .gap(px(8.))
                .child(Icon::new(IconName::GitCommitHorizontal).xsmall().text_color(theme::dim()))
                .child(
                    div()
                        .text_size(fs(11.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("提交信息"),
                )
                .child(
                    div()
                        .text_size(fs(10.))
                        .text_color(theme::faint())
                        .child("⌘↩ 提交"),
                )
                .child(div().flex_1())
                .child(amend_chip)
                .child(commit_btn),
        )
        .child(
            div()
                .px(px(10.))
                .pb(px(8.))
                .child(
                    div()
                        .w_full()
                        .rounded(px(5.))
                        .border_1()
                        .border_color(theme::border())
                        .bg(theme::editor())
                        .px(px(8.))
                        .py(px(6.))
                        .child(
                            Textarea::new(&app.message)
                                .h(px(74.))
                                .w_full()
                                .text_size(fs(11.5))
                                .font_family(theme::mono())
                                .appearance(false)
                                .bordered(false),
                        ),
                ),
        )
}

fn changes(
    app: &GitApp,
    staged: &[Change],
    unstaged: &[Change],
    selected: Option<&(bool, String)>,
    loading: bool,
    cx: &mut Context<GitApp>,
) -> Div {
    let total = staged.len() + unstaged.len();
    let mut list = div()
        .id("changes-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll();

    if loading && total == 0 {
        list = list.child(
            div()
                .p(px(12.))
                .text_size(fs(11.))
                .text_color(theme::faint())
                .child("读取状态…"),
        );
    } else if total == 0 {
        list = list
            .child(widgets::empty_state(IconName::Check, "工作区干净", "没有需要提交的变更"));
    } else {
        if !staged.is_empty() {
            list = list.child(widgets::section_header("已暂存", Some(staged.len())));
            for change in staged {
                list = list.child(change_row(app, change, true, selected, cx));
            }
        }
        if !unstaged.is_empty() {
            list = list.child(widgets::section_header("变更", Some(unstaged.len())));
            for change in unstaged {
                list = list.child(change_row(app, change, false, selected, cx));
            }
        }
    }

    let staged_count = staged.len();
    let all_staged = !unstaged.is_empty();

    let mut header = div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(30.))
        .px(px(8.))
        .gap(px(6.))
        .border_b(px(1.))
        .border_color(theme::border())
        .child(Icon::new(IconName::FolderGit2).xsmall().text_color(theme::dim()))
        .child(
            div()
                .text_size(fs(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("变更"),
        )
        .child(
            div()
                .text_size(fs(10.))
                .text_color(theme::faint())
                .child(format!("{} 个文件 · {} 已暂存", total, staged_count)),
        )
        .child(div().flex_1());

    if all_staged {
        header = header.child(
            div()
                .id("stage-all")
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.))
                .px(px(6.))
                .h(px(20.))
                .rounded(px(4.))
                .cursor_pointer()
                .text_size(fs(10.5))
                .text_color(theme::dim())
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .child(Icon::new(IconName::Plus).xsmall())
                .child("全部暂存")
                .on_click(cx.listener(|this, _, _, cx| this.stage_all(cx))),
        );
    }
    if staged_count > 0 {
        header = header.child(
            div()
                .id("unstage-all")
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.))
                .px(px(6.))
                .h(px(20.))
                .rounded(px(4.))
                .cursor_pointer()
                .text_size(fs(10.5))
                .text_color(theme::dim())
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .child(Icon::new(IconName::Minus).xsmall())
                .child("全部取消")
                .on_click(cx.listener(|this, _, _, cx| this.unstage_all(cx))),
        );
    }

    div()
        .flex()
        .flex_col()
        .w(px(330.))
        .flex_none()
        .min_h(px(0.))
        .bg(theme::panel())
        .border_r(px(1.))
        .border_color(theme::border())
        .child(header)
        .child(list)
}

fn change_row(
    _app: &GitApp,
    change: &Change,
    staged: bool,
    selected: Option<&(bool, String)>,
    cx: &mut Context<GitApp>,
) -> Stateful<Div> {
    let path = change.path.clone();
    let (letter, color) = if change.conflicted {
        ('U', theme::red())
    } else if change.untracked {
        ('?', theme::green())
    } else if staged {
        widgets::code_letter(change.index)
    } else {
        widgets::code_letter(change.worktree)
    };
    let is_selected = selected
        .map(|(s, p)| *s == staged && p == &change.path)
        .unwrap_or(false);
    let (dir, name) = split_path(&change.path);

    let checkbox = {
        let mark: AnyElement = if staged {
            Icon::new(IconName::Check)
                .with_size(px(10.))
                .text_color(theme::text())
                .into_any_element()
        } else {
            div().into_any_element()
        };
        div()
            .id(format!("chk-{}-{}", staged, path))
            .flex()
            .items_center()
            .justify_center()
            .w(px(13.))
            .h(px(13.))
            .rounded(px(3.))
            .border_1()
            .border_color(if staged { theme::accent() } else { theme::faint() })
            .bg(if staged { theme::accent_soft() } else { theme::editor() })
            .cursor_pointer()
            .child(mark)
            .on_click(cx.listener({
                let path = path.clone();
                move |this, _, _, cx| {
                    if staged {
                        this.unstage(path.clone(), cx);
                    } else {
                        this.stage(path.clone(), cx);
                    }
                }
            }))
    };

    let mut row = div()
        .id(format!("chg-{}-{}", staged, path))
        .flex()
        .flex_row()
        .items_center()
        .h(px(crate::app::ROW_H))
        .px(px(8.))
        .gap(px(6.))
        .cursor_pointer()
        .bg(if is_selected { theme::selection() } else { theme::panel() })
        .hover(|s| s.bg(theme::hover()))
        .on_click(cx.listener({
            let path = path.clone();
            move |this, _, _, cx| this.select_change(staged, path.clone(), cx)
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener({
                let path = path.clone();
                move |this, ev: &MouseDownEvent, _, cx| this.open_change_menu(staged, path.clone(), ev, cx)
            }),
        )
        .child(checkbox)
        .child(widgets::badge(letter, color))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .child(
                    div()
                        .text_size(fs(11.5))
                        .text_color(theme::text())
                        .truncate()
                        .child(name),
                )
                .child(
                    div()
                        .ml(px(6.))
                        .flex_none()
                        .text_size(fs(10.))
                        .text_color(theme::faint())
                        .truncate()
                        .child(dir),
                ),
        );

    if !staged {
        row = row.child(
            div()
                .id(format!("discard-{}", path))
                .flex()
                .items_center()
                .justify_center()
                .w(px(18.))
                .h(px(18.))
                .rounded(px(3.))
                .cursor_pointer()
                .text_color(theme::faint())
                .hover(|s| s.bg(theme::hover_soft()).text_color(theme::red()))
                .child(Icon::new(IconName::Undo2).with_size(px(12.)))
                .on_click(cx.listener({
                    let path = path.clone();
                    move |this, _, _, cx| {
                        this.ask_discard(vec![path.clone()], cx);
                    }
                })),
        );
    }

    if change.untracked {
        row = row.child(widgets::pill("未跟踪", theme::green(), theme::hover_soft()));
    }

    row
}

fn diff_area(
    repo: &RepoState,
    selected: &Option<(bool, String)>,
    loading: &bool,
    cx: &mut Context<GitApp>,
) -> Div {
    let mut pane = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .min_h(px(0.))
        .bg(theme::editor());

    let header: Div = if let Some(pane_data) = repo.diff.as_ref() {
        let (staged, _) = selected.clone().unwrap_or((false, String::new()));
        let mut actions = div().flex().flex_row().items_center().gap(px(4.));
        if staged {
            actions = actions.child(
                div()
                    .id("diff-unstage")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(6.))
                    .h(px(20.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_size(fs(10.5))
                    .text_color(theme::dim())
                    .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                    .child(Icon::new(IconName::Minus).xsmall())
                    .child("取消暂存")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(path) = pane_path(this) {
                            this.unstage(path, cx);
                        }
                    })),
            );
        } else {
            actions = actions.child(
                div()
                    .id("diff-stage")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(6.))
                    .h(px(20.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_size(fs(10.5))
                    .text_color(theme::dim())
                    .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                    .child(Icon::new(IconName::Plus).xsmall())
                    .child("暂存")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(path) = pane_path(this) {
                            this.stage(path, cx);
                        }
                    })),
            );
            actions = actions.child(
                div()
                    .id("diff-discard")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(6.))
                    .h(px(20.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_size(fs(10.5))
                    .text_color(theme::dim())
                    .hover(|s| s.bg(theme::hover()).text_color(theme::red()))
                    .child(Icon::new(IconName::Undo2).xsmall())
                    .child("丢弃")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(path) = pane_path(this) {
                            this.ask_discard(vec![path], cx);
                        }
                    })),
            );
        }

        div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(30.))
            .px(px(8.))
            .gap(px(8.))
            .flex_none()
            .bg(theme::panel())
            .border_b(px(1.))
            .border_color(theme::border())
            .child(
                div()
                    .text_size(fs(11.))
                    .font_family(theme::mono())
                    .text_color(theme::text())
                    .truncate()
                    .child(pane_data.title.clone()),
            )
            .child(
                div()
                    .text_size(fs(10.))
                    .text_color(theme::faint())
                    .child(format!("+{} −{}", pane_data.added, pane_data.removed)),
            )
            .child(div().flex_1())
            .child(actions)
    } else {
        div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(30.))
            .flex_none()
            .bg(theme::panel())
            .border_b(px(1.))
            .border_color(theme::border())
            .child(
                div()
                    .px(px(8.))
                    .text_size(fs(11.))
                    .text_color(theme::faint())
                    .child(if *loading { "读取差异…" } else { "差异" }),
            )
    };

    pane = pane.child(header);

    if let Some(pane_data) = repo.diff.as_ref() {
        pane = pane.child(diff::body(pane_data));
    } else if *loading {
        pane = pane.child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_size(fs(11.))
                .text_color(theme::faint())
                .child("读取差异…"),
        );
    } else {
        pane = pane.child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_size(fs(11.))
                .text_color(theme::faint())
                .child("选择左侧文件查看差异"),
        );
    }
    pane
}

fn pane_path(app: &GitApp) -> Option<String> {
    app.repo
        .as_ref()
        .and_then(|r| r.selected_change.as_ref().map(|(_, p)| p.clone()))
}

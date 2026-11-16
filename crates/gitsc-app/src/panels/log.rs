//! 日志页：左侧提交图（泳道）+ 右侧提交详情。

use gpui_kit::component::input::Input;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;
use gitsc_core::graph::GraphRow;
use gitsc_core::log::{Commit, RefKind};

use crate::app::{GitApp, RepoState, ROW_H};
use crate::panels::diff;
use crate::theme::{self, fs};
use crate::widgets::{self, split_path};

const LANE_W: f32 = 14.0;

pub fn render(app: &GitApp, _window: &mut Window, cx: &mut Context<GitApp>) -> Div {
    let Some(repo) = app.repo.as_ref() else {
        return div();
    };

    div()
        .flex()
        .flex_row()
        .flex_1()
        .min_h(px(0.))
        .w_full()
        .child(history(app, repo, cx))
        .child(detail(app, repo, cx))
}

fn history(app: &GitApp, repo: &RepoState, cx: &mut Context<GitApp>) -> Div {
    let rows = &repo.view;
    let graph = &repo.graph;
    let lanes = repo.graph_lanes.max(1);
    let selected = repo.selected_commit;
    let all = app.graph_all;

    let mut list = div()
        .id("log-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll();

    if rows.is_empty() {
        list = list.child(widgets::empty_state(
            IconName::GitCommitHorizontal,
            "还没有提交",
            "仓库里还没有任何提交，先在提交页创建第一个。",
        ));
    } else {
        for (index, commit) in rows.iter().enumerate() {
            let graph_row = graph.get(index);
            list = list.child(commit_row(
                commit,
                graph_row,
                lanes,
                index == selected.unwrap_or(usize::MAX),
                cx,
            ));
        }
    }

    let mut all_toggle = div()
        .id("graph-all")
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.))
        .px(px(7.))
        .h(px(21.))
        .rounded(px(4.))
        .cursor_pointer()
        .text_size(fs(10.5))
        .text_color(if all { theme::accent() } else { theme::dim() })
        .bg(if all { theme::accent_soft() } else { theme::panel_alt() })
        .child(Icon::new(IconName::GitBranch).xsmall())
        .child("全部分支");
    all_toggle = all_toggle.on_click(cx.listener(|this, _, _, cx| {
        this.graph_all = !this.graph_all;
        this.rebuild_log();
        cx.notify();
    }));

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(34.))
        .px(px(8.))
        .gap(px(6.))
        .border_b(px(1.))
        .border_color(theme::border())
        .child(Icon::new(IconName::GitGraph).xsmall().text_color(theme::dim()))
        .child(
            div()
                .text_size(fs(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("提交历史"),
        )
        .child(
            div()
                .text_size(fs(10.))
                .text_color(theme::faint())
                .child(format!("{} 个提交", rows.len())),
        )
        .child(div().flex_1())
        .child(all_toggle)
        .child(
            div()
                .w(px(190.))
                .child(Input::new(&app.filter).small().cleanable(true)),
        );

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .min_h(px(0.))
        .border_r(px(1.))
        .border_color(theme::border())
        .child(header)
        .child(list)
}

fn commit_row(
    commit: &Commit,
    graph: Option<&GraphRow>,
    lanes: usize,
    selected: bool,
    cx: &mut Context<GitApp>,
) -> Stateful<Div> {
    let oid_for_click = commit.oid.clone();
    let commit_for_menu = commit.clone();
    let mut row = div()
        .id(format!("commit-{}", commit.oid))
        .flex()
        .flex_row()
        .items_center()
        .h(px(ROW_H))
        .w_full()
        .text_size(fs(11.5))
        .bg(if selected { theme::selection() } else { theme::bg() })
        .hover(|s| s.bg(theme::hover_soft()))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_commit_by_oid(oid_for_click.clone(), cx)
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                this.open_commit_menu(commit_for_menu.clone(), ev, cx)
            }),
        );

    row = row.child(graph_cell(graph, lanes));

    let mut refs = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.))
        .flex_none()
        .max_w(px(230.))
        .overflow_hidden();
    for reference in commit.refs.iter().take(3) {
        let (fg, name) = match reference.kind {
            RefKind::Head => (theme::green(), reference.name.clone()),
            RefKind::Local => (theme::accent(), reference.name.clone()),
            RefKind::Remote => (theme::violet(), reference.name.clone()),
            RefKind::Tag => (theme::amber(), reference.name.clone()),
        };
        refs = refs.child(widgets::ref_pill(name, fg));
    }
    row = row.child(refs);

    row = row.child(
        div()
            .flex_1()
            .min_w(px(0.))
            .overflow_hidden()
            .px(px(6.))
            .child(if commit.is_merge() {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(5.))
                    .child(Icon::new(IconName::GitMerge).xsmall().text_color(theme::violet()))
                    .child(
                        div()
                            .truncate()
                            .text_color(theme::text())
                            .child(commit.summary.clone()),
                    )
                    .into_any_element()
            } else {
                div()
                    .truncate()
                    .text_color(theme::text())
                    .child(commit.summary.clone())
                    .into_any_element()
            }),
    )
    .child(
        div()
            .w(px(96.))
            .flex_none()
            .truncate()
            .text_size(fs(10.))
            .text_color(theme::dim())
            .child(commit.author.clone()),
    )
    .child(
        div()
            .w(px(72.))
            .flex_none()
            .text_right()
            .pr(px(6.))
            .text_size(fs(10.))
            .text_color(theme::faint())
            .child(widgets::fmt_time(commit.time)),
    )
    .child(
        div()
            .w(px(58.))
            .flex_none()
            .text_size(fs(10.))
            .font_family(theme::mono())
            .text_color(theme::faint())
            .child(commit.short().to_string()),
    );

    row
}

/// 一行里的泳道图：贯穿线、进入线、节点、分叉线。
fn graph_cell(graph: Option<&GraphRow>, lanes: usize) -> Div {
    let width = px(lanes as f32 * LANE_W + 10.);
    let mut cell = div().relative().flex_none().w(width).h(px(ROW_H));

    let Some(row) = graph else {
        return cell;
    };

    for (lane, color) in &row.pass {
        cell = cell.child(
            div()
                .absolute()
                .left(px(*lane as f32 * LANE_W + 5.))
                .top(px(0.))
                .w(px(2.))
                .h(px(ROW_H))
                .rounded(px(1.))
                .bg(theme::lane(*color)),
        );
    }

    if row.from_top && !row.pass.iter().any(|(l, _)| *l == row.lane) {
        cell = cell.child(
            div()
                .absolute()
                .left(px(row.lane as f32 * LANE_W + 5.))
                .top(px(0.))
                .w(px(2.))
                .h(px(13.))
                .bg(theme::lane(row.color)),
        );
    }

    for edge in &row.edges {
        if edge.to != row.lane {
            let (from, to) = if edge.from < edge.to {
                (edge.from, edge.to)
            } else {
                (edge.to, edge.from)
            };
            cell = cell.child(
                div()
                    .absolute()
                    .left(px(from as f32 * LANE_W + 5.))
                    .top(px(12.))
                    .w(px((to - from) as f32 * LANE_W))
                    .h(px(2.))
                    .bg(theme::lane(edge.color)),
            );
        }
        cell = cell.child(
            div()
                .absolute()
                .left(px(edge.to as f32 * LANE_W + 5.))
                .top(px(12.))
                .w(px(2.))
                .h(px(14.))
                .bg(theme::lane(edge.color)),
        );
    }

    if row.edges.is_empty() && !row.pass.iter().any(|(l, _)| *l == row.lane) {
        cell = cell.child(
            div()
                .absolute()
                .left(px(row.lane as f32 * LANE_W + 5.))
                .top(px(13.))
                .w(px(2.))
                .h(px(13.))
                .bg(theme::lane(row.color)),
        );
    }

    cell.child(
        div()
            .absolute()
            .left(px(row.lane as f32 * LANE_W + 1.))
            .top(px(8.))
            .w(px(10.))
            .h(px(10.))
            .rounded(px(5.))
            .bg(theme::lane(row.color))
            .border_1()
            .border_color(theme::bg()),
    )
}

fn detail(app: &GitApp, repo: &RepoState, cx: &mut Context<GitApp>) -> Div {
    let Some(commit) = repo.selected_commit() else {
        return div()
            .w(px(430.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::panel())
            .child(
                div()
                    .text_size(fs(11.))
                    .text_color(theme::faint())
                    .child("选择提交查看详情"),
            );
    };

    let mut head = div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(34.))
        .px(px(10.))
        .gap(px(6.))
        .border_b(px(1.))
        .border_color(theme::border())
        .child(Icon::new(IconName::GitCommitHorizontal).xsmall().text_color(theme::dim()))
        .child(
            div()
                .text_size(fs(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("提交详情"),
        )
        .child(
            div()
                .id("copy-commit-hash")
                .ml(px(4.))
                .flex_none()
                .cursor_pointer()
                .text_size(fs(10.5))
                .font_family(theme::mono())
                .text_color(theme::accent())
                .child(commit.short().to_string())
                .on_click(cx.listener({
                    let oid = commit.oid.clone();
                    move |this, _, _, cx| this.copy_commit_hash(oid.clone(), cx)
                })),
        )
        .child(div().flex_1());

    if repo.detail.as_ref().map(|d| d.loading).unwrap_or(false) {
        head = head.child(
            div()
                .text_size(fs(10.))
                .text_color(theme::faint())
                .child("读取中…"),
        );
    }

    let mut body = div()
        .id("detail-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .p(px(10.))
        .gap(px(8.))
        .child(
            div()
                .text_size(fs(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(commit.summary.clone()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .text_size(fs(10.5))
                .text_color(theme::dim())
                .child(format!("{} <{}>", commit.author, commit.email))
                .child(widgets::fmt_datetime(commit.time))
                .child(format!(
                    "{} 个父提交{}",
                    commit.parents.len(),
                    if commit.parents.is_empty() { "（根提交）" } else { "" }
                )),
        );

    if let Some(detail) = repo.detail.as_ref() {
        if let Some(error) = &detail.error {
            body = body.child(
                div()
                    .text_size(fs(10.5))
                    .text_color(theme::red())
                    .child(error.clone()),
            );
        }
        let message = detail.body.trim();
        if !message.is_empty() && message != commit.summary {
            body = body.child(
                div()
                    .rounded(px(5.))
                    .bg(theme::editor())
                    .p(px(8.))
                    .text_size(fs(11.))
                    .font_family(theme::mono())
                    .text_color(theme::dim())
                    .child(message.to_string()),
            );
        }

        if let Some(pane) = detail.file_diff.as_ref() {
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .id("file-back")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(3.))
                            .px(px(6.))
                            .h(px(20.))
                            .rounded(px(4.))
                            .cursor_pointer()
                            .bg(theme::panel_alt())
                            .text_size(fs(10.5))
                            .text_color(theme::dim())
                            .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                            .child(Icon::new(IconName::ArrowLeft).xsmall())
                            .child("返回提交")
                            .on_click(cx.listener(|this, _, _, cx| this.close_commit_file())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_size(fs(10.5))
                            .font_family(theme::mono())
                            .text_color(theme::text())
                            .child(pane.title.clone()),
                    ),
            );
            body = body.child(diff::body(pane));
        } else {
            if !detail.files.is_empty() {
                body = body.child(widgets::section_header("变更文件", Some(detail.files.len())));
                let mut files = div().flex().flex_col();
                for (path, added, removed) in &detail.files {
                    let (dir, name) = split_path(path);
                    let added = added.unwrap_or(0);
                    let removed = removed.unwrap_or(0);
                    let (letter, color) = if added == 0 && removed == 0 {
                        ('B', theme::dim())
                    } else if added > 0 && removed == 0 {
                        ('A', theme::green())
                    } else if added == 0 && removed > 0 {
                        ('D', theme::red())
                    } else {
                        ('M', theme::amber())
                    };
                    files = files.child(
                        div()
                            .id(format!("file-{path}"))
                            .flex()
                            .flex_row()
                            .items_center()
                            .h(px(22.))
                            .px(px(4.))
                            .gap(px(6.))
                            .rounded(px(3.))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover_soft()))
                            .on_click(cx.listener({
                                let path = path.clone();
                                move |this, _, _, cx| this.open_commit_file(path.clone(), cx)
                            }))
                            .child(widgets::badge(letter, color))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(fs(10.5))
                                            .text_color(theme::text())
                                            .truncate()
                                            .child(name),
                                    )
                                    .child(
                                        div()
                                            .ml(px(5.))
                                            .flex_none()
                                            .text_size(fs(9.5))
                                            .text_color(theme::faint())
                                            .child(dir),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(fs(9.5))
                                    .font_family(theme::mono())
                                    .text_color(theme::green())
                                    .child(format!("+{added}")),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(fs(9.5))
                                    .font_family(theme::mono())
                                    .text_color(theme::red())
                                    .child(format!("−{removed}")),
                            ),
                    );
                }
                body = body.child(files);
            }
        }
    }

    let oid = commit.oid.clone();
    let actions = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(4.))
        .px(px(8.))
        .py(px(6.))
        .border_t(px(1.))
        .border_color(theme::border())
        .child(
            widgets::toolbar_button("det-checkout", IconName::GitBranch, "检出").on_click(
                cx.listener({
                    let oid = oid.clone();
                    move |this, _, _, cx| this.run(crate::worker::Action::CheckoutDetach(oid.clone()), cx)
                }),
            ),
        )
        .child(
            widgets::toolbar_button("det-branch", IconName::Plus, "新建分支").on_click(
                cx.listener({
                    let oid = oid.clone();
                    move |this, _, window, cx| {
                        let target = oid.clone();
                        this.open_prompt(
                            crate::app::PromptKind::NewBranch { start: target.clone() },
                            "新建分支",
                            "输入分支名",
                            "从当前提交开始",
                            "分支名",
                            "",
                            window,
                            cx,
                        )
                    }
                }),
            ),
        )
        .child(
            widgets::toolbar_button("det-cherry", IconName::GitMerge, "拣选").on_click(
                cx.listener({
                    let oid = oid.clone();
                    move |this, _, _, cx| this.run(crate::worker::Action::CherryPick(oid.clone()), cx)
                }),
            ),
        )
        .child(
            widgets::toolbar_button("det-revert", IconName::Undo2, "还原").on_click(
                cx.listener({
                    let oid = oid.clone();
                    move |this, _, _, cx| this.run(crate::worker::Action::Revert(oid.clone()), cx)
                }),
            ),
        );

    div()
        .w(px(430.))
        .flex_none()
        .flex()
        .flex_col()
        .min_h(px(0.))
        .bg(theme::panel())
        .child(head)
        .child(body)
        .child(actions)
}

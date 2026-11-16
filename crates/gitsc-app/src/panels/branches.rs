//! 分支页：本地 / 远端分支与标签。

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;
use gitsc_core::tags::{Branch, Tag};

use crate::app::{ConfirmKind, GitApp};
use crate::theme::{self, fs};
use crate::widgets::{self, fmt_time};

pub fn render(app: &GitApp, _window: &mut Window, cx: &mut Context<GitApp>) -> Div {
    let Some(repo) = app.repo.as_ref() else {
        return div();
    };
    let local: Vec<&Branch> = repo.branches.iter().filter(|b| !b.is_remote).collect();
    let remote: Vec<&Branch> = repo.branches.iter().filter(|b| b.is_remote).collect();
    let tags = &repo.tags;
    let selected = repo.selected_branch.clone();

    let mut list = div()
        .id("branches-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll();

    if local.is_empty() && remote.is_empty() && tags.is_empty() {
        list = list.child(widgets::empty_state(
            IconName::GitBranch,
            "没有分支",
            "这个仓库还没有任何分支。",
        ));
    }

    if !local.is_empty() {
        list = list.child(widgets::section_header("本地分支", Some(local.len())));
        for branch in &local {
            list = list.child(branch_row(branch, selected.as_deref(), cx));
        }
    }
    if !remote.is_empty() {
        list = list.child(widgets::section_header("远端分支", Some(remote.len())));
        for branch in &remote {
            list = list.child(branch_row(branch, selected.as_deref(), cx));
        }
    }
    if !tags.is_empty() {
        list = list.child(widgets::section_header("标签", Some(tags.len())));
        for tag in tags {
            list = list.child(tag_row(tag, selected.as_deref(), cx));
        }
    }

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(34.))
        .px(px(8.))
        .gap(px(6.))
        .border_b(px(1.))
        .border_color(theme::border())
        .child(Icon::new(IconName::GitBranch).xsmall().text_color(theme::dim()))
        .child(
            div()
                .text_size(fs(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("分支"),
        )
        .child(
            div()
                .text_size(fs(10.))
                .text_color(theme::faint())
                .child(format!("{} 本地 · {} 远端 · {} 标签", local.len(), remote.len(), tags.len())),
        )
        .child(div().flex_1())
        .child(
            widgets::toolbar_button("branch-new", IconName::Plus, "新建分支").on_click(
                cx.listener(|this, _, window, cx| {
                    let start = this
                        .repo
                        .as_ref()
                        .map(|r| r.branch())
                        .unwrap_or_else(|| "HEAD".to_string());
                    this.open_branch_prompt(start, window, cx);
                }),
            ),
        );

    let current = repo.branch();
    let selected_branch = selected.clone();
    let sb_checkout = selected_branch.clone();
    let sb_merge = selected_branch.clone();
    let sb_rebase = selected_branch.clone();
    let sb_rename = selected_branch.clone();
    let sb_delete = selected_branch.clone();
    let mut actions = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.))
        .px(px(8.))
        .py(px(6.))
        .border_t(px(1.))
        .border_color(theme::border())
        .child(
            widgets::toolbar_button("br-checkout", IconName::Check, "检出").on_click(cx.listener(
                move |this, _, _, cx| {
                    if let Some(name) = sb_checkout.clone() {
                        if !name.starts_with("tag:") && name != "HEAD" {
                            this.run(
                                crate::worker::Action::Checkout {
                                    name,
                                    auto_stash: this.settings.auto_stash,
                                },
                                cx,
                            );
                        }
                    }
                },
            )),
        )
        .child(
            widgets::toolbar_button("br-merge", IconName::GitMerge, "合并到当前").on_click(
                cx.listener(move |this, _, _, cx| {
                    if let Some(name) = sb_merge.clone() {
                        this.run(crate::worker::Action::Merge(name), cx);
                    }
                }),
            ),
        )
        .child(
            widgets::toolbar_button("br-rebase", IconName::GitCommitHorizontal, "变基").on_click(
                cx.listener(move |this, _, _, cx| {
                    if let Some(name) = sb_rebase.clone() {
                        this.run(crate::worker::Action::Rebase(name), cx);
                    }
                }),
            ),
        )
        .child(widgets::vdivider())
        .child(
            widgets::toolbar_button("br-rename", IconName::PenLine, "重命名").on_click(cx.listener(
                move |this, _, window, cx| {
                    if let Some(name) = sb_rename.clone() {
                        this.open_prompt(
                            crate::app::PromptKind::RenameBranch { old: name.clone() },
                            "重命名分支",
                            "输入新的分支名",
                            "本地分支改名，不会影响远端",
                            "分支名",
                            &name,
                            window,
                            cx,
                        );
                    }
                },
            )),
        )
        .child(
            widgets::toolbar_button("br-delete", IconName::Trash, "删除").on_click(cx.listener(
                move |this, _, _, cx| {
                    if let Some(name) = sb_delete.clone() {
                        if name == "HEAD" || name == current || name.starts_with("tag:") {
                            this.set_toast(
                                "不能删除当前分支或标签",
                                crate::app::Tone::Error,
                                cx,
                            );
                            return;
                        }
                        this.open_confirm(
                            ConfirmKind::DeleteBranch(name.clone()),
                            "删除分支",
                            format!("确定要删除分支 {name} 吗？未合并的提交会丢失（等价 git branch -D）。"),
                            "删除",
                            true,
                        );
                    }
                },
            )),
        )
        .child(div().flex_1())
        .child(
            widgets::toolbar_button("tag-new", IconName::Tag, "新建标签").on_click(cx.listener(
                move |this, _, window, cx| {
                    let target = this
                        .repo
                        .as_ref()
                        .and_then(|r| r.selected_commit_oid())
                        .unwrap_or_else(|| "HEAD".to_string());
                    this.open_prompt(
                        crate::app::PromptKind::NewTag { target },
                        "新建标签",
                        "标签名",
                        "轻量标签，例如 v0.1.0",
                        "标签名",
                        "",
                        window,
                        cx,
                    )
                },
            )),
        );

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .w_full()
        .child(header)
        .child(list)
        .child(actions)
}

fn branch_row(branch: &Branch, selected: Option<&str>, cx: &mut Context<GitApp>) -> Stateful<Div> {
    let is_selected = selected == Some(branch.name.as_str());
    let name = branch.name.clone();
    let remote = branch.is_remote;

    let mut row = div()
        .id(format!("br-{}", branch.name))
        .flex()
        .flex_row()
        .items_center()
        .h(px(26.))
        .px(px(10.))
        .gap(px(6.))
        .cursor_pointer()
        .bg(if is_selected { theme::selection() } else { theme::panel() })
        .hover(|s| s.bg(theme::hover_soft()))
        .on_click(cx.listener({
            let name = name.clone();
            move |this, _, _, cx| this.select_branch(name.clone(), cx)
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener({
                let branch = branch.clone();
                move |this, ev: &MouseDownEvent, _, cx| this.open_branch_menu(branch.clone(), ev, cx)
            }),
        )
        .child(
            div()
                .w(px(12.))
                .flex_none()
                .child(if branch.is_current {
                    Icon::new(IconName::Check).with_size(px(11.)).text_color(theme::green()).into_any_element()
                } else {
                    div().into_any_element()
                }),
        )
        .child(
            div()
                .flex_none()
                .max_w(px(260.))
                .truncate()
                .text_size(fs(11.5))
                .font_weight(if branch.is_current {
                    gpui::FontWeight::SEMIBOLD
                } else {
                    gpui::FontWeight::NORMAL
                })
                .text_color(if remote { theme::violet() } else { theme::text() })
                .child(branch.name.clone()),
        );

    if let Some(upstream) = branch.upstream.as_ref() {
        if !upstream.is_empty() {
            row = row.child(
                div()
                    .flex_none()
                    .text_size(fs(9.5))
                    .text_color(theme::faint())
                    .child(format!("→ {upstream}")),
            );
        }
    }
    if branch.ahead > 0 {
        row = row.child(widgets::pill(
            format!("↑{}", branch.ahead),
            theme::green(),
            theme::hover_soft(),
        ));
    }
    if branch.behind > 0 {
        row = row.child(widgets::pill(
            format!("↓{}", branch.behind),
            theme::amber(),
            theme::hover_soft(),
        ));
    }

    row.child(
        div()
            .flex_1()
            .min_w(px(0.))
            .truncate()
            .px(px(4.))
            .text_size(fs(10.))
            .text_color(theme::dim())
            .child(branch.subject.clone()),
    )
    .child(
        div()
            .flex_none()
            .w(px(80.))
            .text_right()
            .text_size(fs(9.5))
            .text_color(theme::faint())
            .child(fmt_time(branch.time)),
    )
    .child(
        div()
            .flex_none()
            .w(px(56.))
            .text_size(fs(9.5))
            .font_family(theme::mono())
            .text_color(theme::faint())
            .child(branch.oid.chars().take(7).collect::<String>()),
    )
}

fn tag_row(tag: &Tag, selected: Option<&str>, cx: &mut Context<GitApp>) -> Stateful<Div> {
    let key = format!("tag:{}", tag.name);
    let is_selected = selected == Some(key.as_str());
    let name = tag.name.clone();

    div()
        .id(format!("tag-{}", tag.name))
        .flex()
        .flex_row()
        .items_center()
        .h(px(26.))
        .px(px(10.))
        .gap(px(6.))
        .cursor_pointer()
        .bg(if is_selected { theme::selection() } else { theme::panel() })
        .hover(|s| s.bg(theme::hover_soft()))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_branch(format!("tag:{}", name), cx)
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener({
                let tag = tag.clone();
                move |this, ev: &MouseDownEvent, _, cx| this.open_tag_menu(tag.clone(), ev, cx)
            }),
        )
        .child(
            div()
                .w(px(12.))
                .flex_none()
                .child(Icon::new(IconName::Tag).with_size(px(11.)).text_color(theme::amber())),
        )
        .child(
            div()
                .flex_none()
                .max_w(px(260.))
                .truncate()
                .text_size(fs(11.5))
                .text_color(theme::amber())
                .child(tag.name.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .px(px(4.))
                .text_size(fs(10.))
                .text_color(theme::dim())
                .child(tag.subject.clone()),
        )
        .child(
            div()
                .flex_none()
                .w(px(80.))
                .text_right()
                .text_size(fs(9.5))
                .text_color(theme::faint())
                .child(fmt_time(tag.time)),
        )
        .child(
            div()
                .flex_none()
                .w(px(56.))
                .text_size(fs(9.5))
                .font_family(theme::mono())
                .text_color(theme::faint())
                .child(tag.oid.chars().take(7).collect::<String>()),
        )
}

//! 贮藏页：stash 列表 + 选中贮藏的差异。

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;
use gitsc_core::tags::Stash;

use crate::app::{ConfirmKind, GitApp, PromptKind, RepoState};
use crate::panels::diff;
use crate::theme::{self, fs};
use crate::widgets::{self, fmt_time};

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
        .child(sidebar(app, repo, cx))
        .child(preview(repo, cx))
}

fn sidebar(app: &GitApp, repo: &RepoState, cx: &mut Context<GitApp>) -> Div {
    let stashes = &repo.stashes;
    let selected = repo.selected_stash;

    let mut list = div()
        .id("stash-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll();

    if stashes.is_empty() {
        list = list.child(widgets::empty_state(
            IconName::Archive,
            "没有贮藏",
            "用「新建贮藏」把当前改动收起来，之后可以再取回。",
        ));
    } else {
        for stash in stashes {
            list = list.child(stash_row(stash, selected == Some(stash.index), cx));
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
        .child(Icon::new(IconName::Archive).xsmall().text_color(theme::dim()))
        .child(
            div()
                .text_size(fs(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("贮藏"),
        )
        .child(
            div()
                .text_size(fs(10.))
                .text_color(theme::faint())
                .child(format!("{} 条", stashes.len())),
        )
        .child(div().flex_1())
        .child(
            widgets::toolbar_button("stash-new", IconName::Plus, "新建贮藏").on_click(
                cx.listener(|this, _, window, cx| {
                    this.open_prompt(
                        PromptKind::Stash,
                        "新建贮藏",
                        "贮藏说明（可留空）",
                        "会把已跟踪文件的改动收起来",
                        "说明",
                        "",
                        window,
                        cx,
                    )
                }),
            ),
        );

    let _ = app;

    div()
        .flex()
        .flex_col()
        .w(px(380.))
        .flex_none()
        .min_h(px(0.))
        .bg(theme::panel())
        .border_r(px(1.))
        .border_color(theme::border())
        .child(header)
        .child(list)
}

fn stash_row(stash: &Stash, selected: bool, cx: &mut Context<GitApp>) -> Stateful<Div> {
    let index = stash.index;
    let mut row = div()
        .id(format!("stash-{}", index))
        .flex()
        .flex_col()
        .gap(px(3.))
        .px(px(10.))
        .py(px(6.))
        .cursor_pointer()
        .border_b(px(1.))
        .border_color(theme::border_soft())
        .bg(if selected { theme::selection() } else { theme::panel() })
        .hover(|s| s.bg(theme::hover_soft()))
        .on_click(cx.listener(move |this, _, _, cx| this.select_stash(index, cx)))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener({
                let stash = stash.clone();
                move |this, ev: &MouseDownEvent, _, cx| this.open_stash_menu(stash.clone(), ev, cx)
            }),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .flex_none()
                        .text_size(fs(10.5))
                        .font_family(theme::mono())
                        .text_color(theme::violet())
                        .child(format!("stash@{{{index}}}")),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_size(fs(9.5))
                        .text_color(theme::faint())
                        .child(fmt_time(stash.time)),
                ),
        )
        .child(
            div()
                .text_size(fs(11.))
                .text_color(theme::text())
                .truncate()
                .child(stash.message.clone()),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.))
                .child(
                    action_chip("stash-apply", IconName::ArrowDownToLine, "应用", theme::green()).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.run(crate::worker::Action::StashApply(index), cx)
                        }),
                    ),
                )
                .child(
                    action_chip("stash-pop", IconName::ArrowUpFromLine, "弹出", theme::accent()).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.run(crate::worker::Action::StashPop(index), cx)
                        }),
                    ),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id(format!("stash-drop-{index}"))
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(18.))
                        .h(px(18.))
                        .rounded(px(3.))
                        .cursor_pointer()
                        .text_color(theme::faint())
                        .hover(|s| s.bg(theme::hover_soft()).text_color(theme::red()))
                        .child(Icon::new(IconName::Trash).with_size(px(12.)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_confirm(
                                ConfirmKind::DropStash(index),
                                "删除贮藏",
                                format!("stash@{{{index}}} 将被删除，无法恢复。"),
                                "删除",
                                true,
                            )
                        })),
                ),
        );

    if selected {
        row = row.border_l(px(2.)).border_color(theme::accent());
    }
    row
}

fn action_chip(id: &'static str, icon: IconName, label: &'static str, color: Hsla) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(3.))
        .px(px(5.))
        .h(px(18.))
        .rounded(px(3.))
        .cursor_pointer()
        .bg(theme::panel_alt())
        .text_size(fs(9.5))
        .text_color(color)
        .hover(|s| s.bg(theme::hover()))
        .child(Icon::new(icon).with_size(px(10.)))
        .child(label)
}

fn preview(repo: &RepoState, _cx: &mut Context<GitApp>) -> Div {
    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(34.))
        .px(px(8.))
        .gap(px(8.))
        .flex_none()
        .bg(theme::panel())
        .border_b(px(1.))
        .border_color(theme::border())
        .child(
            div()
                .text_size(fs(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(if let Some(i) = repo.selected_stash {
                    format!("stash@{{{i}}} 的差异")
                } else {
                    "贮藏差异".to_string()
                }),
        )
        .child(div().flex_1());

    let body: AnyElement = match repo.selected_stash {
        None => hint("选择一条贮藏查看差异"),
        Some(_) => match repo.diff.as_ref() {
            Some(doc) => diff::body(doc).into_any_element(),
            None => hint("读取差异…"),
        },
    };

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .min_h(px(0.))
        .bg(theme::editor())
        .child(header)
        .child(div().flex().flex_col().flex_1().min_h(px(0.)).child(body))
}

fn hint(text: &'static str) -> AnyElement {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .text_size(fs(11.))
        .text_color(theme::faint())
        .child(text)
        .into_any_element()
}

//! 版本对比页：左右各选一个版本，中间是变更文件，右边是 diff。

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::{GitApp, ROW_H};
use crate::theme::{self, fs};
use crate::widgets;
use crate::worker::FileStat;

pub fn render(app: &GitApp, _window: &mut Window, cx: &mut Context<GitApp>) -> Div {
    let compare = &app.compare;
    let state = compare.clone();

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .h(px(38.))
        .px(px(10.))
        .bg(theme::panel())
        .border_b(px(1.))
        .border_color(theme::border())
        .child(
            div()
                .text_size(fs(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child("版本对比"),
        )
        .child(widgets::vdivider())
        .child(ref_button(
            "cmp-from",
            "从",
            &state.from,
            theme::accent(),
            true,
            cx,
        ))
        .child(Icon::new(IconName::ArrowRight).xsmall().text_color(theme::faint()))
        .child(ref_button(
            "cmp-to",
            "到",
            &state.to,
            theme::green(),
            false,
            cx,
        ))
        .child(
            widgets::icon_button("cmp-swap", IconName::ArrowLeftRight, theme::dim())
                .on_click(cx.listener(|this, _, _, cx| this.swap_compare(cx))),
        )
        .child(div().flex_1())
        .child(
            widgets::toolbar_button("cmp-refresh", IconName::RefreshCw, "重新比对")
                .on_click(cx.listener(|this, _, _, cx| this.compare_refresh(cx))),
        );

    let files = file_list(app, &state.files, state.selected.as_deref(), cx);
    let diff = match (&state.file_doc, &state.doc) {
        (Some(pane), _) => crate::panels::diff::body(pane),
        (None, Some(pane)) if state.error.is_none() => crate::panels::diff::body(pane),
        _ => {
            let empty = if state.loading {
                widgets::empty_state(IconName::LoaderCircle, "正在比对…", "读取两个版本之间的差异")
            } else if let Some(err) = &state.error {
                widgets::empty_state(IconName::CircleAlert, "比对失败", err)
            } else {
                widgets::empty_state(
                    IconName::GitCompare,
                    "两个版本完全一致",
                    "换一对分支、标签或提交试试",
                )
            };
            empty.into_any_element()
        }
    };

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(header)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h(px(0.))
                .child(files)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w(px(0.))
                        .h_full()
                        .bg(theme::editor())
                        .child(div().flex().flex_1().min_h(px(0.)).child(diff)),
                ),
        )
        .into()
}

fn ref_button(
    id: &'static str,
    label: &'static str,
    value: &str,
    color: Hsla,
    is_from: bool,
    cx: &mut Context<GitApp>,
) -> Stateful<Div> {
    let value = if value.is_empty() { "选择…" } else { value };
    let shown = abbreviate(value);
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(5.))
        .h(px(24.))
        .px(px(8.))
        .rounded(px(4.))
        .bg(theme::panel_alt())
        .border_1()
        .border_color(theme::border())
        .text_size(fs(11.5))
        .text_color(theme::text())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(
            div()
                .text_size(fs(10.))
                .text_color(color)
                .child(label.to_string()),
        )
        .child(widgets::mono_ellipsis(shown, 11.5, theme::text()))
        .child(Icon::new(IconName::ChevronsUpDown).xsmall().text_color(theme::faint()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                this.open_compare_ref_menu(if is_from { 0 } else { 1 }, ev, cx);
            }),
        )
}

fn file_list(
    app: &GitApp,
    files: &[FileStat],
    selected: Option<&str>,
    cx: &mut Context<GitApp>,
) -> Div {
    let mut list = div()
        .flex()
        .flex_col()
        .w(px(380.))
        .h_full()
        .bg(theme::panel())
        .border_r(px(1.))
        .border_color(theme::border())
        .overflow_hidden();

    let total_add: u64 = files.iter().filter_map(|f| f.1).map(u64::from).sum();
    let total_del: u64 = files.iter().filter_map(|f| f.2).map(u64::from).sum();
    let compare = &app.compare;

    list = list.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .h(px(34.))
            .px(px(10.))
            .border_b(px(1.))
            .border_color(theme::border())
            .child(
                div()
                    .text_size(fs(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(format!("{} 个文件", files.len())),
            )
            .child(div().flex_1())
            .child(widgets::delta(total_add as u32, total_del as u32)),
    );

    let mut rows = div()
        .id("cmp-files")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll();

    rows = rows.child(
        div()
            .id("cmp-all")
            .flex()
            .flex_row()
            .items_center()
            .h(px(ROW_H))
            .px(px(10.))
            .gap(px(6.))
            .cursor_pointer()
            .bg(if selected.is_none() {
                theme::selection()
            } else {
                theme::panel()
            })
            .hover(|s| s.bg(theme::hover()))
            .child(Icon::new(IconName::Files).xsmall().text_color(theme::dim()))
            .child(
                div()
                    .text_size(fs(11.5))
                    .text_color(theme::text())
                    .child("全部改动"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(fs(10.))
                    .text_color(theme::faint())
                    .child(format!("{} → {}", abbreviate(&compare.from), abbreviate(&compare.to))),
            )
            .on_click(cx.listener(|this, _, _, cx| this.select_compare_file(None, cx))),
    );

    for (path, added, removed) in files {
        let is_selected = selected == Some(path.as_str());
        let path_for_click = path.clone();
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let dir = path
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default();
        rows = rows.child(
            div()
                .id(path.clone())
                .flex()
                .flex_row()
                .items_center()
                .h(px(ROW_H))
                .px(px(10.))
                .gap(px(6.))
                .cursor_pointer()
                .bg(if is_selected {
                    theme::selection()
                } else {
                    theme::panel()
                })
                .hover(|s| s.bg(theme::hover()))
                .child({
                    // 单行显示：目录用暗色放在文件名前面，保证和右侧增删统计共用同一条基线。
                    let mut label = div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(5.))
                        .flex_1()
                        .min_w(px(0.));
                    if !dir.is_empty() {
                        label = label.child(widgets::ellipsis(
                            format!("{dir}/"),
                            10.,
                            theme::faint(),
                        ));
                    }
                    label.child(widgets::ellipsis(name, 11.5, theme::text()))
                })
                .child(widgets::delta(added.unwrap_or(0), removed.unwrap_or(0)).flex_shrink_0())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_compare_file(Some(path_for_click.clone()), cx)
                })),
        );
    }

    if files.is_empty() && !compare.loading {
        rows = rows.child(
            div()
                .px(px(10.))
                .py(px(10.))
                .text_size(fs(11.))
                .text_color(theme::faint())
                .child("这两个版本之间没有文件差异"),
        );
    }

    list.child(rows).into()
}

fn abbreviate(value: &str) -> String {
    let is_oid = value.len() >= 12 && value.chars().all(|c| c.is_ascii_hexdigit());
    if is_oid {
        value.chars().take(8).collect()
    } else if value.chars().count() > 30 {
        let mut shown: String = value.chars().take(29).collect();
        shown.push('…');
        shown
    } else {
        value.to_string()
    }
}

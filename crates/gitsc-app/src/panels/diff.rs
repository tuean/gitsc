//! diff 视图的渲染：行号栏 + 标记 + 文本，按行类型着色。

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::{DiffPane, DiffRow, RowKind};
use crate::theme::{self, fs};

pub fn body(pane: &DiffPane) -> AnyElement {
    if let Some(empty) = &pane.empty {
        return div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .p(px(20.))
            .text_size(fs(11.5))
            .text_color(theme::dim())
            .child(empty.clone())
            .into_any_element();
    }
    if pane.rows.is_empty() {
        return div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .text_size(fs(11.5))
            .text_color(theme::dim())
            .child("没有文本差异")
            .into_any_element();
    }

    let mut list = div()
        .id(("diff-scroll", pane.serial as usize))
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .w_full()
        .overflow_y_scroll()
        .bg(theme::editor());

    for (index, row) in pane.rows.iter().enumerate() {
        list = list.child(line(index, row));
    }
    if pane.truncated {
        list = list.child(
            div()
                .px(px(10.))
                .py(px(6.))
                .text_size(fs(10.5))
                .text_color(theme::amber())
                .child("差异过长，只显示前 6000 行"),
        );
    }
    list.into_any_element()
}

fn line(_index: usize, row: &DiffRow) -> Div {
    let (bg, fg, marker, marker_color) = match row.kind {
        RowKind::Add => (
            theme::diff_add_bg(),
            theme::text(),
            "+",
            theme::green(),
        ),
        RowKind::Del => (
            theme::diff_del_bg(),
            theme::text(),
            "-",
            theme::red(),
        ),
        RowKind::Ctx => (theme::editor(), theme::dim(), " ", theme::faint()),
        RowKind::Hunk => (
            theme::diff_hunk_bg(),
            theme::blue(),
            "",
            theme::faint(),
        ),
        RowKind::FileHeader => (
            theme::panel(),
            theme::text(),
            "",
            theme::faint(),
        ),
        RowKind::Meta => (
            theme::editor(),
            theme::faint(),
            "",
            theme::faint(),
        ),
    };

    if matches!(row.kind, RowKind::Hunk | RowKind::FileHeader) {
        return div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(20.))
            .w_full()
            .px(px(8.))
            .bg(bg)
            .text_size(fs(11.))
            .text_color(fg)
            .border_t(px(1.))
            .border_color(theme::border_soft())
            .child(
                div()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .truncate()
                    .font_family(theme::mono())
                    .child(row.text.clone()),
            );
    }

    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(18.))
        .w_full()
        .bg(bg)
        .text_size(fs(11.5))
        .font_family(theme::mono())
        .child(
            div()
                .w(px(42.))
                .pr(px(6.))
                .text_right()
                .text_size(fs(10.))
                .text_color(theme::faint())
                .child(row.old.map(|n| n.to_string()).unwrap_or_default()),
        )
        .child(
            div()
                .w(px(42.))
                .pr(px(6.))
                .text_right()
                .text_size(fs(10.))
                .text_color(theme::faint())
                .child(row.new.map(|n| n.to_string()).unwrap_or_default()),
        )
        .child(
            div()
                .w(px(14.))
                .text_center()
                .text_color(marker_color)
                .child(marker.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .truncate()
                .pl(px(4.))
                .text_color(fg)
                .child(row.text.clone()),
        )
}

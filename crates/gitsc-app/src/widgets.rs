//! 小部件与格式化：按钮、徽章、空态、时间与路径拆分。
//!
//! 刻意不套组件库的 Button：IDEA 的工具栏密度是 24/26px，自带按钮太胖，
//! 而且这里大部分交互需要「图标 + 文字 + hover 高亮」的紧凑组合。

use chrono::{Datelike, Local, TimeZone};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::Stateful;

use crate::theme;

pub fn fmt_time(ts: i64) -> String {
    let Some(dt) = Local.timestamp_opt(ts, 0).single() else {
        return String::new();
    };
    let now = Local::now();
    let today = now.date_naive();
    let day = dt.date_naive();
    if day == today {
        format!("{:02}:{:02}", dt.format("%H"), dt.format("%M"))
    } else if Some(day) == today.pred_opt() {
        format!("昨天 {:02}:{:02}", dt.format("%H"), dt.format("%M"))
    } else if dt.year() == now.year() {
        dt.format("%m-%d").to_string()
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

pub fn fmt_datetime(ts: i64) -> String {
    match Local.timestamp_opt(ts, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => String::new(),
    }
}

/// 路径拆成 (文件名, 目录前缀)；目录前缀带结尾 `/`。
pub fn split_path(path: &str) -> (String, String) {
    match path.rfind('/') {
        Some(i) => (path[i + 1..].to_string(), path[..=i].to_string()),
        None => (path.to_string(), String::new()),
    }
}

pub fn tab_label(tab: crate::app::Tab) -> &'static str {
    match tab {
        crate::app::Tab::Commit => "提交",
        crate::app::Tab::Compare => "对比",
        crate::app::Tab::Log => "日志",
        crate::app::Tab::Branches => "分支",
        crate::app::Tab::Stash => "贮藏",
        crate::app::Tab::Console => "控制台",
    }
}

pub fn tab_icon(tab: crate::app::Tab) -> IconName {
    match tab {
        crate::app::Tab::Commit => IconName::GitCommitHorizontal,
        crate::app::Tab::Compare => IconName::GitCompare,
        crate::app::Tab::Log => IconName::GitBranch,
        crate::app::Tab::Branches => IconName::FolderGit,
        crate::app::Tab::Stash => IconName::Archive,
        crate::app::Tab::Console => IconName::SquareTerminal,
    }
}

pub fn toolbar_button(id: &'static str, icon: IconName, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .h(px(26.))
        .px(px(9.))
        .rounded(px(5.))
        .text_size(px(12.))
        .text_color(theme::text())
        .hover(|s| s.bg(theme::hover()))
        .active(|s| s.bg(theme::panel_alt()))
        .cursor_pointer()
        .child(Icon::new(icon).small().text_color(theme::dim()))
        .child(label)
}

pub fn primary_button(id: &'static str, icon: IconName, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .h(px(26.))
        .px(px(12.))
        .rounded(px(5.))
        .text_size(px(12.))
        .text_color(rgb(0xffffff))
        .bg(theme::accent())
        .hover(|s| s.bg(theme::accent_hover()))
        .cursor_pointer()
        .child(Icon::new(icon).small().text_color(rgb(0xffffff)))
        .child(label)
}

pub fn icon_button(id: &'static str, icon: IconName, color: Hsla) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.))
        .rounded(px(4.))
        .text_color(color)
        .hover(|s| s.bg(theme::hover()))
        .cursor_pointer()
        .child(Icon::new(icon).small().text_color(color))
}

pub fn rail_button(id: &'static str, icon: IconName, label: &'static str, active: bool) -> Stateful<Div> {
    let color = if active { theme::accent() } else { theme::dim() };
    let mut base = div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(2.))
        .w_full()
        .h(px(46.))
        .cursor_pointer()
        .text_size(px(9.5))
        .text_color(color);
    if active {
        base = base
            .bg(theme::accent_soft())
            .border_l(px(2.))
            .border_color(theme::accent());
    } else {
        base = base.hover(|s| s.bg(theme::hover()));
    }
    base.child(Icon::new(icon).small().text_color(color))
        .child(label)
}

pub fn pill(text: impl Into<SharedString>, fg: Hsla, bg: Hsla) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.))
        .h(px(18.))
        .px(px(6.))
        .rounded(px(4.))
        .bg(bg)
        .text_size(px(11.))
        .text_color(fg)
        .child(text.into())
}

pub fn ref_pill(text: impl Into<SharedString>, fg: Hsla) -> Div {
    let bg = fg.opacity(0.16);
    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(16.))
        .px(px(5.))
        .rounded(px(3.))
        .bg(bg)
        .text_size(px(10.5))
        .text_color(fg)
        .child(text.into())
}

pub fn badge(letter: char, color: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(15.))
        .rounded(px(3.))
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(letter.to_string())
}

pub fn section_header(title: impl Into<SharedString>, count: Option<usize>) -> Div {
    let title: SharedString = title.into();
    let label = match count {
        Some(n) => format!("{title}  {n}"),
        None => title.to_string(),
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(26.))
        .px(px(10.))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::dim())
        .child(label)
}

pub fn divider() -> Div {
    div().h(px(1.)).w_full().bg(theme::border_soft())
}

pub fn vdivider() -> Div {
    div().w(px(1.)).h(px(18.)).bg(theme::border())
}

pub fn empty_state(icon: IconName, title: &str, hint: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .size_full()
        .p(px(24.))
        .child(Icon::new(icon).with_size(px(30.)).text_color(theme::faint()))
        .child(
            div()
                .text_size(px(13.))
                .text_color(theme::dim())
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(theme::faint())
                .child(hint.to_string()),
        )
}

pub fn kbd(text: &str) -> Div {
    div()
        .flex()
        .items_center()
        .h(px(18.))
        .px(px(5.))
        .rounded(px(3.))
        .border_1()
        .border_color(theme::border())
        .bg(theme::panel_alt())
        .text_size(px(10.))
        .text_color(theme::dim())
        .child(text.to_string())
}

/// 单行文本，超出截断。
pub fn ellipsis(text: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
    div()
        .min_w(px(0.))
        .overflow_hidden()
        .truncate()
        .text_size(px(size))
        .text_color(color)
        .child(text.into())
}

pub fn mono_ellipsis(text: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
    div()
        .min_w(px(0.))
        .overflow_hidden()
        .truncate()
        .font_family(theme::mono())
        .text_size(px(size))
        .text_color(color)
        .child(text.into())
}

/// `+N −N` 的增删统计。
pub fn delta(added: u32, removed: u32) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.))
        .font_family(theme::mono())
        .text_size(px(10.5))
        .child(
            div()
                .text_color(theme::green())
                .child(if added == 0 {
                    "·".to_string()
                } else {
                    format!("+{added}")
                }),
        )
        .child(
            div()
                .text_color(theme::red())
                .child(if removed == 0 {
                    "·".to_string()
                } else {
                    format!("−{removed}")
                }),
        )
}

/// 状态字母 + 颜色，用于变更列表与文件统计。
pub fn code_letter(code: gitsc_core::status::Code) -> (char, Hsla) {
    use gitsc_core::status::Code;
    match code {
        Code::Modified => ('M', theme::amber()),
        Code::Added => ('A', theme::green()),
        Code::Deleted => ('D', theme::red()),
        Code::Renamed => ('R', theme::violet()),
        Code::Copied => ('C', theme::cyan()),
        Code::TypeChanged => ('T', theme::violet()),
        Code::Unmerged => ('U', theme::red()),
        Code::Untracked => ('?', theme::green()),
        Code::Ignored => ('I', theme::faint()),
        Code::Unmodified => (' ', theme::dim()),
        Code::Unknown => ('?', theme::dim()),
    }
}

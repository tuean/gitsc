//! 控制台页：每条 git 命令的 argv、输出、退出码与耗时。
//!
//! 全部走 CLI 的一个直接好处：用户能看到这个客户端到底执行了什么，出问题时
//! 可以把命令直接复制到终端里跑。

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::GitApp;
use crate::theme::{self, fs};

pub fn render(app: &GitApp, _window: &mut Window, cx: &mut Context<GitApp>) -> Div {
    let mut list = div()
        .id("console-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .p(px(8.))
        .gap(px(8.));

    if app.console.is_empty() {
        list = list.child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .py(px(40.))
                .text_size(fs(11.))
                .text_color(theme::faint())
                .child("还没有执行过 git 命令"),
        );
    } else {
        for entry in app.console.iter() {
            let out = &entry.out;
            let failed = !out.ok();
            let mut block = div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .rounded(px(5.))
                .border_1()
                .border_color(if failed { theme::red() } else { theme::border_soft() })
                .bg(theme::editor())
                .p(px(8.))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            Icon::new(if failed {
                                IconName::CircleX
                            } else {
                                IconName::CircleCheck
                            })
                            .with_size(px(11.))
                            .text_color(if failed { theme::red() } else { theme::green() }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .truncate()
                                .text_size(fs(10.5))
                                .font_family(theme::mono())
                                .text_color(theme::accent())
                                .child(out.command_line()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(fs(9.))
                                .text_color(theme::faint())
                                .child(format!("{} ms", out.duration_ms)),
                        ),
                );

            if !out.stdout.trim().is_empty() {
                block = block.child(
                    div()
                        .text_size(fs(10.))
                        .font_family(theme::mono())
                        .text_color(theme::dim())
                        .child(out.stdout.trim_end().to_string()),
                );
            }
            if !out.stderr.trim().is_empty() {
                block = block.child(
                    div()
                        .text_size(fs(10.))
                        .font_family(theme::mono())
                        .text_color(theme::red())
                        .child(out.stderr.trim_end().to_string()),
                );
            }
            if failed && out.code.is_some() {
                block = block.child(
                    div()
                        .text_size(fs(9.))
                        .text_color(theme::faint())
                        .child(format!("退出码 {}", out.code.unwrap_or(-1))),
                );
            }
            list = list.child(block);
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
        .child(Icon::new(IconName::SquareTerminal).xsmall().text_color(theme::dim()))
        .child(
            div()
                .text_size(fs(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("控制台"),
        )
        .child(
            div()
                .text_size(fs(10.))
                .text_color(theme::faint())
                .child(format!("{} 条命令 · 保留最近 300 条", app.console.len())),
        )
        .child(div().flex_1())
        .child(
            div()
                .id("console-clear")
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
                .child(Icon::new(IconName::Trash).xsmall())
                .child("清空")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.console.clear();
                    cx.notify();
                })),
        );

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .w_full()
        .bg(theme::bg())
        .child(header)
        .child(list)
}

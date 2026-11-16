//! 欢迎页：没有打开仓库时的入口（打开 / 克隆 / 新建 + 最近仓库）。

use std::sync::Arc;

use gpui::{Image, ImageFormat, ImageSource};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::{expand_path, GitApp, PromptKind};
use crate::theme::{self, fs};
use crate::widgets;

/// 欢迎页里的品牌标记：直接嵌 256px 的 PNG，避免为一张图去改 AssetSource。
fn logo(size: f32) -> gpui::Img {
    let image = Image::from_bytes(
        ImageFormat::Png,
        include_bytes!("../../../../design/mark-256.png").to_vec(),
    );
    // 标记本身是竖长比例（264×296），按比例缩放。
    img(ImageSource::Image(Arc::new(image)))
        .h(px(size))
        .w(px(size * 264.0 / 296.0))
}

pub fn render(app: &GitApp, _window: &mut Window, cx: &mut Context<GitApp>) -> Div {
    let recents = app.settings.sorted_recents();

    let mut recent_list = div().flex().flex_col().gap(px(2.)).w_full();
    if recents.is_empty() {
        recent_list = recent_list.child(
            div()
                .py(px(18.))
                .text_size(fs(10.5))
                .text_color(theme::faint())
                .child("还没有打开过仓库"),
        );
    } else {
        for recent in recents {
            let path = recent.path.clone();
            let name = if recent.name.is_empty() {
                path.clone()
            } else {
                recent.name.clone()
            };
            let pinned = recent.pinned;
            let row = div()
                .id(format!("recent-{}", path))
                .flex()
                .flex_row()
                .items_center()
                .h(px(30.))
                .px(px(8.))
                .gap(px(8.))
                .rounded(px(4.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .on_click(cx.listener({
                    let path = path.clone();
                    move |this, _, _, cx| this.open_repo(expand_path(&path), cx)
                }))
                .child(Icon::new(IconName::FolderGit2).xsmall().text_color(theme::accent()))
                .child(
                    div()
                        .flex_none()
                        .w(px(150.))
                        .text_size(fs(11.))
                        .text_color(theme::text())
                        .truncate()
                        .child(name),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(fs(9.5))
                        .font_family(theme::mono())
                        .text_color(theme::faint())
                        .truncate()
                        .child(path.clone()),
                )
                .child(
                    div()
                        .id(format!("pin-{}", path))
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(20.))
                        .h(px(20.))
                        .rounded(px(3.))
                        .cursor_pointer()
                        .text_color(if pinned { theme::amber() } else { theme::faint() })
                        .hover(|s| s.bg(theme::hover_soft()).text_color(theme::amber()))
                        .child(Icon::new(IconName::Star).with_size(px(12.)))
                        .on_click(cx.listener({
                            let path = path.clone();
                            move |this, _, _, cx| {
                                this.settings.toggle_pin(&path);
                                let _ = this.settings.save();
                                this.refresh_welcome();
                                cx.notify();
                            }
                        })),
                )
                .child(
                    div()
                        .id(format!("forget-{}", path))
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(20.))
                        .h(px(20.))
                        .rounded(px(3.))
                        .cursor_pointer()
                        .text_color(theme::faint())
                        .hover(|s| s.bg(theme::hover_soft()).text_color(theme::red()))
                        .child(Icon::new(IconName::X).with_size(px(12.)))
                        .on_click(cx.listener({
                            let path = path.clone();
                            move |this, _, _, cx| {
                                this.settings.forget(&path);
                                let _ = this.settings.save();
                                this.refresh_welcome();
                                cx.notify();
                            }
                        })),
                );
            recent_list = recent_list.child(row);
        }
    }

    let card = div()
        .flex()
        .flex_col()
        .w(px(560.))
        .gap(px(14.))
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
                        .gap(px(10.))
                        .child(logo(44.))
                        .child(
                            div()
                                .text_size(px(24.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(theme::text())
                                .child("gitsc"),
                        ),
                )
                .child(
                    div()
                        .text_size(fs(11.5))
                        .text_color(theme::dim())
                        .child("一个 IDEA 风格的 Git 客户端：提交、历史、分支、贮藏各就其位。"),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .child(
                    widgets::primary_button("welcome-open", IconName::FolderOpen, "打开仓库…")
                        .on_click(cx.listener(|this, _, window, cx| this.pick_repo(window, cx))),
                )
                .child(
                    widgets::toolbar_button("welcome-clone", IconName::Download, "克隆仓库…").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.open_prompt(
                                PromptKind::Clone,
                                "克隆仓库",
                                "仓库地址",
                                "支持 https:// 与 git@ 两种写法，用系统 git 凭据",
                                "https://github.com/user/repo.git",
                                "",
                                window,
                                cx,
                            )
                        }),
                    ),
                )
                .child(
                    widgets::toolbar_button("welcome-init", IconName::Plus, "新建仓库…").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.open_prompt(
                                PromptKind::InitRepo,
                                "新建仓库",
                                "目录路径",
                                "目录不存在会先创建，然后 git init（主线为 main）",
                                "~/code/my-project",
                                "",
                                window,
                                cx,
                            )
                        }),
                    ),
                ),
        )
        .child(widgets::divider())
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(fs(10.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme::faint())
                        .child("最近打开"),
                )
                .child(recent_list),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.))
                .child(widgets::kbd("⌘O"))
                .child(
                    div()
                        .text_size(fs(9.5))
                        .text_color(theme::faint())
                        .child("打开仓库"),
                )
                .child(widgets::kbd("⌘R"))
                .child(
                    div()
                        .text_size(fs(9.5))
                        .text_color(theme::faint())
                        .child("刷新"),
                ),
        );

    div()
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .bg(theme::bg())
        .child(card)
}

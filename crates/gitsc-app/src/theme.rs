//! 配色与排版常量。风格对齐 IntelliJ IDEA 的新版深色主题（Darcula 的继任者）。

use gpui_kit::{px, rgb, rgba, Hsla, Pixels};

pub fn bg() -> Hsla {
    rgb(0x1e1f22).into()
}
/// 面板底色（工具栏、列表头、卡片）。
pub fn panel() -> Hsla {
    rgb(0x2b2d30).into()
}
/// 比面板更深一点：编辑器 / diff / 控制台。
pub fn editor() -> Hsla {
    rgb(0x1e1f22).into()
}
pub fn panel_alt() -> Hsla {
    rgb(0x26282b).into()
}
pub fn border() -> Hsla {
    rgb(0x393b40).into()
}
pub fn border_soft() -> Hsla {
    rgb(0x303236).into()
}
pub fn text() -> Hsla {
    rgb(0xdfe1e5).into()
}
pub fn dim() -> Hsla {
    rgb(0x9da0a8).into()
}
pub fn faint() -> Hsla {
    rgb(0x6f737a).into()
}
pub fn accent() -> Hsla {
    rgb(0x3574f0).into()
}
pub fn accent_hover() -> Hsla {
    rgb(0x467ef5).into()
}
pub fn accent_soft() -> Hsla {
    let c: Hsla = rgb(0x3574f0).into();
    c.opacity(0.20)
}
pub fn green() -> Hsla {
    rgb(0x62b543).into()
}
pub fn red() -> Hsla {
    rgb(0xdb5c5c).into()
}
pub fn amber() -> Hsla {
    rgb(0xd9a343).into()
}
pub fn violet() -> Hsla {
    rgb(0x9d7bd8).into()
}
pub fn cyan() -> Hsla {
    rgb(0x42b3a5).into()
}
pub fn blue() -> Hsla {
    rgb(0x5a9cf8).into()
}
pub fn selection() -> Hsla {
    rgb(0x2e436e).into()
}
pub fn hover() -> Hsla {
    rgb(0x35373b).into()
}
pub fn hover_soft() -> Hsla {
    rgb(0x2f3135).into()
}
pub fn scrim() -> Hsla {
    rgba(0x00000099).into()
}

/// 提交图泳道色。
pub const LANE_COLORS: [u32; 8] = [
    0x5a9cf8, 0x62b543, 0xd9a343, 0x9d7bd8, 0x42b3a5, 0xdb5c5c, 0xc27a4a, 0xa0b0c0,
];

pub fn lane(i: usize) -> Hsla {
    rgb(LANE_COLORS[i % LANE_COLORS.len()]).into()
}

/// diff 背景：加行 / 删行 / 行号栏。
pub fn diff_add_bg() -> Hsla {
    rgba(0x35523955).into()
}
pub fn diff_del_bg() -> Hsla {
    rgba(0x5a2b2b55).into()
}
pub fn diff_add_word() -> Hsla {
    rgba(0x3f7a4a88).into()
}
pub fn diff_del_word() -> Hsla {
    rgba(0x8a3d3d88).into()
}
pub fn diff_hunk_bg() -> Hsla {
    rgb(0x2b2d30).into()
}

pub fn fs(size: f32) -> Pixels {
    px(size)
}

pub fn mono() -> &'static str {
    if cfg!(target_os = "windows") {
        "Consolas"
    } else if cfg!(target_os = "linux") {
        "DejaVu Sans Mono"
    } else {
        "Menlo"
    }
}

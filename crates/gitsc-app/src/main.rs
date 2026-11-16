//! gitsc —— 参考 IDEA 的 Git 操作台。
//!
//! 进程结构：
//!
//! ```text
//!   main            GPUI：窗口、渲染、按键
//!   gitsc-git × 1   阻塞 git CLI：串行跑命令，结果回 UI
//! ```
//!
//! UI 线程不做 IO。所有写操作、状态读取都交给 worker，命令与输出同步进控制台，
//! 用户随时能看到这个客户端到底跑过什么。

// Windows 上不弹控制台窗口；日志只写文件。
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![allow(unexpected_cfgs)]

mod app;
mod panels;
mod theme;
mod widgets;
mod worker;

use std::path::PathBuf;

use app::GitApp;
use gpui_kit::{
    point, px, size, App as GpuiApp, AppContext as _, Bounds, SharedString, TitlebarOptions,
    WindowBounds, WindowOptions,
};

fn main() -> anyhow::Result<()> {
    init_logging();
    tracing::info!("---- gitsc {} starting ----", env!("CARGO_PKG_VERSION"));

    // 可选参数：`gitsc [path]` 直接打开某个仓库。
    let initial: Option<PathBuf> = std::env::args()
        .nth(1)
        .filter(|arg| !arg.starts_with('-'))
        .map(PathBuf::from);

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .with_quit_mode(gpui_kit::QuitMode::LastWindowClosed)
        .run(move |cx: &mut GpuiApp| {
            gpui_kit::init(cx);
            // 整个界面按 IDEA 深色配色，组件库默认跟随系统（可能浅色）。
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);

            let title = "gitsc".to_string();
            let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: Some(TitlebarOptions {
                            title: Some(SharedString::from(title)),
                            appears_transparent: true,
                            traffic_light_position: Some(point(px(12.), px(12.))),
                        }),
                        window_min_size: Some(size(px(1024.), px(640.))),
                        ..Default::default()
                    },
                    move |window, cx| {
                        let initial = initial.clone();
                        let app = cx.new(|cx| GitApp::new(initial, window, cx));
                        // Root 负责主题、通知与浮层容器。
                        cx.new(|cx| gpui_kit::component::Root::new(app, window, cx))
                    },
                )
                .expect("failed to open window");

            let _ = window;
            cx.activate(true);
        });

    Ok(())
}

/// 终端 + 文件双写：从 Finder 启动时没有终端，出问题只能看文件。
fn init_logging() {
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::Layer as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    let filter = || {
        tracing_subscriber::EnvFilter::try_from_env("RUST_LOG")
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };

    let log_dir = gitsc_core::config_dir();
    let file = std::fs::create_dir_all(&log_dir)
        .ok()
        .and_then(|_| std::fs::OpenOptions::new().create(true).append(true).open(log_dir.join("gitsc.log")).ok())
        .and_then(|f| {
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(f))
                .with_filter(filter())
                .into()
        });

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(filter()),
        )
        .with(file)
        .init();
}

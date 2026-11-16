fn main() {
    // 只在为 Windows 目标构建时嵌入图标资源；macOS/Linux 构建脚本是空操作。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // 没有 rc.exe/windres 时静默降级（不带图标），不要让构建失败。
        let _ = embed_resource::compile("gitsc.rc", embed_resource::NONE).manifest_optional();
    }
}

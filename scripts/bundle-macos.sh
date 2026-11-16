#!/usr/bin/env bash
# 打包成 .app：只有 bundle 起来的进程才能正常激活窗口、显示通知与 Dock 图标。
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p gitsc-app

APP=target/gitsc.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/gitsc "$APP/Contents/MacOS/gitsc"
cp design/gitsc.icns "$APP/Contents/Resources/gitsc.icns"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>gitsc</string>
  <key>CFBundleDisplayName</key><string>gitsc</string>
  <key>CFBundleIdentifier</key><string>dev.gitsc.app</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>gitsc</string>
  <key>CFBundleIconFile</key><string>gitsc</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

# 本机自签，避免每次启动都被 Gatekeeper 拦。
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
echo "built $APP"

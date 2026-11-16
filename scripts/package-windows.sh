#!/usr/bin/env bash
# 在 macOS 上交叉编译 Windows exe 并打 NSIS 安装包。
# 前置：brew install mingw-w64 nsis；rustup target add x86_64-pc-windows-gnu
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
: "${VERSION:?Cargo.toml 里没找到 version 字段}"

./scripts/build-windows.sh

makensis -DAPP_VERSION="$VERSION" installer/gitsc.nsi

echo
ls -lh "dist/gitsc-windows-x86_64.exe" "dist/gitsc-${VERSION}-windows-x64-setup.exe"

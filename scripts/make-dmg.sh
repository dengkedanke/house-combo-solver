#!/usr/bin/env bash
# 直接生成 macOS dmg，避免 create-dmg 的 AppleScript 步骤
# （该步骤需 Finder 自动化授权，在无图形会话/后台构建环境中会被拒绝：-10004）。
# 生成结果：App + Applications 快捷方式，可正常安装使用（不依赖 Finder 美化）。
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJ_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_NAME="HouseComboSolver"
VERSION="0.1.0"
ARCH="aarch64"

APP_PATH="$PROJ_ROOT/src-tauri/target/release/bundle/macos/${APP_NAME}.app"
DMG_DIR="$PROJ_ROOT/src-tauri/target/release/bundle/dmg"
DMG_PATH="$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg"

if [[ ! -d "$APP_PATH" ]]; then
  echo "未找到已构建的 .app：$APP_PATH" >&2
  echo "请先执行 'npm run tauri build'（其会在 dmg 步骤失败，但已生成 .app）。" >&2
  exit 1
fi

mkdir -p "$DMG_DIR"

# 准备临时内容目录：拷贝 App 并创建 Applications 快捷方式
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cp -R "$APP_PATH" "$TMP/"
ln -s /Applications "$TMP/Applications"

# 若已存在则先删除，避免 hdiutil 报错
rm -f "$DMG_PATH"

echo "正在生成 dmg：$DMG_PATH"
hdiutil create \
  -volname "$APP_NAME" \
  -srcfolder "$TMP" \
  -format UDZO \
  -imagekey zlib-level=9 \
  "$DMG_PATH"

echo "完成：$(du -h "$DMG_PATH" | cut -f1)"

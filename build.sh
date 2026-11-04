#!/usr/bin/env bash
# =============================================================================
#  GlassMC（Tauri 跨平台版）· macOS 打包
#
#  产物：
#    build/GlassMC.app      universal（arm64 + x86_64）
#    build/GlassMC.dmg      带安装指引背景与图标布局
#
#  为什么不用 `cargo tauri build`：
#    本机没有装 tauri-cli，且它的打包流程会顺带改写 Info.plist 里我们需要
#    精细控制的部分（LSMinimumSystemVersion、沙盒开关、图标名）。
#    这里直接 lipo + 手写 plist，路径最短也最可控。
# =============================================================================
set -euo pipefail
cd "$(dirname "$0")"

APP_NAME="GlassMC"
BUNDLE_ID="com.glassmc.launcher"
OUT="build"
SRC_TAURI="src-tauri"

# 版本号只在 tauri.conf.json 里写一份，这里读出来用
VERSION="$(python3 -c 'import json,sys;print(json.load(open("src-tauri/tauri.conf.json"))["version"])')"
[[ -n "$VERSION" ]] || { echo "读不到版本号"; exit 1; }

SKIP_BUILD=0
[[ "${1:-}" == "--skip-build" ]] && SKIP_BUILD=1

# ---------------------------------------------------------------- 1. 编译
if [[ $SKIP_BUILD -eq 0 ]]; then
  echo "==> 编译 arm64"
  ( cd "$SRC_TAURI" && cargo build --release --target aarch64-apple-darwin )
  echo "==> 编译 x86_64"
  ( cd "$SRC_TAURI" && cargo build --release --target x86_64-apple-darwin )
fi

ARM="$SRC_TAURI/target/aarch64-apple-darwin/release/glassmc"
INTEL="$SRC_TAURI/target/x86_64-apple-darwin/release/glassmc"
for f in "$ARM" "$INTEL"; do
  [[ -f "$f" ]] || { echo "缺少编译产物：$f"; exit 1; }
done

mkdir -p "$OUT"
echo "==> 合并为 universal"
lipo -create "$ARM" "$INTEL" -output "$OUT/$APP_NAME"
chmod +x "$OUT/$APP_NAME"
echo -n "    架构："; lipo -archs "$OUT/$APP_NAME"

# ---------------------------------------------------------------- 2. 组装 app
echo "==> 组装 $APP_NAME.app"
rm -rf "$OUT/$APP_NAME.app"
mkdir -p "$OUT/$APP_NAME.app/Contents/MacOS" "$OUT/$APP_NAME.app/Contents/Resources"

cp "$OUT/$APP_NAME" "$OUT/$APP_NAME.app/Contents/MacOS/$APP_NAME"
cp "$SRC_TAURI/icons/icon.icns" "$OUT/$APP_NAME.app/Contents/Resources/AppIcon.icns"

cat > "$OUT/$APP_NAME.app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key><string>${APP_NAME}</string>
    <key>CFBundleExecutable</key><string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleLocalizations</key>
    <array><string>zh_CN</string><string>en</string></array>
    <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSRequiresAquaSystemAppearance</key><false/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

echo "==> ad-hoc 签名"
codesign --force --deep --sign - "$OUT/$APP_NAME.app" 2>/dev/null
codesign --verify --deep --strict "$OUT/$APP_NAME.app" && echo "    签名校验通过"

# ---------------------------------------------------------------- 3. DMG
if command -v dmgbuild >/dev/null 2>&1; then
  echo "==> 生成 DMG"
  rm -f "$OUT/$APP_NAME.dmg"
  dmgbuild -s Scripts/dmg_settings.py -D app="$OUT/$APP_NAME.app" "$APP_NAME" "$OUT/$APP_NAME.dmg"
  echo -n "    DMG 大小："; du -h "$OUT/$APP_NAME.dmg" | awk '{print $1}'
else
  echo "跳过 DMG（未安装 dmgbuild）"
fi

echo ""
echo "完成："
echo "  $OUT/$APP_NAME.app"
[[ -f "$OUT/$APP_NAME.dmg" ]] && echo "  $OUT/$APP_NAME.dmg"

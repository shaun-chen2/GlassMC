#!/bin/bash
# GlassMC 构建脚本：生成资源 → 编译 → 打包 .app → 签名 → 生成带背景图与图标的 .dmg
# 需在已安装 Command Line Tools 的 macOS 上运行（无需完整 Xcode）。
set -uo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR"

APP_NAME=GlassMC
BUILD="$SCRIPT_DIR/build"
APP="$BUILD/$APP_NAME.app"
DMG="$SCRIPT_DIR/$APP_NAME.dmg"

echo "== 1/6 生成图标与 DMG 背景图 =="
swiftc -O Scripts/generate_assets.swift -o "$BUILD/gen_assets" -framework AppKit -framework Foundation
./build/gen_assets "$SCRIPT_DIR/Assets"
test -f "$SCRIPT_DIR/Assets/AppIcon.icns" && echo "  icon ok" || { echo "图标生成失败"; exit 1; }

echo "== 2/6 编译 Swift 源码 =="
SOURCES=$(find Sources -name '*.swift' | sort)
mkdir -p "$BUILD"
swiftc -target arm64-apple-macosx15.0 -O -parse-as-library \
  -o "$BUILD/$APP_NAME" $SOURCES \
  -framework SwiftUI -framework AppKit -framework Foundation
test -f "$BUILD/$APP_NAME" && echo "  编译成功" || { echo "编译失败"; exit 1; }

echo "== 3/6 组装 .app 包 =="
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BUILD/$APP_NAME" "$APP/Contents/MacOS/$APP_NAME"
cp "$SCRIPT_DIR/Assets/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
chmod +x "$APP/Contents/MacOS/$APP_NAME"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key><string>${APP_NAME}</string>
    <key>CFBundleExecutable</key><string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key><string>com.glassmc.launcher</string>
    <key>CFBundleVersion</key><string>1.1</string>
    <key>CFBundleShortVersionString</key><string>1.1</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSMinimumSystemVersion</key><string>15.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>NSHumanReadableCopyright</key><string>GlassMC · 液态玻璃 Minecraft 启动器</string>
</dict>
</plist>
EOF
printf 'APPL????' > "$APP/Contents/PkgInfo"

echo "== 4/6 Ad-hoc 签名（本地运行） =="
codesign --force --deep --sign - "$APP" && echo "  签名完成" || echo "  签名跳过（不影响本地运行）"

echo "== 5/6 用 dmgbuild 生成带背景与图标布局的 DMG =="
# dmgbuild 直接写 .DS_Store，不依赖 Finder GUI 自动化（适配无头/沙箱环境）。
rm -f "$DMG"
python3 -m dmgbuild -s "$SCRIPT_DIR/dmg_settings.py" "$APP_NAME" "$DMG" 2>&1 | tail -15
test -f "$DMG" && echo "  DMG 生成成功" || { echo "DMG 生成失败"; exit 1; }

echo ""
echo "完成 ✅"
echo "  App: $APP"
echo "  DMG: $DMG"
echo "双击 DMG 后将 App 拖到「应用程序」即可使用。"

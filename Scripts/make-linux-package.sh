#!/usr/bin/env bash
# 把编译好的 Linux 二进制打成 .deb 和 .tar.gz
#
# 用法（在仓库根目录）：
#   bash Scripts/make-linux-package.sh [二进制路径]
#
# 默认读 src-tauri/target/release/glassmc。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${1:-$ROOT/src-tauri/target/release/glassmc}"
VERSION="$(grep -m1 '^version' "$ROOT/src-tauri/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"
OUT="$ROOT/build/linux"
PKG="$OUT/glassmc_${VERSION}_${ARCH}"

[ -x "$BIN" ] || { echo "找不到可执行文件：$BIN（先 cargo build --release）" >&2; exit 1; }

rm -rf "$OUT"; mkdir -p "$OUT"
mkdir -p "$PKG/DEBIAN" \
         "$PKG/usr/bin" \
         "$PKG/usr/share/applications" \
         "$PKG/usr/share/icons/hicolor/512x512/apps" \
         "$PKG/usr/share/doc/glassmc"

install -m 755 "$BIN" "$PKG/usr/bin/glassmc"
install -m 644 "$ROOT/src-tauri/icons/icon.png" "$PKG/usr/share/icons/hicolor/512x512/apps/glassmc.png"

cat > "$PKG/usr/share/applications/glassmc.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=GlassMC
GenericName=Minecraft Launcher
Comment=液态玻璃风格的 Minecraft 启动器
Comment[en]=Liquid glass Minecraft launcher
Exec=glassmc
Icon=glassmc
Terminal=false
Categories=Game;
Keywords=minecraft;launcher;glassmc;
StartupWMClass=glassmc
DESKTOP

cp "$ROOT/LICENSE" "$PKG/usr/share/doc/glassmc/copyright" 2>/dev/null || true

INSTALLED_KB="$(du -sk "$PKG" | cut -f1)"
cat > "$PKG/DEBIAN/control" <<CONTROL
Package: glassmc
Version: ${VERSION}
Section: games
Priority: optional
Architecture: ${ARCH}
Installed-Size: ${INSTALLED_KB}
Maintainer: GlassMC <noreply@example.com>
Homepage: https://shaun-chen2.github.io/GlassMC/
Depends: libwebkit2gtk-4.1-0 (>= 2.40), libgtk-3-0, libayatana-appindicator3-1 | libappindicator3-1
Recommends: libgl1-mesa-dri
Description: 液态玻璃 Minecraft 启动器
 装版本、装加载器、装 Java、模组与整合包、多账号登录、
 陶瓦联机。界面使用系统级毛玻璃，macOS / Windows / Linux 同一套代码。
CONTROL

# dpkg 要求目录 755、普通文件 644，否则 lintian 会报权限异常
find "$PKG" -type d -exec chmod 755 {} +
find "$PKG" -type f -exec chmod 644 {} +
chmod 755 "$PKG/usr/bin/glassmc"

dpkg-deb --root-owner-group -Zxz --build "$PKG" "$OUT/GlassMC_${VERSION}_${ARCH}.deb" >/dev/null
echo "deb: $OUT/GlassMC_${VERSION}_${ARCH}.deb ($(du -h "$OUT/GlassMC_${VERSION}_${ARCH}.deb" | cut -f1))"

# 绿色版 tar.gz，解压即可运行
STAGE="$OUT/stage"
mkdir -p "$STAGE"
install -m 755 "$BIN" "$STAGE/glassmc"
install -m 644 "$ROOT/src-tauri/icons/icon.png" "$STAGE/glassmc.png"
install -m 644 "$ROOT/LICENSE" "$STAGE/LICENSE" 2>/dev/null || true
tar -czf "$OUT/GlassMC_${VERSION}_linux-${ARCH}.tar.gz" -C "$STAGE" .
echo "tar.gz: $OUT/GlassMC_${VERSION}_linux-${ARCH}.tar.gz ($(du -h "$OUT/GlassMC_${VERSION}_linux-${ARCH}.tar.gz" | cut -f1))"
ls -la "$OUT"

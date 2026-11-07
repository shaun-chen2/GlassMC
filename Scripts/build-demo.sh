#!/usr/bin/env bash
# 打包 GlassMC 在线体验页
#
# 体验页不需要另写一套界面：前端本身就是无构建的静态页，
# 检测不到 Tauri 时 app.js 会自动走 mockInvoke。
# 这里只做三件事：拷一份 src/、把 demo 的外壳样式与脚本拼进去。
#
# 产物在 demo/site，接着 `cd demo && npx wrangler deploy` 即可上线。
set -euo pipefail

cd "$(dirname "$0")/.."

OUT="demo/site"

rm -rf "$OUT"
mkdir -p "$OUT"
cp -R src/. "$OUT"/
cp demo/overlay.css demo/overlay.js "$OUT"/

# 首帧就要带上 data-demo，否则会先闪一下「铺满视口」再缩成悬浮窗
perl -0pi -e 's{</head>}{<link rel="stylesheet" href="overlay.css">\n<script>document.documentElement.setAttribute("data-demo","")</script>\n</head>}' "$OUT/index.html"
perl -0pi -e 's{</body>}{<script src="overlay.js"></script>\n</body>}' "$OUT/index.html"

grep -q 'overlay.css' "$OUT/index.html" || { echo "注入失败：index.html 里没找到 overlay.css" >&2; exit 1; }
grep -q 'overlay.js' "$OUT/index.html" || { echo "注入失败：index.html 里没找到 overlay.js" >&2; exit 1; }
grep -q 'data-demo' "$OUT/index.html" || { echo "注入失败：index.html 里没找到 data-demo" >&2; exit 1; }

echo "体验页已生成：${OUT}（$(du -sh "$OUT" | cut -f1)）"

#!/usr/bin/env bash
# 打包 glassmc.shuan.uk 整站
#
#   /      介绍页（docs/index.html，单文件 + 图标）
#   /web   在线体验（前端 src/ 原样拷过去，再拼上 demo 的展示外壳）
#
# 体验页不需要另写一套界面：前端本来就是无构建的静态页，
# 检测不到 Tauri 时 app.js 会自动走 mockInvoke。
#
# 产物在 demo/site，接着 `cd demo && npx wrangler deploy` 上线。
set -euo pipefail

cd "$(dirname "$0")/.."

OUT="demo/site"

rm -rf "$OUT"
mkdir -p "$OUT/web"

# ---- 介绍页
cp docs/index.html "$OUT/index.html"   # 单文件：开场动画的 JS 已内联在页面里
cp -R docs/images "$OUT/images"

# ---- 体验页
cp -R src/. "$OUT/web"/
cp demo/overlay.css demo/overlay.js "$OUT/web"/

# 首帧就要带上 data-demo，否则会先闪一下「铺满视口」再缩成悬浮窗
perl -0pi -e 's{</head>}{<link rel="stylesheet" href="overlay.css">\n<script>document.documentElement.setAttribute("data-demo","")</script>\n</head>}' "$OUT/web/index.html"
perl -0pi -e 's{</body>}{<script src="overlay.js"></script>\n</body>}' "$OUT/web/index.html"

for f in "$OUT/web/index.html"; do
  grep -q 'overlay.css' "$f" || { echo "注入失败：$f 里没找到 overlay.css" >&2; exit 1; }
  grep -q 'overlay.js' "$f" || { echo "注入失败：$f 里没找到 overlay.js" >&2; exit 1; }
  grep -q 'data-demo' "$f" || { echo "注入失败：$f 里没找到 data-demo" >&2; exit 1; }
done
[ -f "$OUT/images/icon-256.png" ] || { echo "缺少介绍页图标 images/icon-256.png" >&2; exit 1; }

echo "整站已生成：${OUT}（$(du -sh "$OUT" | cut -f1)）"
echo "  /     介绍页"
echo "  /web  在线体验"

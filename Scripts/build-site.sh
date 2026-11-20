#!/usr/bin/env bash
# 打包 glassmc.shuan.uk 整站
#
#   /               介绍页（docs/index.html，单文件 + 图标）
#   /web            在线体验（前端 src/ 原样拷过去，再拼上 demo 的展示外壳）
#   /robots.txt     AI 爬虫放行 + sitemap 指路
#   /sitemap.xml    两个 HTML 页面
#   /llms.txt       给 LLM 的摘要索引（llmstxt.org 格式）
#   /llms-full.txt  整页正文 + 三份技术文档的纯文本语料
#   /docs/*.md      技术文档原文
#
# 体验页不需要另写一套界面：前端本来就是无构建的静态页，
# 检测不到 Tauri 时 app.js 会自动走 mockInvoke。
#
# 产物在 demo/site，接着 `cd demo && npx wrangler deploy` 上线。
set -euo pipefail

cd "$(dirname "$0")/.."

OUT="demo/site"

# ---- GEO 资产：结构化数据、robots、sitemap、llms.txt 全部在这里生成
python3 Scripts/gen-geo.py

rm -rf "$OUT"
mkdir -p "$OUT/web" "$OUT/docs"

# ---- 介绍页
cp docs/index.html "$OUT/index.html"   # 单文件：开场动画的 JS 已内联在页面里
cp -R docs/images "$OUT/images"

# ---- GEO 与文档
cp docs/robots.txt docs/sitemap.xml docs/llms.txt docs/llms-full.txt "$OUT/"
cp docs/*.md "$OUT/docs/"

# ---- 体验页
cp -R src/. "$OUT/web"/
cp demo/overlay.css demo/overlay.js demo/wallpaper.jpg "$OUT/web"/

# 首帧就要带上 data-demo，否则会先闪一下「铺满视口」再缩成悬浮窗
perl -0pi -e 's{</head>}{<link rel="stylesheet" href="overlay.css">\n<script>document.documentElement.setAttribute("data-demo","")</script>\n</head>}' "$OUT/web/index.html"
perl -0pi -e 's{</body>}{<script src="overlay.js"></script>\n</body>}' "$OUT/web/index.html"

for f in "$OUT/web/index.html"; do
  grep -q 'overlay.css' "$f" || { echo "注入失败：$f 里没找到 overlay.css" >&2; exit 1; }
  grep -q 'overlay.js' "$f" || { echo "注入失败：$f 里没找到 overlay.js" >&2; exit 1; }
  grep -q 'data-demo' "$f" || { echo "注入失败：$f 里没找到 data-demo" >&2; exit 1; }
done

# 体验页的标题 / 描述 / canonical / JSON-LD（不改 src/ 里的应用本体）
python3 Scripts/gen-geo.py --web "$OUT/web/index.html"

[ -f "$OUT/images/icon-256.png" ] || { echo "缺少介绍页图标 images/icon-256.png" >&2; exit 1; }
[ -f "$OUT/images/og-cover.png" ] || { echo "缺少社交封面 images/og-cover.png（先跑 Scripts/gen-og.py）" >&2; exit 1; }
[ -f "$OUT/web/wallpaper.jpg" ] || { echo "缺少体验页壁纸 demo/wallpaper.jpg" >&2; exit 1; }
for f in "$OUT/robots.txt" "$OUT/sitemap.xml" "$OUT/llms.txt" "$OUT/llms-full.txt" "$OUT/docs/architecture.md"; do
  [ -f "$f" ] || { echo "缺少 $f" >&2; exit 1; }
done

echo "整站已生成：${OUT}（$(du -sh "$OUT" | cut -f1)）"
echo "  /              介绍页"
echo "  /web           在线体验"
echo "  /llms.txt      LLM 摘要索引（$(wc -c < "$OUT/llms.txt" | tr -d ' ') 字节）"
echo "  /llms-full.txt 全文语料（$(wc -c < "$OUT/llms-full.txt" | tr -d ' ') 字节）"
echo "  /robots.txt /sitemap.xml /docs/*.md"

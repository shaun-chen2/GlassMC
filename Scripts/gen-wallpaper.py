#!/usr/bin/env python3
"""生成在线体验页的壁纸（demo/wallpaper.jpg）。

为什么不用纯 CSS 渐变：渐变被 blur 之后还是同一片平滑渐变，
透过玻璃看不出任何变化。这张图分两层做：

  低频 —— 几块大色团，负责「颜色」；
  高频 —— 斜向光带、bokeh 光斑、细碎星点，负责「细节」。

细节被 backdrop-filter 糊掉的差别肉眼一眼可辨，玻璃才有存在感。
只用 Pillow，无 numpy 依赖；固定随机种子，重跑结果一致。
"""
import math
import random

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageOps

W, H = 2560, 1600
OUT = "/Users/chenhehe/WorkBuddy/2026-11-01-09-09-01/GlassMC/demo/wallpaper.jpg"

random.seed(20261114)


def mul(img, k):
    """整体乘一个系数（Pillow 没有现成的，用 multiply 实现）。"""
    v = max(0, min(255, int(round(255 * k))))
    return ImageChops.multiply(img, Image.new("RGB", img.size, (v, v, v)))


# ---------- 1. 底色：凌晨海面，从上到下压暗 ----------
base = Image.new("RGB", (W, H), (6, 9, 16))
d = ImageDraw.Draw(base)
for y in range(H):
    t = y / H
    k = math.sin(t * math.pi)          # 中间略微提亮
    d.line(
        [(0, y), (W, y)],
        fill=(int(9 + 7 * (1 - t) + 5 * k),
              int(13 + 11 * (1 - t) + 7 * k),
              int(24 + 24 * (1 - t) + 12 * k)),
    )
base = mul(base, 0.92)

# ---------- 2. 低频：五块大色团 ----------
blobs = [
    (0.14, 0.18, 0.42, (46, 92, 255), 0.92),   # 电光蓝
    (0.80, 0.12, 0.38, (124, 58, 237), 0.86),  # 紫
    (0.66, 0.82, 0.44, (6, 182, 212), 0.78),   # 青
    (0.08, 0.88, 0.34, (245, 158, 11), 0.50),  # 暖橙，压住左下角
    (0.95, 0.64, 0.30, (236, 72, 153), 0.32),  # 洋红，很淡
]
layer = Image.new("RGB", (W, H), (0, 0, 0))
dl = ImageDraw.Draw(layer)
for cx, cy, rr, col, amt in blobs:
    x, y = cx * W, cy * H
    r = rr * min(W, H)
    dl.ellipse([x - r, y - r, x + r, y + r], fill=tuple(int(c * amt) for c in col))
layer = layer.filter(ImageFilter.GaussianBlur(210))

img = ImageChops.add(base, mul(layer, 1.02))

# ---------- 2.5 两束大光带：大尺度、高亮度，用来「看得见在动」
# 玻璃后面只有小尺度细节在动，肉眼很难察觉；一条亮带的缓慢横移一眼就能看出来。
glow = Image.new("RGB", (W, H), (0, 0, 0))
dg = ImageDraw.Draw(glow)
for cx, cy, rx, ry, col, amt in [
    (0.30, 0.34, 0.34, 0.10, (120, 190, 255), 0.30),
    (0.72, 0.60, 0.30, 0.09, (110, 240, 220), 0.22),
]:
    dg.ellipse(
        [(cx - rx) * W, (cy - ry) * H, (cx + rx) * W, (cy + ry) * H],
        fill=tuple(int(c * amt) for c in col),
    )
glow = glow.filter(ImageFilter.GaussianBlur(120))
img = ImageChops.add(img, glow)

# ---------- 3. 暗角：中心偏上最亮，四角压下去 ----------
rad = Image.radial_gradient("L").resize((int(W * 1.45), int(H * 1.55)), Image.LANCZOS)
rad = ImageOps.invert(rad)                       # 中心白、边缘黑
canvas = Image.new("L", (W, H), 0)
canvas.paste(rad, (int(-W * 0.225), int(-H * 0.26)))   # 让亮点落在 (0.5, 0.42) 附近
mask = canvas.point(lambda v: int(92 + 163 * (v / 255)))  # 0.36 → 1.0
img = ImageChops.multiply(img, Image.merge("RGB", (mask, mask, mask)))

# ---------- 4. 高频：斜向光带 ----------
streaks = Image.new("RGB", (W, H), (0, 0, 0))
ds = ImageDraw.Draw(streaks)
for _ in range(11):
    x0 = random.uniform(-0.2, 1.1) * W
    y0 = random.uniform(-0.1, 0.9) * H
    length = random.uniform(0.5, 1.0) * W
    slope = random.uniform(0.45, 0.85)
    alpha = random.randint(30, 80)
    col = random.choice([(180, 214, 255), (150, 190, 255), (210, 230, 255)])
    ds.line([(x0, y0), (x0 + length, y0 - length * slope)],
            fill=tuple(int(c * alpha / 255) for c in col),
            width=int(random.uniform(2, 9)))
streaks = streaks.filter(ImageFilter.GaussianBlur(5))
img = ImageChops.add(img, streaks)

# ---------- 5. 高频：bokeh 光斑 + 细碎星点 ----------
bokeh = Image.new("RGB", (W, H), (0, 0, 0))
db = ImageDraw.Draw(bokeh)
for _ in range(130):
    x, y = random.uniform(0, W), random.uniform(0, H)
    r = random.uniform(2.5, 14)
    a = random.uniform(0.20, 0.78)
    col = random.choice([(190, 222, 255), (255, 236, 200), (170, 210, 255)])
    db.ellipse([x - r, y - r, x + r, y + r], fill=tuple(int(c * a) for c in col))
for _ in range(900):
    x, y = random.uniform(0, W), random.uniform(0, H)
    r = random.uniform(0.6, 1.9)
    col = random.choice([(200, 226, 255), (255, 244, 214)])
    db.ellipse([x - r, y - r, x + r, y + r], fill=tuple(int(c * 0.5) for c in col))
bokeh = bokeh.filter(ImageFilter.GaussianBlur(1.4))
img = ImageChops.add(img, bokeh)

# ---------- 6. 收尾 ----------
img = img.filter(ImageFilter.GaussianBlur(0.5))
img.save(OUT, quality=84, optimize=True, progressive=True)
print("saved", OUT, img.size)

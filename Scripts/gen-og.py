#!/usr/bin/env python3
"""生成社交 / AI 摘要用的 1200×630 封面图 docs/images/og-cover.png。

用 Pillow 原生绘制（不依赖 ImageMagick），配色与介绍页一致：
深空底 + 蓝紫青光晕 + 应用窗口截图。可重复运行。
"""

from __future__ import annotations

import pathlib

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "images" / "og-cover.png"
SHOT = ROOT / "docs" / "images" / "app-window.jpg"

W, H = 1200, 630
SANS = "/System/Library/Fonts/Hiragino Sans GB.ttc"
LATIN_BOLD = "/System/Library/Fonts/Supplemental/Arial Bold.ttf"
LATIN = "/System/Library/Fonts/Supplemental/Arial.ttf"


def font(path: str, size: int, index: int = 0) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(path, size, index=index)


def glow(canvas: Image.Image, cx: int, cy: int, r: int, color: tuple[int, int, int], alpha: int) -> None:
    """在 canvas 上叠一团柔光（径向渐变）。"""
    layer = Image.new("RGBA", (r * 2, r * 2), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    steps = 48
    for i in range(steps, 0, -1):
        t = i / steps
        rr = int(r * t)
        a = int(alpha * (1 - t) ** 2)
        d.ellipse((r - rr, r - rr, r + rr, r + rr), fill=(*color, a))
    layer = layer.filter(ImageFilter.GaussianBlur(r * 0.06))
    canvas.alpha_composite(layer, (cx - r, cy - r))


def main() -> None:
    # ---- 背景：上深下更深的竖向渐变
    base = Image.new("RGB", (W, H), (5, 8, 15))
    d = ImageDraw.Draw(base)
    for y in range(H):
        t = y / H
        c = (
            int(12 * (1 - t) + 5 * t),
            int(22 * (1 - t) + 8 * t),
            int(38 * (1 - t) + 15 * t),
        )
        d.line([(0, y), (W, y)], fill=c)

    canvas = base.convert("RGBA")
    glow(canvas, -60, -80, 520, (58, 104, 194), 190)
    glow(canvas, 1180, -120, 460, (128, 66, 172), 165)
    glow(canvas, 980, 700, 420, (22, 120, 114), 120)

    # ---- 右侧：应用窗口截图
    shot = Image.open(SHOT).convert("RGB")
    target_w = 560
    shot = shot.resize((target_w, round(shot.height * target_w / shot.width)), Image.LANCZOS)
    sw, sh = shot.size
    sx, sy = 620, (H - sh) // 2

    shadow = Image.new("RGBA", (sw + 120, sh + 120), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        (60, 60, 60 + sw, 60 + sh), radius=22, fill=(0, 0, 0, 165)
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(26))
    canvas.alpha_composite(shadow, (sx - 60, sy - 46))

    mask = Image.new("L", (sw, sh), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, sw - 1, sh - 1), radius=16, fill=255)
    canvas.paste(shot, (sx, sy), mask)

    # 窗口描边，避免暗色截图在暗背景上糊成一片
    ImageDraw.Draw(canvas).rounded_rectangle(
        (sx, sy, sx + sw - 1, sy + sh - 1), radius=16, outline=(120, 170, 225, 90), width=2
    )

    draw = ImageDraw.Draw(canvas)

    # ---- 左侧文案
    x = 76
    draw.text((x, 142), "M I N E C R A F T   L A U N C H E R", font=font(LATIN_BOLD, 17), fill=(122, 165, 208))

    draw.text((x - 4, 168), "GlassMC", font=font(LATIN_BOLD, 92), fill=(240, 248, 255))

    draw.text((x, 292), "液态玻璃 Minecraft 启动器", font=font(SANS, 38, index=1), fill=(207, 228, 248))

    lines = [
        "393 个版本 · Fabric / NeoForge · Modrinth",
        "macOS · Windows · Linux，同一套代码",
        "GPLv3 开源 · Rust + Tauri 2",
    ]
    y = 366
    for line in lines:
        draw.text((x, y), line, font=font(SANS, 21), fill=(138, 163, 191))
        y += 40

    # 底部域名 + 一条渐隐细线
    for i in range(300):
        a = int(70 * (1 - i / 300))
        draw.line([(x + i, 546), (x + i, 546)], fill=(95, 159, 214, a))
    draw.text((x, 562), "glassmc.shuan.uk", font=font(LATIN, 20), fill=(110, 145, 180))

    OUT.parent.mkdir(parents=True, exist_ok=True)
    canvas.convert("RGB").save(OUT, "PNG", optimize=True)
    print("已生成", OUT, OUT.stat().st_size // 1024, "KB")


if __name__ == "__main__":
    main()

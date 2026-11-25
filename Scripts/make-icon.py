#!/usr/bin/env python3
"""从一张 1024×1024 的源图生成 GlassMC 各平台图标。

用法：
    python3 Scripts/make-icon.py /path/to/source.png

源图要求：正方形，主体是带圆角的底板 + 透明/纯色背景。脚本会裁到
底板范围、套一层 macOS 风格的圆角遮罩，再输出：
  src-tauri/icons/icon.icns / icon.ico / icon.png 及各尺寸 png
  docs/images/icon-256.png / icon-512.png
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "src-tauri" / "icons"
DOCS = ROOT / "docs" / "images"

# 源图里深色底板的范围；换图时按实际主体边界调整这两组数字
CROP = (208, 208, 816, 816)
BODY = 824  # macOS 图标主体占画布的 80.5%
CANVAS = 1024
RADIUS_RATIO = 0.207  # 底板圆角占主体宽度的比例


def render_master(src: Image.Image) -> Image.Image:
    src = src.convert("RGBA")
    body = src.crop(CROP).resize((BODY, BODY), Image.LANCZOS)
    # 4 倍超采样画圆角遮罩，避免锯齿；再轻微羽化，贴合系统图标的视觉边缘
    ss = 4
    size = BODY * ss
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    radius = int(size * RADIUS_RATIO)
    draw.rounded_rectangle([0, 0, size - 1, size - 1], radius=radius, fill=255)
    mask = mask.resize((BODY, BODY), Image.LANCZOS).filter(ImageFilter.GaussianBlur(0.5))
    body.putalpha(mask)
    canvas = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    canvas.alpha_composite(body, ((CANVAS - BODY) // 2, (CANVAS - BODY) // 2))
    return canvas


def save_png(master: Image.Image, path: Path, size: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    master.resize((size, size), Image.LANCZOS).save(path, "PNG", optimize=True)


def make_icns(master: Image.Image) -> None:
    iconset = Path(tempfile.mkdtemp(prefix="glassmc-icon-")) / "GlassMC.iconset"
    iconset.mkdir()
    for size, name in [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ]:
        master.resize((size, size), Image.LANCZOS).save(iconset / name, "PNG")
    subprocess.run(
        ["iconutil", "-c", "icns", str(iconset), "-o", str(ICONS / "icon.icns")],
        check=True,
    )


def make_ico(master: Image.Image) -> None:
    master.resize((256, 256), Image.LANCZOS).save(
        ICONS / "icon.ico",
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )


def main() -> None:
    if len(sys.argv) < 2:
        sys.exit("用法：python3 Scripts/make-icon.py <1024x1024 源图>")
    source = Path(sys.argv[1]).expanduser().resolve()
    if not source.is_file():
        sys.exit("找不到源图：" + str(source))
    master = render_master(Image.open(source))

    ICONS.mkdir(parents=True, exist_ok=True)
    master.save(ICONS / "icon.png", "PNG")
    save_png(master, ICONS / "32x32.png", 32)
    save_png(master, ICONS / "128x128.png", 128)
    save_png(master, ICONS / "128x128@2x.png", 256)
    save_png(master, ICONS / "Square44x44Logo.png", 44)
    save_png(master, ICONS / "Square150x150Logo.png", 150)
    save_png(master, ICONS / "StoreLogo.png", 50)
    make_ico(master)
    make_icns(master)
    save_png(master, DOCS / "icon-256.png", 256)
    save_png(master, DOCS / "icon-512.png", 512)

    print("已生成：")
    print("  src-tauri/icons/icon.icns、icon.ico、icon.png 及各尺寸")
    print("  docs/images/icon-256.png、icon-512.png")


if __name__ == "__main__":
    main()

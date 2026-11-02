# dmgbuild 配置：生成带安装指引背景图与图标布局的 DMG。
# 直接写 .DS_Store，不依赖 Finder GUI 自动化（适配无头 / 沙箱环境）。
# 注：dmgbuild 以自定义命名空间 exec 本文件，__file__ 不可用；路径以项目根目录为基准。

APP = defines.get("app", "build/GlassMC.app")
BG = "Scripts/dmg_background.png"

title = "GlassMC"
volume_name = "GlassMC"
format = "UDZO"
compression_level = 9

files = [APP]
symlinks = {"Applications": "/Applications"}

background = BG

# 窗口与图标布局（窗口宽 658，图标必须落在窗口内）
window_rect = ((400, 200), (658, 498))
icon_size = 110
text_size = 14
icon_locations = {
    "GlassMC.app": (175, 170),
    "Applications": (483, 170),
}

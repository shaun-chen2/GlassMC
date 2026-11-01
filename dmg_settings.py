# dmgbuild 配置文件：生成带安装指引背景图与图标布局的 DMG。
# 直接写 .DS_Store，不依赖 Finder GUI 自动化（适配无头/沙箱环境）。
# 注：dmgbuild 以自定义命名空间 exec 本文件，__file__ 不可用；
# 此处使用相对路径（build.sh 已切到项目根目录）。

APP = "build/GlassMC.app"
BG = "Assets/dmg_background.png"

# 输出与卷名（也可在命令行覆盖）
title = "GlassMC"
volume_name = "GlassMC"
format = "UDZO"
compression_level = 9

# 放入 DMG 的内容
files = [APP]
symlinks = {"Applications": "/Applications"}

# 安装指引背景图（PNG）
background = BG

# 窗口与图标布局（窗口宽 658，图标须落在窗口内）
window_rect = ((400, 200), (658, 498))
icon_size = 110
text_size = 14
icon_locations = {
    "GlassMC.app": (175, 170),
    "Applications": (483, 170),
}

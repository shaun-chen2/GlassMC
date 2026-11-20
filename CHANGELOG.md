# 更新日志

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [未发布]

### 计划中

- 界面多语言（i18n）：把前端中文文案抽成资源文件，接受社区翻译
- 设置页加「打开数据目录 / 复制启动日志」，方便反馈问题时取日志
- 更多平台的实测记录（Windows 真机、GNOME / KWin 下的玻璃效果）

## [1.3.3] - 2026-11-20

### 新增

- **版本列表分页**：后端不再把清单截断到 600 条，前端每次渲染 200 行、底部「显示更多」继续加。
  以前勾选「含快照」时 1.7.10 / 1.8.9 这些老版本会被截断，只能靠搜索找到。
- **Modrinth 依赖补装**：按 `required` 依赖递归补装（深度 4 层、32 个节点上限、环检测），
  可选依赖由「同时安装可选依赖」开关控制。已装项目记在 `.glassmc-modrinth.json`，重复安装直接跳过；
  已装项目的依赖仍会重新检查，手动删了前置也能补回来。
- **更新提示**：启动静默查 GitHub Releases（24 小时缓存、按版本号失效），设置页显示新版本号、
  安装包大小与「下载并打开」，可忽略此版本。
- **CurseForge 整合包导入**：支持含 `manifest.json` 的 `.zip`，免 API key 查文件名后从 CDN 下载；
  预览与「补装游戏本体 / 加载器」流程复用 `.mrpack` 那一套。

### 修复

- 补齐 `opener:allow-open-path` 权限，首页「打开目录」与模组页「打开 mods」不再被拒。
- `extract_prefix` 补 Zip Slip 防护：绝对路径、反斜杠、`..`、符号链接一律不落盘（`.mrpack` 一并受益）。
- 整合包识别失败时透出后端真实错误，不再只报「无法识别」。
- `content-visibility` 收窄到版本列表，避免其它列表滚动时内容延迟绘制。

### 其他

- `cargo fmt` / `clippy --all-targets -- -D warnings` 全绿，无行为变化。

## [1.3.2] - 2026-11-05

### 修复

- Windows 窗口是白的，透不出桌面：
  - 背景材质按系统主题取色，系统是浅色时 Acrylic / Mica 会铺一层近白的底。
    改成强制深色（`DWMWA_USE_IMMERSIVE_DARK_MODE`），并优先用 Acrylic ——
    它是实时模糊窗口后面的桌面，Mica 只拿壁纸配色。
  - 无边框窗口为了能拖拽改大小保留了 `WS_THICKFRAME`，DWM 会沿窗口描一条 1px 灰边。
    用 `DWMWA_BORDER_COLOR = NONE` 关掉，拖拽热区不变。
  - 圆角改为完全交给 DWM 裁：外壳不再自己画一条 CSS 弧线，内容铺到直角，由系统裁出圆角。
    自己画的话两条弧半径总差几像素（DPI 缩放、抗锯齿），角上会留一条亮边。
    Win10 及更早不认这个属性，退回自己画 14px。
- 「运行环境」卡片里的窗口玻璃档位以前是写死的，现在显示启动时实际申请到的结果。

## [1.3.1] - 2026-11-04

### 修复

- 安装页每个版本号下面的类型徽章是空的，只剩一个灰点。
  后端 `serde(rename = "type")` 双向生效，序列化给前端时字段名也变成了 `type`，
  而前端读的是 `kind`。改成只对反序列化重命名。
- 设置页「关掉窗口时退出应用」的开关比标题低半个行高，因为开关挂在说明文字那一行里。
  去掉说明行，改成标题与开关同一行的 flex 布局。

### 变更

- 介绍页精简：8 个长节压到 5 节，功能卡 12 张合并成 6 张
- 介绍页安装教程补齐 Windows 与 Linux：SmartScreen 放行、WebView2 说明、
  `apt install ./GlassMC.deb`、绿色版解压运行、补 WebKitGTK 依赖
- 代码统一 `cargo fmt` 格式，clippy 告警清零

## [1.3.0] - 2026-11-02

### 新增

- **重写为 Rust + Tauri 2**，同一套代码出 macOS / Windows / Linux 三个平台
- 原生毛玻璃：macOS 走 `NSVisualEffectView`，Windows 走 Mica / Acrylic，
  Linux 走合成器；不支持时自动降级为磨砂并保留圆角
- 全版本安装：正式版与快照，走 BMCLAPI 镜像，逐项校验 SHA1，已下载的跳过
- 加载器：Fabric / Quilt / Forge / NeoForge，装完自动成为独立实例
- Java 托管：按版本自动选 8 / 11 / 17 / 21，从清华镜像下载
- 三种登录：离线、微软正版、Yggdrasil 外置登录（内置 LittleSkin / Ely.by / MiniSkins）
- 皮肤：3D 头部预览、在线皮肤库浏览与套用、外置登录皮肤同步（authlib-injector）
- 模组：本地模组开关与删除、缺失依赖提示、Modrinth 搜索安装
- 整合包：`.mrpack` 导入与导出
- 导入：自动扫描 HMCL、官方启动器、Prism / MultiMC 并复制版本与存档
- 陶瓦联机：生成邀请码与陶瓦联机互通

### 说明

- 发布包没有 Apple / Microsoft 代码签名，首次打开需要手动放行一次

## [1.1] - 2026-10-28

### 说明

- 这是 **Swift + SwiftUI** 的原生 macOS 版本，只支持 macOS。
  代码已从 main 移除并打 tag [`swift-v1.1`](https://github.com/shaun-chen2/GlassMC/tree/swift-v1.1) 归档，
  不再维护。1.3 之后的所有功能都在 Rust 版上继续。

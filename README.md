# GlassMC

**一个把「装游戏」这件事藏起来的 Minecraft 启动器。**
界面是液态玻璃——窗口本身向系统申请原生毛玻璃，不是网页里堆出来的假透明。

[![Release](https://img.shields.io/github/v/release/shaun-chen2/GlassMC?style=flat-square&label=release)](https://github.com/shaun-chen2/GlassMC/releases/latest)
[![CI](https://github.com/shaun-chen2/GlassMC/actions/workflows/ci.yml/badge.svg)](https://github.com/shaun-chen2/GlassMC/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-GPLv3-2f7cf6?style=flat-square)](LICENSE)
[![Downloads](https://img.shields.io/github/downloads/shaun-chen2/GlassMC/total?style=flat-square&color=2f7cf6)](https://github.com/shaun-chen2/GlassMC/releases)
[![Issues](https://img.shields.io/github/issues/shaun-chen2/GlassMC?style=flat-square)](https://github.com/shaun-chen2/GlassMC/issues)
[![PRs welcome](https://img.shields.io/badge/PRs-welcome-7fd3a0?style=flat-square)](CONTRIBUTING.md)

[![macOS](https://img.shields.io/badge/macOS-下载%20dmg-2f7cf6?style=for-the-badge&logo=apple&logoColor=white)](https://github.com/shaun-chen2/GlassMC/releases/latest/download/GlassMC.dmg)
[![Windows](https://img.shields.io/badge/Windows-下载%20exe-2f7cf6?style=for-the-badge&logo=windows&logoColor=white)](https://github.com/shaun-chen2/GlassMC/releases/latest/download/GlassMC-Setup.exe)
[![Linux](https://img.shields.io/badge/Linux-下载%20deb-2f7cf6?style=for-the-badge&logo=linux&logoColor=white)](https://github.com/shaun-chen2/GlassMC/releases/latest/download/GlassMC.deb)
[![介绍页](https://img.shields.io/badge/%E4%BB%8B%E7%BB%8D%E9%A1%B5-GitHub.io-111?style=for-the-badge)](https://shaun-chen2.github.io/GlassMC/)

![GlassMC 主界面](docs/images/app-window.jpg)

选一个版本点安装，剩下的它自己来：下载游戏文件、配 Java、装加载器、拉模组。
装完直接点启动，中间不用碰任何配置文件。

| 390+ | 4 | 3 | GPLv3 |
|:---:|:---:|:---:|:---:|
| 可装版本 | 模组加载器 | 操作系统 | 开源许可 |

---

## 下载

| 平台 | 要求 | 文件 |
|---|---|---|
| **macOS** | 15 Sequoia 或更新，Apple Silicon / Intel 通用 | [`GlassMC.dmg`](https://github.com/shaun-chen2/GlassMC/releases/latest/download/GlassMC.dmg) · 约 9 MB |
| **Windows** | 10 1809+ / 11，64 位 | [`GlassMC-Setup.exe`](https://github.com/shaun-chen2/GlassMC/releases/latest/download/GlassMC-Setup.exe) · 约 3 MB |
| **Linux** | 需要 WebKitGTK 4.1 | [`GlassMC.deb`](https://github.com/shaun-chen2/GlassMC/releases/latest/download/GlassMC.deb) · 约 3.4 MB<br>[`GlassMC-linux.tar.gz`](https://github.com/shaun-chen2/GlassMC/releases/latest/download/GlassMC-linux.tar.gz) · 约 4.6 MB（绿色版） |

> **第一次打开会被系统拦下来**，提示「无法验证开发者」「已损坏」或 SmartScreen 警告。
> 这不是文件坏了，是发布包没有代码签名证书的正常现象。
> 三个平台各自的放行方法见[安装说明](https://shaun-chen2.github.io/GlassMC/#install)，只需操作一次。

Release 页面附有每个文件的 SHA-256，下载后可以核对。

---

## 它能做什么

### 装游戏：从零到能进服

- **全版本安装**：正式版和快照都在列表里，共 390 多个。游戏文件走 BMCLAPI 国内镜像，
  镜像没有的自动回退官方源。已下载过的文件会跳过，中断了再点一次就补齐。
- **加载器一键装**：Fabric、Quilt、NeoForge、Forge 四个都支持，自动匹配你选的那个游戏版本。
- **Java 不用自己装**：JDK 8 / 11 / 17 / 21 从清华镜像按需拉，系统里已有的 JDK 也会被自动认出来。
  Apple Silicon 上装 JDK 8 会自动选 x64 版本（Rosetta 能跑），不会给你一个装不上的包。
- **版本隔离**：以 HMCL 那种「版本单独一个目录」的结构装出来的实例，会被识别成隔离实例，
  存档和模组各用各的，不会互相串。

### 账号：三种登录

- **离线账号**：随便起个名字就能玩，UUID 按 Mojang 的离线算法算。
- **微软正版**：设备码登录，弹一个网页让你输码，不用在本程序里填密码。
  登录后能进正版服，皮肤跟着账号走。
- **外置登录（Yggdrasil）**：LittleSkin、Ely.by、MiniSkins 已内置，也能填自建皮肤站。
  认证路径会自动探测，填 `https://xxx/api/yggdrasil` 这种基址就行。

### 皮肤：登录了就能看见

- 皮肤站换的皮肤会自动同步进游戏（启动时挂 authlib-injector，日志里能看到
  `Authentication server: 你的皮肤站`）。
- 内置皮肤库：连 Blessing Skin 站点的皮肤列表，点一张直接套到当前账号上，不用手动换文件。
- 账号头像用 canvas 现场画等距 3D 头，不消耗网络请求。

### 模组：搜索、装、停、删

- **本地模组**：读 jar 里的 `neoforge.mods.toml` / `mods.toml` / `fabric.mod.json`，
  显示的是模组真名和版本号，不是文件名。启停就是改后缀，删除会进 `mods/.trash`（可反悔）。
- **Modrinth**：内置搜索，自动按当前实例的游戏版本和加载器过滤，装之前校验 SHA1。
- **整合包**：`.mrpack` 拖进来就能装（overrides 和文件都落到版本隔离目录）；
  也能把自己的实例导出来分享，依赖从合并后的 libraries 反推。

### 联机：一串邀请码

用的是**陶瓦联机（Terracotta）协议**，不需要公网 IP、不用配端口映射。

1. 房主在游戏里「对局域网开放」，回启动器「联机」页点创建房间；
2. 生成一个形如 `U/6DRG-BECK-V92T-F9ZZ` 的邀请码，发给朋友；
3. 朋友粘贴进来，游戏「多人游戏」列表里会多出「陶瓦联机大厅」，双击进。

和用官方陶瓦客户端的朋友可以直接互加，反过来也一样。协议组件首次使用自动下载（约 12 MB）。

### 从别的启动器搬家

自动扫描 **HMCL**、**官方启动器**、**Prism / MultiMC** 的目录，选一个点导入：
版本、模组、存档、资源包、设置都复制过来，原目录不动。
HMCL 配置里存着的登录令牌也会顺手带过来。

### 其它

- **内存**：滑块给，单位 GB。整合包可以给到 8~12 GB。
- **启动参数**：自定义 JVM 参数、游戏窗口尺寸，改完立即生效。
- **实时日志**：启动时窗口里滚动显示游戏输出，同时落盘到 `logs/game-latest.log`。
- **界面**：玻璃不透明度可调（0.35 ~ 1.0），跟随系统深浅色，可切纯深 / 纯浅。

---

## 安装

**macOS**：双击 dmg，把 GlassMC 拖进「应用程序」。第一次打开被拦时，
去 **系统设置 → 隐私与安全性** 点「仍要打开」；或者终端执行
`xattr -dr com.apple.quarantine /Applications/GlassMC.app`。

**Windows**：双击 exe，若出现蓝色「Windows 已保护你的电脑」，
点 **更多信息 → 仍要运行**。装完从开始菜单启动。

**Linux**：`sudo apt install ./GlassMC.deb`，装完菜单里就有（`./` 不能省）。
Fedora / Arch 用绿色版：`tar xzf GlassMC-linux.tar.gz && ./glassmc`。

三个平台的详细步骤、缺依赖怎么办、白窗口怎么办，都在
[介绍页的安装一节](https://shaun-chen2.github.io/GlassMC/#install)。

数据目录：macOS `~/Library/Application Support/GlassMC`、Windows `%APPDATA%\GlassMC`、
Linux `~/.local/share/GlassMC`。整个目录拷走就是完整备份，卸载时删掉即可。

---

## 从源码跑

```bash
cd src-tauri
cargo run              # 编译并启动；前端没有构建步骤，改完界面重跑即可
```

```bash
bash build.sh                          # macOS：build/GlassMC.app + GlassMC.dmg
bash Scripts/make-linux-package.sh     # Linux：deb + tar.gz
```

只想调界面不想等编译，可以用浏览器的 mock 模式：

```bash
python3 -m http.server 8080 --directory src
# 打开 http://localhost:8080
```

完整的三端构建步骤、交叉编译、打包脚本说明见 [开发说明](docs/development.md)，
模块划分与启动链见 [架构说明](docs/architecture.md)。

---

## 参与贡献

这个项目欢迎任何人参与，包括第一次给开源项目提 PR 的人。

| 你能做的 | 从哪开始 |
|---|---|
| 报一个能复现的 bug | [新建 issue](https://github.com/shaun-chen2/GlassMC/issues/new/choose)——按表单填，日志比截图有用 |
| 认领一个小任务练手 | [`good first issue`](https://github.com/shaun-chen2/GlassMC/labels/good%20first%20issue) |
| 在真机上验证 | [`help wanted`](https://github.com/shaun-chen2/GlassMC/labels/help%20wanted)：Windows 真机、各种桌面环境都缺人 |
| 改代码、修文档、翻译 | [CONTRIBUTING.md](CONTRIBUTING.md) 里有本地开发、代码约定和 PR 流程 |
| 提问、聊想法 | [Discussions](https://github.com/shaun-chen2/GlassMC/discussions) |

提 PR 之前请跑一遍 `cargo fmt --all --check` 和 `cargo clippy --all-targets -- -D warnings`，
CI 会检查同样的东西，外加三端编译。

参与本项目即表示你同意遵守[贡献者行为准则](CODE_OF_CONDUCT.md)。
安全问题请走[私下报告](SECURITY.md)，不要开公开 issue。

---

## 项目状态

三端都由 CI 编译验证，但**运行验证的程度不一样**，这里如实写清楚：

- **macOS**：在本机跑过完整流程——装机、装版本、装加载器、启动进主菜单。
  窗口毛玻璃是真实的 `NSVisualEffectView`。
- **Windows**：交叉编译产物，在 wine 里走完了「静默安装 → 从 `Program Files` 启动 → 卸载无残留」，
  能执行到读配置、找 Java。**真实 Windows 上的界面还没人看过**，最可能出问题的是
  Mica / Acrylic 的版本差异和进程管理。欢迎有 Windows 机器的朋友帮忙验证。
- **Linux**：在 Ubuntu 26.04 上原生编译，并在虚拟显示里跑完整条链路：
  八个页面正常渲染、真实下载 1.21.11（62 个文件 + 4591 个资源对象，0 失败）、
  启动后进到主菜单。模糊取决于桌面合成器，测试环境没有合成器，所以实际是磨砂效果。

---

## 常见问题

**Q：提示「已损坏」是不是下载坏了？**
不是。是没有开发者签名。按[安装说明](https://shaun-chen2.github.io/GlassMC/#install)放行一次即可，不用重新下载。

**Q：启动游戏报 Java 相关的错？**
去「安装 → Java 环境」看有没有可用运行时。没有就点安装，选 17 或 21
（1.17 及更新版本用 17/21，1.16 及更早用 8）。从别处导入的实例也可以指定实例自带的 Java。

**Q：内存该给多少？**
8 GB 机器给 4 GB，16 GB 给 6~8 GB，大型整合包 8~12 GB。别把全部内存都给游戏，系统会卡。

**Q：皮肤站登录了，游戏里还是默认皮肤？**
确认启动的实例用的是那个皮肤站账号。看日志里有没有
`Authentication server: 你的皮肤站地址`；没有的话重启一次启动器再启动游戏。

**Q：加了模组进不去游戏？**
先看日志，最常见是缺前置模组或版本区间不对。模组页会标出缺失依赖，也可以逐个禁用排查。

**Q：以前 HMCL 的整合包和存档能直接用吗？**
能。「导入」页会自动扫描 HMCL / 官方 / Prism 的目录，导入时复制过来，原目录不受影响。

**Q：和上一代原生版是什么关系？**
上一代是纯 SwiftUI 写的，只能在 macOS 跑。这一代换成 Tauri 2（Rust + Web 前端），
界面和功能重做了一遍，为的是能跑三个平台。两代**共用同一个数据目录**，
已装的版本、账号、设置都直接沿用。Swift 源码保留在
[`swift-v1.1`](https://github.com/shaun-chen2/GlassMC/tree/swift-v1.1) 标签里。

---

## 项目结构

```
src/                前端：一个 index.html + 几个 js/css，没有构建步骤
src-tauri/src/      Rust 后端：23 个模块，一个模块一件事
Scripts/            打包脚本（NSIS、deb、DMG 配置、图标生成）
docs/               介绍页与文档
```

细节在 [架构说明](docs/architecture.md)。

## 致谢

界面与功能的设计大量参考了 [HMCL](https://github.com/HMCL-dev/HMCL) 和
[PCL2](https://github.com/Hex-Dragon/PCL2) 的使用习惯；联机协议来自
[陶瓦联机 Terracotta](https://github.com/burningtnt/Terracotta)；外置登录注入用
[authlib-injector](https://github.com/yushijinhun/authlib-injector)。

用到的公共服务：

- 游戏文件：BMCLAPI 镜像（HMCL 默认国内源）
- Java 运行时：Adoptium，走清华大学开源软件镜像站
- 模组数据：Modrinth API

## 许可

**GPLv3**。本项目与 Mojang / Microsoft 无关联，连接正版服务器需要自备正版账号。
Minecraft 是 Mojang Studios 的商标。


# GlassMC

**一个把「装游戏」这件事藏起来的 Minecraft 启动器。**
界面是液态玻璃——窗口本身申请系统的原生毛玻璃，不是网页里堆出来的假透明。

[![下载](https://img.shields.io/badge/下载-GlassMC.dmg-2f7cf6?style=for-the-badge&logo=apple&logoColor=white)](https://github.com/shaun-chen2/GlassMC/releases/latest/download/GlassMC.dmg)
[![介绍页](https://img.shields.io/badge/介绍页-GitHub.io-111?style=for-the-badge)](https://shaun-chen2.github.io/GlassMC/)
[![许可](https://img.shields.io/badge/许可-GPLv3-2f7cf6?style=for-the-badge)](LICENSE)

![GlassMC 主界面](docs/images/app-window.jpg)

选一个版本点安装，剩下的它自己来：下载游戏文件、配 Java、装加载器、拉模组。
装完直接点启动，中间不用碰任何配置文件。

---

## 下载

| 平台 | 状态 | 文件 |
|---|---|---|
| **macOS**（Apple Silicon / Intel 通用） | 已验证 | [`GlassMC.dmg`](GlassMC.dmg) 约 10 MB |
| **Windows** 10 / 11 | 代码已完成，**未实机验证** | 需自行编译，见 [开发说明](docs/development.md) |
| **Linux**（WebKitGTK） | 代码已完成，**未实机验证** | 需自行编译，见 [开发说明](docs/development.md) |

macOS 要求 **15 Sequoia 或更新**。装法：双击 dmg，把 GlassMC 拖进「应用程序」。

> **第一次打开会被系统拦下来**，提示「无法验证开发者」或「已损坏」——
> 这不是文件坏了，是没有 Apple 开发者签名的正常现象。处理方法见下面
> [打不开怎么办](#打不开怎么办)。

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

## 打不开怎么办

因为没买 Apple 开发者证书（一年 99 美元），应用只有 ad-hoc 签名。第一次打开会被拦，
**放行一次之后就不会再问**。两种方法选一个：

### 方法一：系统设置里点「仍要打开」（推荐）

1. 双击 GlassMC，弹出「无法打开，因为 Apple 无法验证…」时点 **完成**；
2. 打开 **系统设置 → 隐私与安全性**，向下滚到「安全性」区域；
3. 会看到 `"GlassMC" 已被阻止使用，因为它不是来自已识别的开发者`，点右边的 **仍要打开**，输开机密码；
4. 再双击一次 GlassMC，弹窗里会出现 **打开** 按钮，点它。

> 如果提示的是 **「GlassMC 已损坏，应该将它移到废纸篓」**，处理方式完全一样：
> 先去隐私与安全性点「仍要打开」放行，再打开应用。**不要点「移到废纸篓」。**

### 方法二：终端一行命令

```bash
xattr -dr com.apple.quarantine /Applications/GlassMC.app
```

这条命令只是去掉 macOS 给下载文件打的「待检查」标记，不改应用内容。
没装在「应用程序」文件夹的话，把路径换成实际位置。

---

## 系统要求

| 项目 | 要求 |
|---|---|
| 芯片 | macOS：Apple Silicon 或 Intel 都行（universal 二进制） |
| 系统 | macOS 15 Sequoia 或更新；Windows 10 1809+；主流 Linux 发行版 |
| 磁盘 | 每个游戏版本约 500 MB ~ 1 GB；整合包另算 |
| Java | 不用自己装，启动器按版本要求自动配 |
| 内存 | 游戏给 4 GB 起，大整合包建议 8~12 GB |

---

## 常见问题

**Q：提示「已损坏」是不是下载坏了？**
不是。是没有开发者签名。按上面「打不开怎么办」放行一次即可，不用重新下载。

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

**Q：Windows 和 Linux 真的能用吗？**
代码路径都写好了（数据目录、毛玻璃档位、进程托管、路径分隔符按平台分支），
但开发者手上只有 macOS，**没有实机验证过**。愿意试的话欢迎反馈 issue，
最可能出问题的地方是 Windows 的毛玻璃 API 版本差异和 Linux 的窗口管理器兼容性。

**Q：和上一代原生版是什么关系？**
上一代是纯 SwiftUI 写的，只能在 macOS 跑。这一代换成 Tauri 2（Rust + Web 前端），
界面和功能重做了一遍，为的是能跑三个平台。两代**共用同一个数据目录**，
已装的版本、账号、设置都直接沿用，可以同时装着互不影响。
Swift 源码保留在 [`swift-v1.1`](https://github.com/shaun-chen2/GlassMC/tree/swift-v1.1) 标签里。

---

## 从源码跑

```bash
cd src-tauri
cargo run --release
```

打包 macOS 应用与 dmg：

```bash
bash build.sh
```

Windows / Linux 的依赖与构建方式、目录结构、实现细节，都在 [开发说明](docs/development.md)。

---

## 许可与致谢

**GPLv3**。用到的公共服务与项目：

- 游戏文件：BMCLAPI 镜像（HMCL 默认国内源）
- Java 运行时：Adoptium，走清华大学开源软件镜像站
- 模组数据：Modrinth API
- 外置登录注入：authlib-injector
- 联机协议：[陶瓦联机 Terracotta](https://github.com/burningtnt/Terracotta)

本项目与 Mojang / Microsoft 无关联。连接正版服务器需要自备正版账号。
Minecraft 是 Mojang Studios 的商标。

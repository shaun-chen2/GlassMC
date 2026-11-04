# 开发说明

面向想改代码或自己编译的人。只想用的话看 [README](../README.md) 就够了。

---

## 技术栈

Tauri 2：**Rust 管系统层，Web 管界面**。

启动器最脏最重的活（拼上千项 JVM 参数、解压 natives、fork 进程、扫几百个 jar、算 SHA1）
本来就在系统层，Rust 干这些比任何前端框架都顺手；界面用 Web 是因为它是唯一能在
macOS / Windows / Linux 上渲染出同一套像素的东西。

玻璃的实现跟着换了思路：

| | 上一代（SwiftUI） | 本代（Tauri） |
|---|---|---|
| 窗口玻璃 | `NSVisualEffectView` | macOS 同一个 API；Windows 用 Mica → Acrylic 回退；Linux 交给合成器 |
| 卡片玻璃 | `NSVisualEffectView` 多层 | **半透明色块，不叠滤镜**（窗口已经糊过了，再糊一次只是白烧内存） |
| 开关控件 | 原生 `Toggle` | 自研 Web Component，能力不足时自动降级但**圆角不变** |

---

## 目录

```
src/                      前端（无构建步骤，改完刷新就生效）
  index.html
  js/app.js               骨架：路由、首页、版本、设置基础字段
  js/features.js          账号 / 皮肤 / 皮肤库 / 弹层与进度
  js/features-install.js  安装（原版·加载器·Java）与模组（本地·Modrinth）
  js/features-more.js     导入、陶瓦联机、设置补充项
  styles/
    baseline.css          跨内核渲染基线（必须先于业务样式引入）
    app.css               设计系统 + 布局
  vendor/
    liquid-glass-toggle.js    液态玻璃开关（Web Component）

src-tauri/                Rust 后端
  src/
    main.rs               入口；--print-command / --install / --test-terracotta
    lib.rs                Builder、命令注册、玻璃初始化
    paths.rs              三平台数据目录
    config.rs             设置持久化（含认证服务器与皮肤库）
    manifest.rs           版本清单、本地扫描、inheritsFrom 深合并
    download.rs           通用下载：重试 / 状态码 / SHA1 / 原子落盘
    install.rs            安装原版：jar + 依赖库 + 资源对象（带进度事件）
    loader.rs             Fabric / Quilt / NeoForge / Forge
    javadl.rs             Adoptium + 清华镜像装 JDK
    java.rs               JDK 扫描与版本解析
    launch.rs             启动命令构建（核心）+ authlib-injector 注入
    game.rs               子进程、实时日志、退出通知
    account.rs            账号、离线 UUID、账号文件读写
    yggdrasil.rs          外置登录：多路径探测 + sessionserver 贴图
    microsoft.rs          微软设备码登录全链路
    skins.rs              皮肤缓存 + Blessing Skin 皮肤库
    injector.rs           authlib-injector 下载与校验
    modrinth.rs           Modrinth API + jar 元数据解析 + 本地模组
    modpack.rs            .mrpack 导入导出
    importer.rs           从其它启动器导入
    terracotta.rs         陶瓦联机（下载组件 / 守护进程 / 房间 API）
    glass.rs              三平台窗口毛玻璃
    net.rs                HTTP 客户端（带 UA / 本地禁用代理）
    commands.rs           基础命令
    commands_ext.rs       第二阶段新增的 50+ 个命令

build.sh                  编译 → lipo → 组 .app → 签名 → dmg
Scripts/                  DMG 背景图与布局配置
docs/rendering-parity.md  跨内核渲染差异的实测记录
```

---

## 跑起来

### macOS

```bash
cd src-tauri
cargo run --release
```

打包 universal 应用与 dmg：

```bash
bash build.sh
# 产物：build/GlassMC.app、build/GlassMC.dmg
```

`build.sh` 会分别编译 `aarch64-apple-darwin` 与 `x86_64-apple-darwin`，用 `lipo` 合并，
再组 `.app`、ad-hoc 签名、调 dmgbuild 出安装包。

### Windows

需要 **WebView2 运行时**（Windows 11 自带；Windows 10 需装 Evergreen Runtime）。
毛玻璃优先 Mica（Windows 11），失败退 Acrylic（Windows 10 1803+），都不行则前端降级为磨砂。

#### 在 Linux 上交叉编译（发布包就是这么来的）

不需要 Windows 机器，也不需要 Mac。`mingw-w64` 出链接器，`nasm` 给 `aws-lc-sys`
汇编用，`nsis` 是 **Linux 原生版 makensis**（不用 wine 就能打安装包）：

```bash
sudo apt install -y mingw-w64 nsis nasm
rustup target add x86_64-pc-windows-gnu

cat >> ~/.cargo/config.toml << 'EOF'
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
ar = "x86_64-w64-mingw32-ar"
EOF

cd src-tauri
cargo build --release --target x86_64-pc-windows-gnu
# 产物：target/x86_64-pc-windows-gnu/release/glassmc.exe
#      同目录还有 WebView2Loader.dll —— 必须和 exe 放一起
```

打 NSIS 安装包：

```bash
cd ..                          # 回到仓库根
D="$PWD/build/windows"; mkdir -p "$D"
cp src-tauri/target/x86_64-pc-windows-gnu/release/{glassmc.exe,WebView2Loader.dll} "$D"/

makensis -DEXE_PATH="$D/glassmc.exe" \
         -DICON_PATH="$PWD/src-tauri/icons/icon.ico" \
         -DEXTRA_DIR="$D" \
         -DOUTFILE="$D/GlassMC-Setup.exe" \
         "$PWD/Scripts/windows-installer.nsi"
```

`-DEXTRA_DIR` 让脚本把目录里的 `*.dll` 一起装进去（WebView2Loader，以及将来可能出现的
运行时 DLL）。卸载时会一并删除。

> **运行时依赖**：exe 已经静态链接了 mingw 运行时，`objdump -p` 里只有
> `KERNEL32/USER32/ole32/...` 这类系统 DLL 加 `WebView2Loader.dll`，
> 目标机器不用装 mingw、不用装 VC++ 运行库。

#### 在 Windows 上原生编译

装了 Visual Studio Build Tools（C++ 工作负载）的话直接：

```powershell
cd src-tauri
cargo build --release
# 产物：src-tauri\target\release\glassmc.exe
```

### Linux

需要 WebKitGTK 与相关开发库：

```bash
# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel

cd src-tauri && cargo build --release
# 产物：src-tauri/target/release/glassmc
```

打 deb 和绿色版：

```bash
bash Scripts/make-linux-package.sh
# 产物：build/linux/GlassMC_<版本>_amd64.deb
#      build/linux/GlassMC_<版本>_linux-amd64.tar.gz
```

脚本会写好 `.desktop`、hicolor 图标、`control`（依赖声明 `libwebkit2gtk-4.1-0` 等），
并规范权限（目录 755 / 文件 644），最后 `dpkg-deb --root-owner-group -Zxz --build`。

> Linux 没有统一的窗口模糊 API：KWin 开了 blur 才有真玻璃，GNOME 默认没有。
> 所以本版在 Linux 上一律先按「磨砂」出，能不能真糊取决于桌面环境——这也正是
> 「不适配就自动关掉玻璃、保留圆角」这条规则存在的意义。

---

## 数据目录

三平台按系统习惯放，和上一代原生版**完全一致**，所以能直接沿用已装的版本和账号：

| 平台 | 路径 |
|---|---|
| macOS | `~/Library/Application Support/GlassMC` |
| Windows | `%APPDATA%\GlassMC` |
| Linux | `~/.local/share/GlassMC` |

里面是 `minecraft/`（游戏本体）、`accounts.json`、`tauri-config.json`
（刻意与原生版的 `UserDefaults` 分开存，两边同时装着不会互相踩）、`logs/`、`terracotta/`。

---

## 几个容易踩的点

### 1. 整合包实例的客户端 jar 在父版本目录里

`新蒸程1.7` 的 json 里既没有 `jar` 字段也没有 `downloads.client`——真正的 jar 是
`versions/1.21.1-NeoForge/1.21.1-NeoForge.jar`。只看本版本会误判成「缺客户端 jar」。
`manifest::local_client_jar_id()` 会沿 `inheritsFrom` 一路找下去。

同理，`--gameDir` 要指向 `versions/<id>/`（HMCL 的版本隔离），否则会读到根目录的 mod 和存档。

### 2. 父子版本的 `--add-opens` 各写一份

不去重不会崩（JVM 允许重复），但参数会变长；而 `-p` / `-cp` / `--add-modules`
如果重复就会直接报 `MultipleArgumentsForOptionException`。所以两类去重是分开的：
单值选项保留最后一个，`--add-opens` 这类只删完全重复的。

### 3. `-cp` 已经在版本 JSON 里了

现代版本的 jvm 参数自带 `-cp ${classpath}`。要替换占位符，但**不要**再追加一个 `-cp`。

### 4. macOS 需要 `-XstartOnFirstThread`

新版 JSON 自带；旧版缺失时要补上，否则 GLFW 初始化失败。

### 5. BMCLAPI 的资源路径没有 `objects/` 段

`/assets/{hash 前两位}/{hash}` 才是对的，`/assets/objects/...` 是 404。
另外要校验 HTTP 状态码（否则 404 页面会被当文件写盘），并带 `User-Agent`。

### 6. 前端钩子的时序

`app.js` 的 `boot()` 如果用 `else boot()` 同步执行，会在后续脚本解析前就跑完，
那时 `features*.js` 还没注册渲染钩子——表现是版本页少了按钮、设置页列表空白。
现在的写法是 `setTimeout(boot, 0)` + 首次渲染后 60ms 再兜底渲染一次。

### 7. `#[serde(default)]` 用的是字段类型的默认值

不是 struct 的 `Default::default()`。给 `Config` 新增字段时，旧配置文件里会是空值，
需要在 `config::load()` 里显式补预设（`authServers` 就是这么补的）。

---

## 跨内核渲染一致性

同一份 HTML 在 WKWebView（macOS）、WebView2（Windows）、WebKitGTK（Linux）和 Chrome 里
默认会排出不同高度，根因是 `line-height: normal` 的解释不同、以及系统字体回退链不一致。
实测数据与修复方式见 [rendering-parity.md](rendering-parity.md)。

结论就三条：所有文本显式写 `line-height`；字体栈写全
`-apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif`；等宽场景写死 `Menlo`。

---

## 调试用命令行

不打开界面也能验证启动链路：

```bash
# 打印某个版本的完整启动命令（含 classpath 项数、缺失文件数、未替换占位符检查）
./target/release/glassmc --print-command "新蒸程1.7"

# 只用命令行装一个版本，打印下载 / 跳过 / 失败的文件数
./target/release/glassmc --install "1.21.4"

# 自测陶瓦联机：下载组件、拉起守护进程、查一次房间状态后退出
./target/release/glassmc --test-terracotta
```

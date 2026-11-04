# 参与 GlassMC

先说清楚一件事：这个项目不接受「帮我加个功能」式的许愿。但只要你愿意跟着把事做完，
这里对任何水平的 PR 都友好——包括第一次给开源项目提 PR 的人。

## 目录

- [能做什么](#能做什么)
- [报问题](#报问题)
- [本地跑起来](#本地跑起来)
- [改代码](#改代码)
- [提交与 PR](#提交与-pr)
- [发布流程（维护者）](#发布流程维护者)

---

## 能做什么

| 方式 | 适合谁 | 从哪开始 |
|---|---|---|
| 报一个能复现的 bug | 所有人 | [新建 issue](https://github.com/shaun-chen2/GlassMC/issues/new/choose)，按表单填 |
| 认领 `good first issue` | 想练手的 | [标签筛选](https://github.com/shaun-chen2/GlassMC/labels/good%20first%20issue) |
| 在真实设备上验证 | 有 Windows / Linux 机器的人 | [help wanted](https://github.com/shaun-chen2/GlassMC/labels/help%20wanted) |
| 改代码 / 修文档 | 会 Rust 或前端 | 直接看下面的「本地跑起来」 |
| 翻译界面 | 会用任意语言的 | 见 [界面多语言](https://github.com/shaun-chen2/GlassMC/issues) 这条 tracking issue |

不确定从哪下手，就在 [Discussions](https://github.com/shaun-chen2/GlassMC/discussions) 里问一句，
说清你会什么、想做什么，维护者会指一个具体的坑给你。

## 报问题

**能复现的 bug 是最有价值的贡献**，比「加个功能」有用得多。提之前：

1. 先搜一遍 issue，避免重复；
2. 装不上 / 打不开，先看[安装说明](https://shaun-chen2.github.io/GlassMC/#install)，九成是系统拦截；
3. 启动失败，把日志最后 100 行贴进表单——没有日志的启动问题基本无法排查。

日志在数据目录的 `logs/game-latest.log`，游戏自己的在 `minecraft/logs/latest.log`。
数据目录：macOS `~/Library/Application Support/GlassMC`、Windows `%APPDATA%\GlassMC`、
Linux `~/.local/share/GlassMC`。

## 本地跑起来

### 需要什么

- **Rust 1.85+**（`rustup` 装，跟着 `rust-toolchain` 走就行）
- **Node.js**：只用来做 JS 语法检查，不参与构建
- **系统依赖**：
  - macOS：Xcode Command Line Tools
  - Windows：Visual Studio Build Tools（C++ 桌面开发）+ WebView2（Win11 自带）
  - Linux：`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev patchelf`

### 跑起来

```bash
cd src-tauri
cargo run            # 编译并启动，第一次要几分钟
```

**先留够磁盘空间。** `src-tauri/target/` 首次全量构建要 3~5 GB，
跑 `build.sh`（arm64 与 x86_64 各编一遍）会更多。空间不足时的报错是
`ld: write() failed, errno=28 (No space left on device)` —— 看着像编译失败，
其实是磁盘写满了，`cargo clean` 腾出空间，或者把构建目录挪到大盘：

```bash
CARGO_TARGET_DIR=/path/to/big-disk/glassmc-target cargo build --release
```

前端是**没有构建步骤**的纯 HTML/JS/CSS，直接被编进二进制。改完界面要重新 `cargo run` 才生效。

只想调界面不想等 Rust 编译，可以用浏览器 mock 模式（数据是假的，不会真下载游戏）：

```bash
python3 -m http.server 8080 --directory src
# 打开 http://localhost:8080
```

### 打包

```bash
bash build.sh                          # macOS：build/GlassMC.app + GlassMC.dmg
bash Scripts/make-linux-package.sh     # Linux：deb + tar.gz（读 Cargo.toml 的版本号）
```

Windows 的安装包用 NSIS 脚本打，交叉编译和原生编译两条路都写在
[docs/development.md](docs/development.md) 里。

### 检查

提 PR 之前至少跑这两条，CI 也会跑同样的：

```bash
cd src-tauri
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

**项目目前没有自动化测试**。所以 PR 里请写清楚你是怎么手工验证的：哪台机器、点了哪里、
看到了什么。截图和日志片段都算证据。

## 改代码

先读 [docs/architecture.md](docs/architecture.md)，那里有模块划分和三条最容易踩的坑。

### 目录

```
src/                    前端，无构建步骤
  index.html            所有页面都在这一份 HTML 里
  js/app.js             骨架：路由、渲染、状态、mock
  js/features*.js       按功能追加的钩子（安装 / 模组 / 账号 / 联机 …）
  styles/app.css        设计系统（玻璃面板、按钮、列表）
src-tauri/src/          Rust 后端，一个模块一件事
  lib.rs                启动入口 + 命令注册表
  commands.rs           核心命令
  commands_ext.rs       扩展命令（安装 / Java / 账号 / 模组 / 联机）
  paths.rs              全局目录约定
Scripts/                打包脚本（NSIS、deb、DMG 配置、图标生成）
docs/                   介绍页与开发文档
```

### 约定

- **后端加一个功能**：写个 `#[tauri::command]` 函数 → 在 `lib.rs` 的 `invoke_handler` 里注册 →
  前端 `invoke('命令名', {...})`。参数名用 camelCase，Rust 侧自动转 snake_case。
- **前端加一页**：`index.html` 里加 `<section class="page" data-page="xxx">`，导航加 `data-nav`，
  逻辑写进对应 `features*.js` 的钩子里，不要往 `app.js` 里堆。
- **错误信息写给用户看**，用中文、说清下一步该做什么，不要直接 `Err(e.to_string())` 抛底层报错。
  例：`format!("读取失败：{e}")` 比 `e.to_string()` 好。
- **注释解释「为什么」**，不解释「是什么」。这个仓库里所有不显然的写法都留了原因，跟着这个习惯。
- **不轻易加依赖**。能用手写 30 行解决的，不要引一个 crate；确实要加，在 PR 里说明理由和体积影响。
- **三个平台一起想**。改路径、进程、窗口、字体之前，先想另外两个平台会怎样。

## 提交与 PR

提交信息建议带前缀，正文写「为什么」：

```
fix: 安装页版本类型徽章空白

serde 的 rename 是双向的，序列化时字段名被改回 type，前端读 kind 拿不到值。
改成只对反序列化生效，前端再兜一层。
```

- 一个 PR 只做一件事；顺手重构请单独提。
- 标题写给用户看：「修 windows 下控制台窗口乱弹」，不要写「update game.rs」。
- PR 描述里勾选你真跑过的平台，别全勾。
- 界面改动请贴前后对比截图。

维护者一般一周内会看一眼。超过一周没动静，直接 @ 一下，不要客气。

## 发布流程（维护者）

1. 改版本号：`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、
   `Scripts/windows-installer.nsi`（`PRODUCT_VERSION` 与 `PRODUCT_VERSION4`）、
   `src/js/app.js` 的 mock 版本。CI 会校验这四处是否一致。
2. 更新 `CHANGELOG.md`。
3. 打 tag 并推送：

   ```bash
   git tag v1.3.2 && git push origin v1.3.2
   ```

4. GitHub Actions 的 `Release` 工作流会自动构建三端、统一文件名、生成 SHA-256、
   建 Release 并上传。tag 与 `tauri.conf.json` 版本号不一致会直接失败。
5. 想先看构建结果不发版：在 Actions 页面手动跑 `Release`，只产出 artifact。

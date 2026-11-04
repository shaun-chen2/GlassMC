# 架构说明

写给要改代码的人。看完应该能回答三个问题：东西在哪、数据怎么流、动哪里最容易炸。

## 一句话

Tauri 2 应用：**Rust 做所有实事，前端只负责显示**。没有 Node 构建链，前端是几个普通
`.html/.js/.css` 文件，编译时被嵌进二进制里。

```
┌─────────────────────────────────────────────────────────┐
│ WebView（三端各自的内核：WKWebView / WebView2 / WebKitGTK）│
│   index.html  所有页面写在这一个文件里                     │
│   app.js      骨架：状态 S、路由、渲染、mock               │
│   features*.js 按功能挂到 __GM_HOOKS 上的钩子             │
└───────────────▲──────────────────────┬──────────────────┘
        event   │                      │ invoke('cmd', args)
   （progress / log / game-exit）      ▼
┌─────────────────────────────────────────────────────────┐
│ Rust（lib.rs 注册 60+ 个 command）                        │
│   commands.rs / commands_ext.rs   入口，全部 async         │
│   launch.rs  ← 启动链的核心                                │
│   install / java / loader / download / net                 │
│   account / yggdrasil / microsoft / skins / injector       │
│   modrinth / modpack / importer / terracotta               │
│   paths.rs  config.rs  glass.rs  game.rs                   │
└─────────────────────────────────────────────────────────┘
```

## 数据在哪

| 内容 | 位置 |
|---|---|
| 数据根目录 | macOS `~/Library/Application Support/GlassMC`，Windows `%APPDATA%\GlassMC`，Linux `~/.local/share/GlassMC` |
| 设置 | `<根>/tauri-config.json`（刻意与 Swift 版分文件，两边共存不打架） |
| 账号 | `<根>/accounts.json` |
| 游戏 | `<根>/minecraft/`（等价于官方 `.minecraft`） |
| Java | `<根>/java/<feature>/` |
| 日志 | `<根>/logs/game-latest.log`；游戏自己的在 `minecraft/logs/latest.log` |

`GLASSMC_DATA` 环境变量可以整体覆盖根目录（便携模式、测试用），见 `paths.rs`。

## 后端模块

按域分组，一个模块一件事。改之前先读文件顶部的 `//!` 注释，那里写了「为什么这么写」。

### 启动链（最核心）

| 模块 | 职责 |
|---|---|
| `manifest.rs` | 远程版本清单、本地已装扫描、`inheritsFrom` 深合并 |
| `launch.rs` | 把版本 JSON 变成一条完整的 java 命令行 |
| `game.rs` | 进程启动、实时日志、退出通知、强杀 |
| `java.rs` / `javadl.rs` | 检测已装 JDK / 从清华镜像装 Adoptium |

### 安装

| 模块 | 职责 |
|---|---|
| `install.rs` | 客户端 jar → 依赖库 → 资源对象，逐项校验 |
| `loader.rs` | Fabric / Quilt 落 profile，NeoForge / Forge 跑官方安装器 |
| `download.rs` | 重试、状态码校验、SHA1 校验、原子落盘 |
| `net.rs` | 统一 HTTP 客户端（UA、超时、镜像回退） |

### 账号与皮肤

| 模块 | 职责 |
|---|---|
| `account.rs` | 账号模型、离线 UUID（`MD5("OfflinePlayer:name")`）、激活账号 |
| `yggdrasil.rs` | 外置登录，端点逐个探测（不同皮肤站路径不同） |
| `microsoft.rs` | 微软 OAuth 设备码 → XBL → XSTS → Minecraft |
| `skins.rs` | 皮肤缓存与 Blessing Skin 皮肤库 |
| `injector.rs` | `authlib-injector` 的下载、校验、拼进 `-javaagent` |

### 外围

| 模块 | 职责 |
|---|---|
| `modrinth.rs` | Modrinth 搜索 / 版本 / 安装，本地模组开关 |
| `modpack.rs` | `.mrpack` 导入导出（zip 操作在这里，注意路径穿越） |
| `importer.rs` | 从 HMCL / 官方 / Prism 导入 |
| `terracotta.rs` | 陶瓦联机，下载官方包并只取裸二进制 |
| `glass.rs` | 三端毛玻璃能力申请，失败交给前端降级 |

## 启动链：一次点击发生了什么

`launch_game` 之后：

1. `manifest.rs` 读版本 JSON，沿 `inheritsFrom` 向上找到底，**深合并**成一份完整清单；
2. `java.rs` 选 JDK（版本要求 → 已装候选 → 实例自带的）；
3. `launch.rs` 展开 `arguments.jvm` / `arguments.game` 里的占位符与 `rules`；
4. 拼 classpath，校验每个文件是否存在；
5. `game.rs` 拉起进程，把 stdout 同时 emit 给界面、写进 `logs/game-latest.log`；
6. 进程退出时 emit `game-exit`，前端把按钮状态改回来。

### 四条不要随便改的规则

这四条都是从「游戏能启动」这个结果反推出来的，简化任何一条都会炸：

1. **`inheritsFrom` 必须深合并**，而且：
   - `libraries` 按 `group:artifact`（不含版本）取并集，子版本覆盖父版本。
     不同版本的同名库同时保留，NeoForge 会抛
     `Module named org.objectweb.asm.commons was already on the JVM's module path`。
   - `arguments` 父 + 子依次拼接，再用 `dedupe_options` / `dedupe_game_arguments` 去重
     （`-p`/`-cp`/`--add-modules`/`-D*` 与 `--key value` 保留最后一个），否则会
     `MultipleArgumentsForOptionException: Found multiple arguments for option launchTarget`。
2. **客户端 jar 放在父版本目录**，文件名取版本 JSON 的 `jar` 字段（如 `1.21.1-NeoForge`），
   并且必须排在 classpath 第一位，否则主类找不到。
3. **`rules` 按「最后匹配的规则说了算」**展开，并检查 `os.arch`；带 `features` 的规则一律视为不匹配，
   否则命令里会残留 `${quickPlay…}` 占位符。
4. **macOS 需要 `-XstartOnFirstThread`**，旧版 JSON 没有时要补上，否则 GLFW 初始化失败。

## 前端

没有框架，没有打包器。三个约定：

- `index.html` 里的每一页是 `<section class="page" data-page="xxx">`，导航是 `data-nav="xxx"`。
- `app.js` 持有全局状态 `S` 与渲染函数，并把三个时机暴露成钩子：

  ```js
  (function () {
    const HOOKS = (window.__GM_HOOKS = window.__GM_HOOKS || { ready: [], nav: [], render: [] });
    HOOKS.render.push(() => { /* 重新渲染我这一块 */ });
  })();
  ```

  顺序：`ready` 启动时一次；`nav` 切页时；`render` 每次状态刷新后。
  **`features*.js` 必须用 `setTimeout(boot, 0)` 异步启动**，否则脚本还没解析完，
  前面的模块就已经走完了启动流程，钩子全部丢失。
- 所有 `invoke` 都走 `app.js` 里的同一个封装；参数用 camelCase，Rust 侧自动转 snake_case。

### 浏览器 mock 模式

没有 `window.__TAURI__` 时，`app.js` 自动切到 `mockInvoke`，返回一份假数据。
所以你可以只开个静态服务就调界面：

```bash
python3 -m http.server 8080 --directory src
```

mock 数据在 `app.js` 顶部的 `MOCK_STATE`，改界面时顺手改它，能让新的空态也在浏览器里看得到。

## 调试技巧

```bash
# 打印某个版本的完整启动命令（不启动游戏）
glassmc --print-command 1.21.11

# 直接命令行安装，绕过界面
glassmc --install 1.21.11
```

排查「启动不了」时，先看 `--print-command` 的输出里还有没有 `${...}` 残留、
classpath 里有没有缺失文件，再去看游戏日志。

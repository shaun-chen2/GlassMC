# 跨内核渲染一致性：实测记录

> 起因：在 macOS 上跑 Tauri，页面"看起来变形"，和 Chrome 里预览的不一样。
> 结论：**这不是玄学，是两个可定位、可修复的具体差异。** 本文是实测数据与修复方案。

测试环境：macOS 27.0.1 · Retina（DPR = 2）· WKWebView（WebKit 605.1.15）· Chrome 156 headless · Tauri 2（debug build）

测试方法：同一份 HTML，分别用裸 WKWebView、Chrome、真实 Tauri 窗口加载，
页面内跑同一段 JS 探针读取 `getBoundingClientRect` / `getComputedStyle` / `CSS.supports`，
三方逐项 diff。Tauri 的探针结果通过 `fetch` POST 回本地端口取出（避免依赖截图）。

---

## 一、根因 1：`line-height: normal` 两个内核算得不一样

这是"页面变形"最主要的原因。

| 元素 | 字号 | WKWebView 行高 | Chrome 行高 |
|---|---|---|---|
| `h1` | 22px | **26px**（×1.18） | **31px**（×1.41） |
| 卡片（含 11/24/10px 三层文字）| — | 91px | 93px |

WebKit 与 Blink 对"normal 行高"的取值来自各自对字体 `ascent + descent + lineGap` 的解释，
WebKit 偏紧（约 1.18×），Blink 偏松（约 1.41×）。单看一个元素只差 5px，
但一屏几十个文字块累积下来，整页高度、折行位置、卡片对齐全都会变——这就是"变形"的观感来源。

**修复：所有文本显式声明 `line-height`，一个都不留给 `normal`。**

修复后：

| 指标 | WKWebView | Chrome | 差异 |
|---|---|---|---|
| `h1` 高度 | 27.5px | 27.5px | **0** |
| 卡片高度 | 95.98px | 95.99px | 0.01px |

见 `src/styles/baseline.css`。

---

## 二、根因 2：窗口尺寸 ≠ 内容区尺寸

Tauri 的 `tauri.conf.json` 里 `width` / `height` 指的是**窗口尺寸**，不是 webview 内容区。
`decorations: true` 时，标题栏吃掉 32px：

```
配置 height: 800  →  实际 innerHeight: 768
```

如果前端按 800 设计（`100vh` 布局、`height: 800px` 的分栏），底部会被裁掉，
或者冒出不该出现的滚动条。这也会被误认为"页面被压扁了"。

**修复：用 `decorations: false`，自绘标题栏。**

实测：

| 配置 | 窗口 | 内容区 |
|---|---|---|
| `decorations: true`, height 800 | 1200×800 | 1200×**768** |
| `decorations: false`, height 800 | 1200×800 | 1200×**800** ✅ |

GlassMC 这种"整窗液态玻璃"的界面本来就应该无边框——玻璃可以一直延伸到顶，
不需要系统标题栏那条灰带。顺便还消掉了这个尺寸损耗。

---

## 三、实测排除的"疑似问题"

以下这些都逐个测过，**没有问题**，不用浪费时间调：

| 项目 | 结果 |
|---|---|
| flex / grid 布局 | 完全一致（色块行 972×42 两边相同） |
| 圆角、渐变 | 完全一致 |
| `backdrop-filter` | 两边都支持；WK 还额外支持 `-webkit-` 前缀 |
| `border-radius` 取整 | 一致 |
| DPR | 都是 2 |
| 完整字体链的西文字宽 | **176.33 = 176.33**，完全一致（见下） |
| Tauri vs 裸 WKWebView | **29 项一致**，仅 6 项差异且全部来自上面那个 32px |

### 关于字体，有个坑要说清楚

字体**不是**问题，但字体链写不全就会变成问题：

| 写法 | WKWebView | Chrome | 差异 |
|---|---|---|---|
| `20px -apple-system` | 176.33 | 190.53 | **7.5%** ❌ |
| `20px "SF Pro Text"` | 168.3 | 190.53 | **11.7%** ❌ |
| `20px -apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif` | 176.33 | 176.33 | **0** ✅ |
| `20px Arial` | 178.98 | 178.98 | 0 ✅ |

`-apple-system` 是 WebKit 私有关键字，Blink 不认；`"SF Pro Text"` 在 Chrome 里也不是合法族名。
**只写 `-apple-system` 会在 Chrome 侧静默回退到默认字体**，字宽差 7.5%——这才是网上很多
"Tauri 页面字变大/字变宽"的真正原因。字体链写全就没事。

### 唯一残留：等宽字体

| 写法 | WKWebView | Chrome | 差异 |
|---|---|---|---|
| `ui-monospace, SFMono-Regular, Menlo, monospace` | 108.8（SF Mono） | 105.97（Menlo） | 2.7% |

`ui-monospace` 在 WebKit 解析成 SF Mono，在 Blink 解析成 Menlo。11px 下每字符差 0.3px，
如果拿等宽字体做对齐（版本号、日志、邀请码），会有肉眼可见的错位。

**修复：等宽场景显式写死一个字体**（本项目统一用 `Menlo`，三端都有或可回退）。

---

## 四、可直接抄的配置

### `tauri.conf.json` 窗口部分

```json
{
  "app": {
    "windows": [{
      "width": 1200,
      "height": 800,
      "decorations": false,
      "transparent": true,
      "titleBarStyle": "Overlay",
      "dragDropEnabled": true
    }]
  }
}
```

- `decorations: false` —— 消掉 32px 损耗，玻璃延伸到顶
- `transparent: true` —— 配合 `window-vibrancy` 做窗口级毛玻璃
- 无边框后需要自己实现窗口拖动：给标题区域加 `data-tauri-drag-region`

### CSS 基线

见 `src/styles/baseline.css`，必须在所有业务样式之前引入。

---

## 五、验证脚本

想自己复现这套测量：

```bash
# 1. WKWebView 侧
swiftc -O probe.swift -o probe -framework Cocoa -framework WebKit
./probe test.html wk.json wk.png

# 2. Chrome 侧
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --no-sandbox --hide-scrollbars \
  --window-size=1200,800 --force-device-scale-factor=2 \
  --virtual-time-budget=4000 --dump-dom file:///path/test.html

# 3. diff 两份 JSON 即可
```

注意：Chrome headless 的 `--window-size` 不等于 `innerHeight`（实测 800 → 657），
所以**只比元素内部尺寸，不要比 viewport 高度**，否则会得出假差异。

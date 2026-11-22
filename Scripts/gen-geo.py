#!/usr/bin/env python3
"""glassmc.shuan.uk 的 GEO（生成式引擎优化）资产生成器。

做四件事，全部幂等，可以反复运行：

  1. 往 docs/index.html 的 GEO 标记之间写入头块：
     canonical / robots / hreflang / Open Graph / Twitter Card / JSON-LD
  2. 生成 docs/robots.txt     显式放行主流 AI 爬虫，并指向 sitemap
  3. 生成 docs/sitemap.xml
  4. 生成 docs/llms.txt 与 docs/llms-full.txt（给 LLM 的摘要与全文语料）

FAQ 的结构化数据是从页面里真实的 <details> 抽出来的，
保证「schema 说的」和「用户看到的」永远一致，不会各写一份然后慢慢漂移。

用法：
    python3 Scripts/gen-geo.py                 # 主站（docs/index.html）
    python3 Scripts/gen-geo.py --web <文件>    # 构建产物里的体验页 /web/index.html
"""

from __future__ import annotations

import argparse
import datetime as dt
import html
import json
from html.parser import HTMLParser
import re
import subprocess
import sys
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
INDEX = DOCS / "index.html"

SITE = "https://glassmc.shuan.uk"
REPO = "https://github.com/shaun-chen2/GlassMC"
AUTHOR = "shaun-chen2"

BEGIN = "<!-- GEO:BEGIN 由 Scripts/gen-geo.py 生成，请勿手改（重新运行脚本即可覆盖） -->"
END = "<!-- GEO:END -->"

META_DESC = (
    "GlassMC 是一个跨平台的 Minecraft 启动器：液态玻璃界面，一键装版本、加载器与模组，"
    "支持离线 / 微软正版 / 皮肤站登录，内置 Modrinth、整合包导入、陶瓦联机、"
    "存档备份、崩溃诊断、服务器延迟与 MCP 服务端。"
    "Rust + Tauri 2 实现，macOS（Apple Silicon / Intel）、Windows、Linux 同一套代码，"
    "并提供浏览器在线体验页。"
)
SHORT_DESC = (
    "GlassMC 是 Rust + Tauri 2 写的跨平台 Minecraft 启动器：系统级毛玻璃界面，"
    "393 个正式版与快照、Fabric / Quilt / Forge / NeoForge、JDK 自动配置、"
    "Modrinth 模组、整合包导入导出、陶瓦联机、存档备份、崩溃诊断与 MCP 服务端，"
    "macOS / Windows / Linux 同一套代码，GPLv3 开源。"
)
OG_TITLE = "GlassMC · 液态玻璃 Minecraft 启动器下载"
OG_DESC = (
    "跨平台（macOS / Windows / Linux），一键装版本与模组，支持离线 / 微软正版 / 认证服务器登录，"
    "内置陶瓦联机、存档备份、崩溃诊断与 MCP 服务端。不想先安装，可以直接在浏览器里在线体验。"
)
OG_ALT = "GlassMC 界面：液态玻璃质感的启动器窗口浮在深色星云壁纸上"

FEATURES = [
    "393 个正式版与快照，一键安装并逐项校验 SHA1",
    "Fabric / Quilt / Forge / NeoForge 四种加载器",
    "JDK 8 / 11 / 17 / 21 按游戏版本自动配置，清华镜像下载",
    "离线登录 / 微软正版 / 外置登录（Yggdrasil，LittleSkin、Ely.by、MiniSkins）",
    "Modrinth 模组搜索安装、缺失依赖提示、本地模组开关",
    "整合包导入与导出（.mrpack）",
    "从 HMCL / 官方启动器 / Prism、MultiMC 一键搬家",
    "陶瓦联机：邀请码进服，不用公网 IP 和端口映射",
    "系统级毛玻璃：NSVisualEffectView / Mica / Acrylic / WebKitGTK",
]


# --------------------------------------------------------------------------- 文本工具
def text_of(fragment: str) -> str:
    """HTML 片段转纯文本（保留行内语义，压缩空白）。"""
    s = re.sub(r"<(script|style|svg)\b[\s\S]*?</\1>", " ", fragment, flags=re.I)
    s = re.sub(r"<button\b[^>]*>[\s\S]*?</button>", " ", s, flags=re.I)
    s = re.sub(r"<br\s*/?>", " ", s, flags=re.I)
    s = re.sub(r"<[^>]+>", "", s)
    s = html.unescape(s).replace("\u00a0", " ")
    return re.sub(r"\s+", " ", s).strip()


def git(*args: str) -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(ROOT), *args], text=True, stderr=subprocess.DEVNULL
        ).strip()
    except Exception:
        return ""


def dates() -> tuple[str, str]:
    """(首次发布, 最后修改)。

    最后修改取 git 提交日期；如果页面有尚未提交的改动，说明这次改动就是要发布的版本，
    直接用今天 —— 这样「先改后提交再构建」和「构建后再提交」得到同一个日期，不会来回抖。
    """
    today = dt.date.today().isoformat()
    first = git("log", "--reverse", "--format=%cs", "--", "docs/index.html").splitlines()
    last = git("log", "-1", "--format=%cs", "--", "docs/index.html") or today
    dirty = subprocess.run(
        ["git", "-C", str(ROOT), "diff", "--quiet", "HEAD", "--", "docs/index.html"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ).returncode != 0
    return (first[0] if first else today), (today if dirty else last)


def version_of(page: str) -> str:
    m = re.search(r"v(\d+\.\d+\.\d+)", page)
    return m.group(1) if m else "1.0.0"


# --------------------------------------------------------------------------- 内容抽取
def faq_pairs(page: str) -> list[tuple[str, str]]:
    seg = re.search(r'<section[^>]*id="faq"[\s\S]*?</section>', page)
    if not seg:
        return []
    out: list[tuple[str, str]] = []
    for block in re.findall(r"<details\b[^>]*>([\s\S]*?)</details>", seg.group(0)):
        qm = re.search(r"<summary[^>]*>([\s\S]*?)</summary>", block)
        if not qm:
            continue
        q = re.sub(r"^Q\d+\s*", "", text_of(qm.group(1))).strip()
        a = text_of(block[qm.end():])
        if q and a:
            out.append((q, a))
    return out


VOID = {
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "param", "source", "track", "wbr",
}


class _Extractor(HTMLParser):
    """HTML → Markdown 的极简抽取器。

    保留标题层级、段落、列表、代码块；整块丢掉 nav / footer / button / svg /
    aria-hidden 的装饰（跑马灯、假终端、复制按钮）。正则做不到“按嵌套深度整块跳过”，
    所以这里用解析器。
    """

    SKIP_TAGS = {"script", "style", "svg", "nav", "footer", "button", "template", "noscript"}
    BLOCK_TAGS = {
        "p", "div", "section", "article", "header", "details", "summary",
        "ul", "ol", "table", "tr", "blockquote", "figure", "figcaption",
    }

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.out: list[str] = []
        self.skip = 0
        self.pre = 0

    @staticmethod
    def _decorative(tag: str, attrs: list[tuple[str, str | None]]) -> bool:
        if tag in _Extractor.SKIP_TAGS:
            return True
        d = dict(attrs)
        if d.get("aria-hidden") == "true":
            return True
        cls = d.get("class") or ""
        tokens = set(cls.split())
        return bool(tokens & {"marquee", "term", "sec-tag", "sec-divider", "q"}) or "marquee" in cls or "term-" in cls

    def handle_starttag(self, tag: str, attrs) -> None:
        if self.skip:
            if tag not in VOID:
                self.skip += 1
            return
        if self._decorative(tag, attrs):
            self.skip = 1
            return
        if tag in ("h1", "h2", "h3", "h4", "h5", "h6"):
            self.out.append("\n\n" + "#" * int(tag[1]) + " ")
        elif tag == "li":
            self.out.append("\n- ")
        elif tag == "br":
            self.out.append("\n")
        elif tag == "pre":
            self.pre += 1
            self.out.append("\n\n```\n")
        elif tag == "code" and not self.pre:
            self.out.append("`")
        elif tag in self.BLOCK_TAGS:
            self.out.append("\n\n")

    def handle_startendtag(self, tag: str, attrs) -> None:
        if self.skip:
            return
        if tag == "br":
            self.out.append("\n")

    def handle_endtag(self, tag: str) -> None:
        if self.skip:
            if tag not in VOID:
                self.skip -= 1
            return
        if tag in ("h1", "h2", "h3", "h4", "h5", "h6"):
            self.out.append("\n\n")
        elif tag == "li":
            self.out.append("\n")
        elif tag == "pre":
            self.pre = max(0, self.pre - 1)
            self.out.append("\n```\n\n")
        elif tag == "code" and not self.pre:
            self.out.append("`")
        elif tag in self.BLOCK_TAGS:
            self.out.append("\n\n")

    def handle_data(self, data: str) -> None:
        if self.skip:
            return
        data = data.replace("\u00a0", " ")
        if self.pre:
            self.out.append(data)
            return
        if not data.strip():
            if self.out and not self.out[-1].endswith(("\n", " ")):
                self.out.append(" ")
            return
        self.out.append(re.sub(r"\s+", " ", data))


def page_to_markdown(page: str) -> str:
    """把介绍页正文转成 Markdown（装饰元素与交互控件不进语料）。"""
    ex = _Extractor()
    ex.feed(page[page.find("<body"):])
    ex.close()
    text = "".join(ex.out)
    text = re.sub(r"[ \t]+", " ", text)
    text = re.sub(r" *\n *", "\n", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip()


# --------------------------------------------------------------------------- 结构化数据
def build_graph(page: str) -> dict:
    pub, mod = dates()
    ver = version_of(page)
    faqs = faq_pairs(page)

    author = {
        "@type": "Person",
        "@id": f"{SITE}/#author",
        "name": AUTHOR,
        "url": f"https://github.com/{AUTHOR}",
        "sameAs": [f"https://github.com/{AUTHOR}", REPO],
    }
    website = {
        "@type": "WebSite",
        "@id": f"{SITE}/#website",
        "url": f"{SITE}/",
        "name": "GlassMC",
        "alternateName": ["GlassMC 启动器", "GlassMC 官网", "GlassMC Minecraft Launcher"],
        "description": SHORT_DESC,
        "inLanguage": "zh-CN",
        "publisher": {"@id": f"{SITE}/#author"},
    }
    webpage = {
        "@type": "WebPage",
        "@id": f"{SITE}/#webpage",
        "url": f"{SITE}/",
        "name": "GlassMC · 液态玻璃 Minecraft 启动器下载",
        "description": META_DESC,
        "isPartOf": {"@id": f"{SITE}/#website"},
        "about": {"@id": f"{SITE}/#software"},
        "primaryImageOfPage": {
            "@type": "ImageObject",
            "url": f"{SITE}/images/og-cover.png",
            "width": 1200,
            "height": 630,
            "caption": OG_ALT,
        },
        "datePublished": pub,
        "dateModified": mod,
        "inLanguage": "zh-CN",
        "keywords": "Minecraft 启动器, MC 启动器, 我的世界启动器, 液态玻璃, Tauri, Rust, NeoForge, Fabric, Modrinth",
    }
    app = {
        "@type": "SoftwareApplication",
        "@id": f"{SITE}/#software",
        "name": "GlassMC",
        "alternateName": ["GlassMC 启动器", "GlassMC Minecraft Launcher", "GlassMC 液态玻璃启动器"],
        "description": SHORT_DESC,
        "applicationCategory": "GameApplication",
        "applicationSubCategory": "Minecraft 启动器",
        "operatingSystem": "macOS 15+, Windows 10 1809+ (x64), Linux (WebKitGTK 4.1)",
        "softwareVersion": ver,
        "softwareRequirements": "macOS 15+（Universal）；Windows 10 1809+ / 11（x64，需 WebView2）；Linux（需 WebKitGTK 4.1）",
        "downloadUrl": f"{REPO}/releases/latest",
        "installUrl": f"{SITE}/#download",
        "softwareHelp": {"@type": "CreativeWork", "url": f"{SITE}/#faq"},
        "license": "https://www.gnu.org/licenses/gpl-3.0.html",
        "isAccessibleForFree": True,
        "offers": {
            "@type": "Offer",
            "price": "0",
            "priceCurrency": "CNY",
            "availability": "https://schema.org/InStock",
        },
        "author": {"@id": f"{SITE}/#author"},
        "publisher": {"@id": f"{SITE}/#author"},
        "inLanguage": "zh-CN",
        "screenshot": f"{SITE}/images/app-window.jpg",
        "featureList": FEATURES,
        "sameAs": [REPO],
    }
    howto = {
        "@type": "HowTo",
        "@id": f"{SITE}/#install-macos",
        "name": "在 macOS 上安装并首次打开 GlassMC",
        "description": "GlassMC 的 dmg 没有 Apple 开发者签名，首次打开会被 Gatekeeper 拦一次，按这三步装好。",
        "totalTime": "PT2M",
        "tool": [{"@type": "HowToTool", "name": "GlassMC.dmg"}],
        "step": [
            {
                "@type": "HowToStep",
                "position": 1,
                "name": "下载并拖进「应用程序」",
                "text": "双击 GlassMC.dmg，把图标拖到旁边的 Applications 文件夹。",
                "url": f"{SITE}/#install",
            },
            {
                "@type": "HowToStep",
                "position": 2,
                "name": "第一次打开时放行一次",
                "text": "双击 GlassMC 被拦下时点「完成」，打开「系统设置 → 隐私与安全性 → 安全性」，"
                        "点「仍要打开」并输入密码；再双击一次，在弹窗里点「打开」。之后不再询问。",
                "url": f"{SITE}/#install",
            },
            {
                "@type": "HowToStep",
                "position": 3,
                "name": "登录并安装版本",
                "text": "登录账号后到「安装」页挑一个版本点安装，下载完回「版本」页点启动。",
                "url": f"{SITE}/#install",
            },
        ],
    }

    graph: list[dict] = [author, website, webpage, app, howto]
    if faqs:
        graph.append(
            {
                "@type": "FAQPage",
                "@id": f"{SITE}/#faq",
                "isPartOf": {"@id": f"{SITE}/#webpage"},
                "mainEntity": [
                    {
                        "@type": "Question",
                        "name": q,
                        "acceptedAnswer": {"@type": "Answer", "text": a},
                    }
                    for q, a in faqs
                ],
            }
        )
    return {"@context": "https://schema.org", "@graph": graph}


def build_head_block(page: str) -> str:
    data = build_graph(page)
    ld = json.dumps(data, ensure_ascii=False, indent=2)
    return "\n".join(
        [
            BEGIN,
            f'<link rel="canonical" href="{SITE}/">',
            '<meta name="robots" content="index, follow, max-image-preview:large, max-snippet:-1, max-video-preview:-1">',
            f'<meta name="author" content="{AUTHOR}">',
            '<meta name="application-name" content="GlassMC">',
            '<meta name="apple-mobile-web-app-title" content="GlassMC">',
            f'<link rel="alternate" hreflang="zh-CN" href="{SITE}/">',
            f'<link rel="alternate" hreflang="x-default" href="{SITE}/">',
            f'<link rel="alternate" type="text/plain" href="{SITE}/llms.txt" title="LLM 摘要（llms.txt）">',
            '<meta property="og:type" content="website">',
            '<meta property="og:site_name" content="GlassMC">',
            '<meta property="og:locale" content="zh_CN">',
            f'<meta property="og:url" content="{SITE}/">',
            f'<meta property="og:title" content="{OG_TITLE}">',
            f'<meta property="og:description" content="{OG_DESC}">',
            f'<meta property="og:image" content="{SITE}/images/og-cover.png">',
            f'<meta property="og:image:secure_url" content="{SITE}/images/og-cover.png">',
            '<meta property="og:image:type" content="image/png">',
            '<meta property="og:image:width" content="1200">',
            '<meta property="og:image:height" content="630">',
            f'<meta property="og:image:alt" content="{OG_ALT}">',
            '<meta name="twitter:card" content="summary_large_image">',
            f'<meta name="twitter:title" content="{OG_TITLE}">',
            f'<meta name="twitter:description" content="{OG_DESC}">',
            f'<meta name="twitter:image" content="{SITE}/images/og-cover.png">',
            f'<meta name="twitter:image:alt" content="{OG_ALT}">',
            '<script type="application/ld+json">',
            ld,
            "</script>",
            END,
        ]
    )


# --------------------------------------------------------------------------- 文件生成
def write_index(page_path: pathlib.Path) -> None:
    page = page_path.read_text(encoding="utf-8")
    block = build_head_block(page)
    if BEGIN in page and END in page:
        page = re.sub(re.escape(BEGIN) + r"[\s\S]*?" + re.escape(END), lambda _: block, page, count=1)
    else:
        anchor = re.search(r'<meta name="theme-color"[^>]*>', page)
        if not anchor:
            sys.exit("找不到 <meta name=\"theme-color\">，无法定位插入点")
        page = page[: anchor.end()] + "\n" + block + page[anchor.end():]
    page_path.write_text(page, encoding="utf-8")


def write_robots() -> None:
    bots = [
        "GPTBot", "OAI-SearchBot", "ChatGPT-User",
        "ClaudeBot", "Claude-Web", "anthropic-ai",
        "PerplexityBot", "Perplexity-User",
        "Google-Extended", "Applebot-Extended", "CCBot",
        "Bytespider", "Baiduspider", "YisouSpider",
        "Amazonbot", "cohere-ai", "meta-externalagent",
    ]
    lines = [
        "# glassmc.shuan.uk — 公开项目站点，欢迎搜索引擎与 AI 抓取",
        "",
        "User-agent: *",
        "Allow: /",
        "",
        "# 主流大模型 / AI 检索爬虫，逐条显式放行",
    ]
    lines += [f"User-agent: {b}" for b in bots]
    lines += [
        "Allow: /",
        "",
        f"Sitemap: {SITE}/sitemap.xml",
        f"# LLM 友好的纯文本索引：{SITE}/llms.txt",
        "",
    ]
    (DOCS / "robots.txt").write_text("\n".join(lines), encoding="utf-8")


def write_sitemap() -> None:
    _, mod = dates()
    urls = [
        (f"{SITE}/", mod, "weekly", "1.0"),
        (f"{SITE}/web/", mod, "monthly", "0.7"),
    ]
    body = "\n".join(
        f"  <url>\n    <loc>{u}</loc>\n    <lastmod>{d}</lastmod>\n"
        f"    <changefreq>{c}</changefreq>\n    <priority>{p}</priority>\n  </url>"
        for u, d, c, p in urls
    )
    xml = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        f"{body}\n</urlset>\n"
    )
    (DOCS / "sitemap.xml").write_text(xml, encoding="utf-8")


def write_llms(page: str) -> None:
    ver = version_of(page)
    faqs = faq_pairs(page)

    lines = [
        "# GlassMC",
        "",
        f"> {SHORT_DESC}",
        "",
        "GlassMC 是一个直接面向最终用户的桌面应用（不是模组加载器、也不是服务端），"
        "装好之后选版本、点安装、点启动即可进游戏；启动参数、Java 运行时、依赖库、"
        "资源对象、皮肤注入都由启动器处理。界面使用操作系统原生的毛玻璃材质"
        "（macOS 的 NSVisualEffectView、Windows 的 Mica / Acrylic、Linux 的 WebKitGTK），"
        "不是网页里叠滤镜模拟出来的效果。",
        "",
        "## 核心事实",
        "",
        f"- 当前版本：{ver}",
        "- 平台：macOS 15+（Apple Silicon 与 Intel 共用一个 Universal 包）、"
        "Windows 10 1809+ / 11（x64，需要 WebView2）、Linux（需要 WebKitGTK 4.1）",
        "- 技术栈：Rust 后端 + Tauri 2 + 无构建的静态前端（HTML / CSS / 原生 JS）",
        "- 许可：GPLv3",
        "- 安装包体积：macOS dmg 约 9 MB，Windows exe 约 3 MB，Linux deb 约 3.4 MB",
        "- 源码与 issue：" + REPO,
        "- 下载：" + f"{REPO}/releases/latest",
        f"- 在线体验（浏览器里跑同一套前端，示例数据，不装游戏、不连账号）：{SITE}/web/",
        "- 关联声明：本项目与 Mojang / Microsoft 无关联；Minecraft 是 Mojang Studios 的商标；"
        "连正版服务器需要自备正版账号。",
        "",
        "## 能力",
        "",
    ]
    lines += [f"- {f}" for f in FEATURES]
    lines += [
        "",
        "## 页面",
        "",
        f"- [介绍页]({SITE}/)：功能、平台状态、三平台安装与放行步骤、常见问题",
        f"- [在线体验]({SITE}/web/)：八个页面的可交互界面，数据是示例数据",
        "",
        "## 技术文档（Markdown 原文）",
        "",
        f"- [架构说明]({SITE}/docs/architecture.md)：进程模型、模块划分、数据目录",
        f"- [开发说明]({SITE}/docs/development.md)：环境搭建、构建、调试",
        f"- [渲染一致性]({SITE}/docs/rendering-parity.md)：三个平台 WebView 的差异与对齐方式",
        "",
        "## 常见问题",
        "",
    ]
    for q, a in faqs:
        brief = a.strip()
        if len(brief) > 130:
            cut = brief[:130]
            pos = max(cut.rfind("。"), cut.rfind("；"))
            brief = cut[: pos + 1] if pos >= 60 else cut.rstrip("，、；") + "…"
        lines.append(f"- **{q}** {brief}")
    lines += [
        "",
        "## 可选",
        "",
        f"- [llms-full.txt]({SITE}/llms-full.txt)：本页正文与三份技术文档的纯文本全文，适合一次性摄取",
        "",
    ]
    (DOCS / "llms.txt").write_text("\n".join(lines), encoding="utf-8")

    # ---- 全文语料
    parts = [
        "# GlassMC 全文语料",
        "",
        f"> 站点：{SITE}/",
        f"> 内容更新时间：{dates()[1]}（由 Scripts/gen-geo.py 自动生成）",
        "> 内容：介绍页正文 + 仓库内技术文档。许可 GPLv3。",
        "",
        "---",
        "",
        "# 一、介绍页正文",
        "",
        page_to_markdown(page),
        "",
    ]
    for title, name in [
        ("二、架构说明", "architecture.md"),
        ("三、开发说明", "development.md"),
        ("四、渲染一致性", "rendering-parity.md"),
    ]:
        f = DOCS / name
        if f.exists():
            parts += ["---", "", f"# {title}（docs/{name}）", "", f.read_text(encoding="utf-8").strip(), ""]
    (DOCS / "llms-full.txt").write_text("\n".join(parts), encoding="utf-8")


# --------------------------------------------------------------------------- 体验页
def patch_web(path: pathlib.Path) -> None:
    """给构建出来的 /web/index.html 补上标题、描述、canonical 与 WebPage 标记。"""
    page = path.read_text(encoding="utf-8")
    if BEGIN in page:
        return
    page = page.replace("<title>GlassMC</title>", "<title>GlassMC 在线体验 · 浏览器里的 Minecraft 启动器</title>", 1)
    ld = {
        "@context": "https://schema.org",
        "@type": "WebPage",
        "@id": f"{SITE}/web/#webpage",
        "url": f"{SITE}/web/",
        "name": "GlassMC 在线体验",
        "description": "在浏览器里直接操作 GlassMC 的界面：首页、版本、安装、模组、账号、设置等八个页面都能点，"
                       "数据是示例数据，不会真的下载游戏或连接账号。",
        "isPartOf": {"@id": f"{SITE}/#website"},
        "about": {"@id": f"{SITE}/#software"},
        "inLanguage": "zh-CN",
        "isAccessibleForFree": True,
        "primaryImageOfPage": {"@type": "ImageObject", "url": f"{SITE}/images/og-cover.png", "width": 1200, "height": 630},
    }
    block = "\n".join(
        [
            BEGIN,
            f'<link rel="canonical" href="{SITE}/web/">',
            '<meta name="robots" content="index, follow, max-image-preview:large">',
            '<meta name="description" content="在浏览器里直接操作 GlassMC 的界面：八个页面都能点，示例数据，不装游戏、不连账号。">',
            f'<link rel="alternate" type="text/plain" href="{SITE}/llms.txt" title="LLM 摘要（llms.txt）">',
            '<meta property="og:type" content="website">',
            '<meta property="og:site_name" content="GlassMC">',
            '<meta property="og:locale" content="zh_CN">',
            f'<meta property="og:url" content="{SITE}/web/">',
            '<meta property="og:title" content="GlassMC 在线体验 · 浏览器里的 Minecraft 启动器">',
            '<meta property="og:description" content="八个页面都能点，示例数据，不装游戏、不连账号。">',
            f'<meta property="og:image" content="{SITE}/images/og-cover.png">',
            '<meta property="og:image:width" content="1200">',
            '<meta property="og:image:height" content="630">',
            '<meta name="twitter:card" content="summary_large_image">',
            '<script type="application/ld+json">',
            json.dumps(ld, ensure_ascii=False, indent=2),
            "</script>",
            END,
        ]
    )
    page = page.replace("</head>", block + "\n</head>", 1)
    path.write_text(page, encoding="utf-8")


# --------------------------------------------------------------------------- 入口
def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--web", metavar="HTML", help="给构建产物里的体验页补元信息")
    args = ap.parse_args()

    if args.web:
        patch_web(pathlib.Path(args.web))
        print("已写入体验页元信息：", args.web)
        return

    page = INDEX.read_text(encoding="utf-8")
    faqs = faq_pairs(page)
    if not faqs:
        sys.exit("没有从 #faq 抽到任何问答，拒绝生成（否则 FAQPage 会是空的）")

    write_index(INDEX)
    write_robots()
    write_sitemap()
    write_llms(INDEX.read_text(encoding="utf-8"))
    print(f"GEO 资产已生成：{len(faqs)} 条 FAQ、robots.txt、sitemap.xml、llms.txt、llms-full.txt")


if __name__ == "__main__":
    main()

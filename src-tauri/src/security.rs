//! 安全中心：恶意模组启发式扫描 + Minecraft 官方新闻聚合。
//!
//! 扫描是纯只读的：逐个 zip 条目读取，只统计与提示，绝不删除、隔离或改写任何文件。
//! 所有结论都只是「启发式提示，不是杀毒」，命中不代表一定存在恶意代码。
//! 新闻走官方 RSS，失败回退本地缓存，完全没有缓存时给明确标注的离线示例。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::net;
use crate::paths;
use crate::store;

// ---------------------------------------------------------------- 数据结构

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Finding {
    /// high / medium / low
    pub severity: String,
    pub title: String,
    pub detail: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ModScanResult {
    /// 文件名，用于界面显示
    pub file: String,
    /// 完整路径，供 open_dir 使用
    pub path: String,
    pub sha256: String,
    pub findings: Vec<Finding>,
    /// 0~100，越高越可疑
    pub score: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanReport {
    pub results: Vec<ModScanResult>,
    pub scanned: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsItem {
    pub title: String,
    pub link: String,
    pub pub_date: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsResult {
    pub items: Vec<NewsItem>,
    /// true = 来自本地缓存（离线）
    pub cached: bool,
    /// true = 内置离线示例（无缓存且无网络）
    pub demo: bool,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct NewsCache {
    items: Vec<NewsItem>,
    updated_at: i64,
}

// ---------------------------------------------------------------- 常量

/// 防 zip bomb：单个条目最多读 1MB，最多看 2000 个条目。
const MAX_ENTRY_BYTES: u64 = 1024 * 1024;
const MAX_ENTRIES: usize = 2000;
const MAX_NEWS: usize = 12;

/// 仅作提示，不是杀毒：名字来自公开报道，命中不代表一定有恶意代码。
const KNOWN_BAD: &[&str] = &[
    "serverseeker", // 曾被用于窃取会话 token
    "wurst",        // 知名作弊客户端
    "liquidbounce", // 作弊客户端
    "novoline",     // 作弊客户端
    "kakao",        // 曾出现注入事件
    "frus",         // 广告 / 捆绑模组
];

/// .class 常量池里的 ASCII 字符串通常原样存储，直接搜字节即可。
/// (字节串, 说明) —— 命中统一按高危计分，说明用于前端提示文案。
const SUSPICIOUS: &[(&str, &str)] = &[
    ("java/lang/ProcessBuilder", "ProcessBuilder 创建外部进程"),
    ("ProcessBuilder", "ProcessBuilder 创建外部进程"),
    ("java/lang/Runtime", "引用 java.lang.Runtime"),
    ("getRuntime", "调用 getRuntime()"),
    ("java/net/URLClassLoader", "URLClassLoader 动态类加载"),
    ("URLClassLoader", "URLClassLoader 动态类加载"),
    ("defineClass", "运行时定义类 defineClass"),
    ("java/net/Socket", "使用网络 Socket"),
];

const EXEC_EXTS: &[&str] = &[".exe", ".dll", ".sh", ".bat", ".vbs", ".cmd", ".ps1"];
const RSS_URLS: &[&str] = &[
    "https://www.minecraft.net/en-us/feed/rss",
    "https://www.minecraft.net/en-us/rss.xml",
];

// ---------------------------------------------------------------- 模组扫描

#[tauri::command]
pub fn mods_security_scan(version_id: String) -> Result<ScanReport, String> {
    let p = paths::paths();
    let vid = version_id.trim();
    // 只读也要防路径穿越：实例 id 只能是普通名字
    if !vid.is_empty()
        && vid != "*"
        && vid != "all"
        && (vid.contains('/') || vid.contains('\\') || vid.contains(".."))
    {
        return Err("实例 id 不合法".into());
    }

    let mut dirs: Vec<PathBuf> = Vec::new();
    if vid.is_empty() || vid == "*" || vid == "all" {
        if let Ok(rd) = std::fs::read_dir(&p.versions_dir) {
            for entry in rd.flatten() {
                let mods = entry.path().join("mods");
                if mods.is_dir() {
                    dirs.push(mods);
                }
            }
        }
    } else {
        let mods = p.versions_dir.join(vid).join("mods");
        if mods.is_dir() {
            dirs.push(mods);
        }
    }

    let mut results = Vec::new();
    let mut scanned = 0usize;
    for dir in &dirs {
        scan_mods_dir(dir, &mut results, &mut scanned);
    }
    results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.file.cmp(&b.file)));
    Ok(ScanReport { results, scanned })
}

fn scan_mods_dir(dir: &Path, out: &mut Vec<ModScanResult>, scanned: &mut usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "jar" && ext != "zip" {
            continue;
        }
        *scanned += 1;
        out.push(scan_one(&path));
    }
}

fn scan_one(path: &Path) -> ModScanResult {
    let mut r = ModScanResult {
        file: store::file_name(&path.to_string_lossy()),
        path: path.to_string_lossy().to_string(),
        ..Default::default()
    };
    if let Ok(bytes) = std::fs::read(path) {
        r.sha256 = hex_sha256(&bytes);
    }

    let lower = r.file.to_lowercase();
    for &bad in KNOWN_BAD {
        if lower.contains(bad) {
            r.findings.push(Finding {
                severity: "medium".into(),
                title: "命中已知风险模组名".into(),
                detail: format!("文件名包含“{bad}”（仅作提示，不是杀毒；同名不代表一定有毒）"),
            });
        }
    }

    let Ok(file) = std::fs::File::open(path) else {
        return r;
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return r;
    };

    let total = archive.len().min(MAX_ENTRIES);
    let mut suspicious_hits = 0usize;
    let mut suspicious_sample = String::new();
    let mut exec_files: Vec<String> = Vec::new();
    let mut obf_names: Vec<String> = Vec::new();
    let mut max_b64 = 0usize;

    for i in 0..total {
        let entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.name().replace('\\', "/");
        let lname = name.to_lowercase();

        // ① 可执行文件
        if EXEC_EXTS.iter().any(|e| lname.ends_with(e)) && exec_files.len() < 5 {
            exec_files.push(name.clone());
        }
        // ④ 极短且高度混淆的类名
        if looks_obfuscated_class(&lname) && obf_names.len() < 5 {
            obf_names.push(name.clone());
        }

        // 目录与超大条目直接跳过（防 zip bomb）
        if entry.is_dir() || entry.size() > MAX_ENTRY_BYTES {
            continue;
        }
        let mut buf = Vec::new();
        if entry.take(MAX_ENTRY_BYTES).read_to_end(&mut buf).is_err() {
            continue;
        }

        // ④ 超长 base64 字符串
        let run = longest_b64_run(&buf);
        if run > max_b64 {
            max_b64 = run;
        }

        // ① 可疑 API 调用（文本与 .class 二进制都按 ASCII 字节搜）
        for &(needle, note) in SUSPICIOUS {
            if contains_ascii(&buf, needle) {
                suspicious_hits += 1;
                if suspicious_sample.is_empty() {
                    suspicious_sample = format!("{name} → {note}");
                }
            }
        }
    }

    if suspicious_hits > 0 {
        r.findings.push(Finding {
            severity: "high".into(),
            title: "发现可疑 API 调用".into(),
            detail: format!("共 {suspicious_hits} 处，例如 {suspicious_sample}（仅作启发式提示）"),
        });
    }
    if !exec_files.is_empty() {
        r.findings.push(Finding {
            severity: "high".into(),
            title: "jar 内含可执行文件".into(),
            detail: format!("{} 等（仅作启发式提示）", exec_files.join("、")),
        });
    }
    if !obf_names.is_empty() {
        r.findings.push(Finding {
            severity: "low".into(),
            title: "疑似混淆的极短类名".into(),
            detail: format!("{} 等（仅作启发式提示）", obf_names.join("、")),
        });
    }
    if max_b64 >= 512 {
        r.findings.push(Finding {
            severity: "medium".into(),
            title: "发现超长 base64 字符串".into(),
            detail: format!("最长连续 {max_b64} 字符，可能隐藏载荷（仅作启发式提示）"),
        });
    }

    let mut score = 0i32;
    for f in &r.findings {
        score += match f.severity.as_str() {
            "high" => 40,
            "medium" => 15,
            "low" => 5,
            _ => 0,
        };
    }
    r.score = score.min(100);
    r
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in out {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn contains_ascii(hay: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    if n.is_empty() || hay.len() < n.len() {
        return false;
    }
    hay.windows(n.len()).any(|w| w == n)
}

/// 极短（≤2 字符）或只由易混淆字符组成的类名，视为疑似混淆。
fn looks_obfuscated_class(lname: &str) -> bool {
    if !lname.ends_with(".class") {
        return false;
    }
    let stem = lname.rsplit('/').next().unwrap_or(lname);
    let stem = stem.trim_end_matches(".class");
    if stem.is_empty() {
        return false;
    }
    if stem.chars().count() <= 2 {
        return true;
    }
    let confusable = "o0li1|";
    stem.chars().all(|c| confusable.contains(c))
}

fn longest_b64_run(bytes: &[u8]) -> usize {
    let mut best = 0usize;
    let mut cur = 0usize;
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=' {
            cur += 1;
            if cur > best {
                best = cur;
            }
        } else {
            cur = 0;
        }
    }
    best
}

// ---------------------------------------------------------------- MC 新闻

#[tauri::command]
pub async fn mc_news() -> NewsResult {
    news_impl().await
}

#[tauri::command]
pub async fn mc_news_refresh() -> NewsResult {
    news_impl().await
}

async fn news_impl() -> NewsResult {
    match fetch_news().await {
        Ok(items) if !items.is_empty() => {
            let now = store::now_secs();
            let cache = NewsCache {
                items: items.clone(),
                updated_at: now,
            };
            let _ = store::save("mc-news-cache", &cache);
            NewsResult {
                items,
                cached: false,
                demo: false,
                updated_at: now,
            }
        }
        _ => from_cache_or_demo(),
    }
}

async fn fetch_news() -> Result<Vec<NewsItem>, String> {
    let client = net::client();
    let mut last = String::new();
    for &url in RSS_URLS {
        let resp = client
            .get(url)
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => {
                let text = r.text().await.unwrap_or_default();
                let items = parse_rss(&text);
                if !items.is_empty() {
                    return Ok(items);
                }
                last = "RSS 中没有可解析的条目".into();
            }
            Ok(r) => last = format!("HTTP {}", r.status().as_u16()),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

fn from_cache_or_demo() -> NewsResult {
    let cache: NewsCache = store::load("mc-news-cache");
    if cache.items.is_empty() {
        NewsResult {
            items: demo_items(),
            cached: false,
            demo: true,
            updated_at: 0,
        }
    } else {
        NewsResult {
            items: cache.items,
            cached: true,
            demo: false,
            updated_at: cache.updated_at,
        }
    }
}

fn demo_items() -> Vec<NewsItem> {
    let home = "https://www.minecraft.net/";
    vec![
        NewsItem {
            title: "离线示例：联网后会显示官方新闻".into(),
            link: home.into(),
            pub_date: "".into(),
            description: "当前离线且没有本地缓存，这是一条“离线示例”，不是真实新闻。联网后点“刷新”即可获取 Minecraft 官方动态。".into(),
        },
        NewsItem {
            title: "离线示例：查看模组安全提示".into(),
            link: home.into(),
            pub_date: "".into(),
            description: "在“恶意模组扫描”里选择实例即可开始只读扫描；结果只是启发式提示，不能替代杀毒软件。".into(),
        },
        NewsItem {
            title: "离线示例：缓存机制说明".into(),
            link: home.into(),
            pub_date: "".into(),
            description: "成功抓取的新闻会缓存在本地；断网时会自动展示最近一次缓存并标注“缓存”。".into(),
        },
    ]
}

/// 极简 RSS 解析：只取 <item> 的四个字段，去 CDATA / HTML 标签，最多 MAX_NEWS 条。
fn parse_rss(xml: &str) -> Vec<NewsItem> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(pos) = rest.find("<item") {
        let after_open = &rest[pos..];
        // 兼容 <item> 与 <item attr="…">
        let Some(gt) = after_open.find('>') else {
            break;
        };
        let body_start = gt + 1;
        let Some(end) = after_open[body_start..].find("</item>") else {
            break;
        };
        let body_end = body_start + end;
        let block = &after_open[body_start..body_end];

        let item = NewsItem {
            title: rss_tag(block, "title"),
            link: rss_tag(block, "link"),
            pub_date: rss_tag(block, "pubDate"),
            description: rss_tag(block, "description"),
        };
        if !item.title.is_empty() {
            out.push(item);
        }
        if out.len() >= MAX_NEWS {
            break;
        }
        rest = &after_open[body_end + "</item>".len()..];
    }
    out
}

fn rss_tag(block: &str, name: &str) -> String {
    let open = format!("<{name}");
    let Some(p) = block.find(&open) else {
        return String::new();
    };
    let after_open = &block[p + open.len()..];
    let Some(gt) = after_open.find('>') else {
        return String::new();
    };
    let content = &after_open[gt + 1..];
    let close = format!("</{name}>");
    let Some(end) = content.find(&close) else {
        return String::new();
    };
    clean_xml(&content[..end])
}

fn clean_xml(s: &str) -> String {
    let s = s.trim();
    // CDATA 里是原样文本，不能当 HTML 标签去掉；普通字段先解码实体再去掉 HTML 标签。
    let (text, strip_tags) = if s.starts_with("<![CDATA[") && s.ends_with("]]>") && s.len() >= 12 {
        (&s[9..s.len() - 3], false)
    } else {
        (s, true)
    };
    let decoded = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    let plain = if strip_tags {
        let mut out = String::new();
        let mut in_tag = false;
        for c in decoded.chars() {
            if c == '<' {
                in_tag = true;
            } else if c == '>' {
                in_tag = false;
            } else if !in_tag {
                out.push(c);
            }
        }
        out
    } else {
        decoded
    };
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------- 测试

#[cfg(test)]
mod tests {
    use super::{clean_xml, contains_ascii, longest_b64_run, parse_rss};

    #[test]
    fn searches_ascii_bytes() {
        assert!(contains_ascii(
            b"a java/lang/Runtime b",
            "java/lang/Runtime"
        ));
        assert!(!contains_ascii(b"a runtime b", "java/lang/Runtime"));
    }

    #[test]
    fn measures_base64_run() {
        assert_eq!(longest_b64_run(b"ab!!YWJjZA==!!x"), 8);
    }

    #[test]
    fn parses_rss_items() {
        let xml = r#"<?xml version="1.0"?><rss><channel>
          <item><title><![CDATA[Hello World]]></title><link>https://mc.net/a</link>
          <pubDate>Tue, 01 Jan 2026 00:00:00 GMT</pubDate>
          <description>&lt;p&gt;A &amp; B&lt;/p&gt;</description></item>
          <item><title>Second</title><link>https://mc.net/b</link></item>
        </channel></rss>"#;
        let items = parse_rss(xml);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Hello World");
        assert_eq!(items[0].description, "A & B");
        assert_eq!(items[0].link, "https://mc.net/a");
    }

    #[test]
    fn cleans_cdata_and_entities() {
        assert_eq!(clean_xml("<![CDATA[  A &amp; B  ]]>"), "A & B");
    }
}

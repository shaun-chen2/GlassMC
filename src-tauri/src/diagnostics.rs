//! 诊断：崩溃通俗解释、日志导出分享、冗余清理。
//!
//! 设计原则：
//! - 崩溃解释只读日志，不写任何文件；匹配不到特征时给通用排查步骤。
//! - 日志导出只收最近 7 天内容，单文件超过 20 MB 直接跳过，避免打出一个几百 MB 的包。
//! - 清理分「先扫描、再勾选、后删除」两步：`cleanup_scan` 把可删项记到本地 store，
//!   `cleanup_apply` 只处理 store 里存在且 id 被勾选的项；绝不自动删除，也绝不递归删目录。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::{paths, store};

const CLEANUP_KEY: &str = "diagnostics_cleanup";
const MAX_EXPORT_FILE: u64 = 20 * 1024 * 1024;
const SEVEN_DAYS: i64 = 7 * 24 * 3600;

// ---------------------------------------------------------------- 数据结构

/// `code` 是给前端做多语言的稳定标识；title/reason/suggestions 是中文兜底文案。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashExplain {
    pub title: String,
    pub reason: String,
    pub suggestions: Vec<String>,
    pub excerpt: String,
    pub source: String,
    pub at: i64,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItem {
    pub id: String,
    pub path: String,
    pub size: u64,
    pub kind: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupFailure {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub removed: usize,
    pub failed: Vec<CleanupFailure>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanupStore {
    items: Vec<CleanupItem>,
}

// ---------------------------------------------------------------- 小工具

fn mtime(path: &Path) -> Option<i64> {
    let t = std::fs::metadata(path).ok()?.modified().ok()?;
    t.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn add_instance_dir(out: &mut Vec<(String, PathBuf)>, label: String, dir: PathBuf) {
    if !dir.is_dir() {
        return;
    }
    if out.iter().any(|(_, p)| *p == dir) {
        return;
    }
    out.push((label, dir));
}

/// 实例的 game 目录：隔离实例在 `versions/<id>`，共享实例是 `game_dir`。
/// 两个都列进去可以避免依赖别的模块，靠去重与目录名跳过防止重复扫描。
fn instance_dirs(version_id: &str) -> Vec<(String, PathBuf)> {
    let p = paths::paths();
    let mut out = Vec::new();
    if version_id.trim().is_empty() {
        add_instance_dir(&mut out, "shared".to_string(), p.game_dir.clone());
        if let Ok(rd) = std::fs::read_dir(&p.versions_dir) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    let name = e.file_name().to_string_lossy().to_string();
                    add_instance_dir(&mut out, name, e.path());
                }
            }
        }
    } else {
        let id = version_id.trim();
        add_instance_dir(&mut out, id.to_string(), p.versions_dir.join(id));
        add_instance_dir(&mut out, id.to_string(), p.game_dir.clone());
    }
    out
}

/// UTC 时间戳 `YYYYMMDD-HHMMSS`，用于给导出的 zip 命名。
fn compact_stamp() -> String {
    let secs = store::now_secs().max(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// Howard Hinnant 的 days -> (year, month, day) 算法，公历，不依赖 chrono。
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    if m <= 2 {
        y += 1;
    }
    (y, m, d)
}

fn push_item(out: &mut Vec<CleanupItem>, kind: &str, path: &Path, size: u64, description: String) {
    let id = format!(
        "{}-{:x}",
        kind,
        md5::compute(path.to_string_lossy().as_bytes())
    );
    if out.iter().any(|x| x.id == id) {
        return;
    }
    out.push(CleanupItem {
        id,
        path: path.to_string_lossy().to_string(),
        size,
        kind: kind.to_string(),
        description,
    });
}

// ---------------------------------------------------------------- 1. 崩溃解释

struct Rule {
    needles: &'static [&'static str],
    code: &'static str,
    title: &'static str,
    reason: &'static str,
    suggestions: &'static [&'static str],
}

static RULES: &[Rule] = &[
    Rule {
        needles: &["outofmemoryerror", "java heap space", "gc overhead limit exceeded"],
        code: "oom",
        title: "内存不足（OutOfMemory）",
        reason: "Java 虚拟机在加载世界或模组时内存耗尽，游戏被强制退出。常见原因是分配内存太小，或模组、高清材质占用过高。",
        suggestions: &[
            "在设置里把最大内存调到 4–8 GB；32 位 Java 无法使用超过约 1.5 GB，请换 64 位",
            "减少大型模组、高清材质包，或降低视距与模拟距离",
            "关闭浏览器等占内存的程序后再启动",
        ],
    },
    Rule {
        needles: &[
            "already on the jvms module path",
            "already on the jvm's module path",
            "duplicate module",
            "module named",
        ],
        code: "moduleConflict",
        title: "模组库版本冲突",
        reason: "两个模组各自带了一份同名库，在 Java 模块路径上重复加载导致崩溃。通常是同一个库的不同版本被重复打包。",
        suggestions: &[
            "把冲突的两个模组更新到互相兼容的版本",
            "删除 mods 里重复的库类 jar（同一个库的多个版本只留一个）",
            "用「清理冗余」扫描重复模组，勾选后删除多出来的那份",
        ],
    },
    Rule {
        needles: &["duplicatemodsfoundexception", "duplicate mods found", "duplicate mod"],
        code: "duplicateMod",
        title: "重复模组",
        reason: "同一个模组被放进 mods 目录两次，加载器拒绝同时加载两份。",
        suggestions: &[
            "在 mods 目录里保留一个 jar，删掉重复的那份",
            "使用「清理冗余」扫描内容完全相同的重复 jar",
        ],
    },
    Rule {
        needles: &[
            "missingmodsexception",
            "modloadingexception",
            "missing mod",
            "required mod",
            "missing dependency",
        ],
        code: "missingMod",
        title: "缺少前置模组",
        reason: "某个模组依赖的前置库没有安装或版本不匹配，加载器在启动阶段就中止了。",
        suggestions: &[
            "看崩溃信息里列出的缺失模组名，下载并安装对应前置",
            "确认前置的版本与游戏版本、加载器版本匹配",
            "已安装的话检查文件是否完整，或文件名是不是被改成了 .disabled",
        ],
    },
    Rule {
        needles: &[
            "unsupportedclassversionerror",
            "class file version",
            "compiled by a more recent version",
        ],
        code: "javaVersion",
        title: "Java 版本太低",
        reason: "游戏或模组是用更高版本的 Java 编译的，当前使用的 Java 读不了这个 class 文件。",
        suggestions: &[
            "在设置里切换到 Java 17（1.18 及以上）或 Java 21（部分新模组）",
            "给不同游戏版本分别安装对应的 Java，启动器会自动挑选",
            "切换后重启启动器再试",
        ],
    },
    Rule {
        needles: &[
            "could not find or load main class",
            "net.minecraft.client.main.main",
            "classnotfoundexception",
        ],
        code: "mainClass",
        title: "游戏主类 / 继承链损坏",
        reason: "启动器找不到游戏主类，通常是客户端 jar 缺失，或版本继承链（inheritsFrom）被破坏。",
        suggestions: &[
            "在版本页重新安装该版本，修复客户端 jar",
            "检查这个版本的 JSON 继承链，确认父版本还在",
            "检查 libraries 目录是否缺少依赖文件",
        ],
    },
    Rule {
        needles: &[
            "glfw",
            "opengl",
            "lwjgl",
            "failed to create window",
            "no opengl context",
        ],
        code: "graphics",
        title: "显卡驱动 / 图形环境问题",
        reason: "游戏无法初始化 OpenGL 图形上下文，多见于显卡驱动过旧、远程桌面或虚拟机环境，以及 LWJGL 原生库缺失。",
        suggestions: &[
            "把显卡驱动更新到最新版本",
            "退出远程桌面 / 虚拟机的 3D 限制，尽量在原生桌面环境运行",
            "确认 natives 目录里的 LWJGL 原生库完整，可重装版本修复",
        ],
    },
    Rule {
        needles: &[
            "connection refused",
            "connectexception",
            "unknownhostexception",
            "http 4",
            "http 5",
            "timed out",
        ],
        code: "network",
        title: "网络或镜像源异常",
        reason: "下载资源或连接服务器时被拒绝、超时，或返回了 4xx/5xx 错误，通常是网络不通、镜像失效或被防火墙拦截。",
        suggestions: &[
            "检查网络连接，必要时换网络或配置代理",
            "在设置里更换下载镜像源",
            "检查防火墙 / 杀毒软件是否拦截了启动器或 Java 的联网",
        ],
    },
];

static FALLBACK: Rule = Rule {
    needles: &[],
    code: "unknown",
    title: "暂时没认出这个错误",
    reason: "日志里没有匹配到常见错误特征，需要结合原始堆栈再判断。",
    suggestions: &[
        "展开下面的原始摘录，找最后一处 Exception / Error 关键词",
        "确认游戏版本、加载器和模组版本互相兼容",
        "移掉最近新增的模组后再启动一次",
        "拿错误信息到模组作者或社区渠道搜索",
    ],
};

fn classify(text: &str) -> (&'static Rule, Vec<String>) {
    let lower = text.to_ascii_lowercase();
    for rule in RULES {
        if rule
            .needles
            .iter()
            .any(|n| lower.contains(&n.to_ascii_lowercase()))
        {
            return (
                rule,
                rule.suggestions.iter().map(|s| s.to_string()).collect(),
            );
        }
    }
    (
        &FALLBACK,
        FALLBACK.suggestions.iter().map(|s| s.to_string()).collect(),
    )
}

fn build_excerpt(lines: &[&str]) -> String {
    let keys = [
        "exception",
        "error",
        "caused by",
        "\tat ",
        "module",
        "missing",
        "glfw",
        "lwjgl",
        "opengl",
        "duplicate",
        "unsupported",
        "failed",
    ];
    let picked: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            let ll = l.to_ascii_lowercase();
            keys.iter().any(|k| ll.contains(k))
        })
        .collect();
    let src: &[&str] = if picked.len() >= 4 { &picked } else { lines };
    let start = src.len().saturating_sub(60);
    src[start..].join("\n")
}

/// 取最后 8 MB 就够覆盖堆栈了；日志动辄几百 MB，不整文件读进内存。
fn read_tail(path: &Path) -> Result<String, String> {
    let len = file_size(path);
    let take = len.min(8 * 1024 * 1024);
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if take > 0 {
        f.seek(SeekFrom::End(-(take as i64)))
            .map_err(|e| e.to_string())?;
    }
    let mut buf = Vec::with_capacity(take as usize + 1);
    f.take(take)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

#[tauri::command]
pub fn crash_explain(version_id: String) -> Result<CrashExplain, String> {
    let mut sources: Vec<(PathBuf, i64)> = Vec::new();

    let latest = paths::paths().log_dir.join("game-latest.log");
    if let Some(t) = mtime(&latest) {
        sources.push((latest, t));
    }
    for (_, dir) in instance_dirs(&version_id) {
        let reports = dir.join("crash-reports");
        if let Ok(rd) = std::fs::read_dir(&reports) {
            for e in rd.flatten() {
                let p = e.path();
                let is_txt = p
                    .extension()
                    .map(|x| x.eq_ignore_ascii_case("txt"))
                    .unwrap_or(false);
                if is_txt {
                    if let Some(t) = mtime(&p) {
                        sources.push((p, t));
                    }
                }
            }
        }
    }

    let Some((source, at)) = sources.into_iter().max_by_key(|x| x.1) else {
        return Ok(CrashExplain {
            title: "没找到崩溃日志".to_string(),
            reason: "没有发现游戏日志或 crash-reports 报告，暂时无从分析。".to_string(),
            suggestions: vec![
                "先启动一次游戏并复现问题，再回来分析".to_string(),
                "确认选的是出问题的那个实例".to_string(),
                "也可以直接用「日志导出」把日志发给别人帮忙看".to_string(),
            ],
            excerpt: String::new(),
            source: String::new(),
            at: 0,
            code: "none".to_string(),
        });
    };

    let text = read_tail(&source)?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(200);
    let window = &lines[start..];
    let (rule, suggestions) = classify(&text);
    let excerpt = build_excerpt(window);

    Ok(CrashExplain {
        title: rule.title.to_string(),
        reason: rule.reason.to_string(),
        suggestions,
        excerpt,
        source: source.to_string_lossy().to_string(),
        at,
        code: rule.code.to_string(),
    })
}

// ---------------------------------------------------------------- 2. 日志导出

fn collect_export_file(path: &Path, zip: &mut zip::ZipWriter<std::fs::File>, name: &str) -> bool {
    let size = file_size(path);
    if size == 0 || size > MAX_EXPORT_FILE {
        return false;
    }
    let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let mut f = match std::fs::File::open(path) {
        Ok(v) => v,
        Err(_) => return false,
    };
    if zip.start_file(name, opts).is_err() {
        return false;
    }
    let mut buf = Vec::with_capacity(size as usize);
    if f.read_to_end(&mut buf).is_err() {
        return false;
    }
    zip.write_all(&buf).is_ok()
}

/// 收某个目录下最近 7 天的文件，`prefix` 是 zip 里的目录前缀。
fn collect_recent(dir: &Path, prefix: &str, zip: &mut zip::ZipWriter<std::fs::File>) -> usize {
    let cutoff = store::now_secs() - SEVEN_DAYS;
    let mut count = 0;
    let entries = match std::fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return 0,
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        if mtime(&p).unwrap_or(0) < cutoff {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let entry = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if collect_export_file(&p, zip, &entry) {
            count += 1;
        }
    }
    count
}

#[tauri::command]
pub fn logs_export(version_id: String) -> Result<String, String> {
    let download = dirs::download_dir().unwrap_or_else(|| paths::paths().data_root.clone());
    let _ = std::fs::create_dir_all(&download);
    let out = download.join(format!("GlassMC-logs-{}.zip", compact_stamp()));

    let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let mut count = 0usize;

    // 全局日志（含 game-latest.log）
    count += collect_recent(&paths::paths().log_dir, "logs", &mut zip);

    // 所选实例的 crash-reports 全部收进来（崩溃报告通常不大，且越旧越有参考价值）
    for (label, dir) in instance_dirs(&version_id) {
        let reports = dir.join("crash-reports");
        let prefix = format!("{}/crash-reports", store::file_name(&label));
        let entries = match std::fs::read_dir(&reports) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        for e in entries.flatten() {
            let p = e.path();
            if !p.is_file() || file_size(&p) > MAX_EXPORT_FILE {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            let mut f = match std::fs::File::open(&p) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if zip.start_file(format!("{prefix}/{name}"), opts).is_err() {
                continue;
            }
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_ok() && zip.write_all(&buf).is_ok() {
                count += 1;
            }
        }
        // 隔离实例自己的 logs/ 也一起带上
        count += collect_recent(
            &dir.join("logs"),
            &format!("{}/logs", store::file_name(&label)),
            &mut zip,
        );
    }

    zip.finish().map_err(|e| e.to_string())?;
    if count == 0 {
        let _ = std::fs::remove_file(&out);
        return Err("最近 7 天没有可导出的日志或崩溃报告".to_string());
    }
    Ok(out.to_string_lossy().to_string())
}

#[tauri::command]
pub fn logs_reveal(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let parent = p
        .parent()
        .filter(|x| !x.as_os_str().is_empty())
        .unwrap_or(&p)
        .to_path_buf();
    tauri_plugin_opener::open_path(parent, None::<&str>).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- 3. 清理冗余

fn scan_duplicates(dir: &Path, out: &mut Vec<CleanupItem>) {
    let mut groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return,
    };
    for e in entries.flatten() {
        let p = e.path();
        let is_jar = p
            .extension()
            .map(|x| x.eq_ignore_ascii_case("jar"))
            .unwrap_or(false);
        if is_jar {
            if let Ok(h) = sha256_file(&p) {
                groups.entry(h).or_default().push(p);
            }
        }
    }
    for (_, mut files) in groups {
        if files.len() < 2 {
            continue;
        }
        files.sort();
        let keep = store::file_name(&files[0].to_string_lossy());
        for p in files.into_iter().skip(1) {
            push_item(
                out,
                "duplicate",
                &p,
                file_size(&p),
                format!("内容与 {keep} 完全相同，保留其中一个即可"),
            );
        }
    }
}

fn scan_temp(dir: &Path, out: &mut Vec<CleanupItem>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return,
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let low = name.to_ascii_lowercase();
        if low.ends_with(".part") || low.ends_with(".tmp") || low.ends_with(".disabled") {
            push_item(
                out,
                "temp",
                &p,
                file_size(&p),
                "下载中断或被禁用的临时文件，确认后可以直接删除".to_string(),
            );
        }
    }
}

fn collect_old_files(dir: &Path, cutoff: i64, kind: &str, out: &mut Vec<CleanupItem>, crash: bool) {
    let entries = match std::fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return,
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        if crash {
            let is_txt = p
                .extension()
                .map(|x| x.eq_ignore_ascii_case("txt"))
                .unwrap_or(false);
            if !is_txt {
                continue;
            }
        }
        if mtime(&p).unwrap_or(0) >= cutoff {
            continue;
        }
        let desc = if crash {
            "7 天前的崩溃报告，问题已处理即可删除"
        } else {
            "7 天前的旧日志，导出备份后即可删除"
        };
        push_item(out, kind, &p, file_size(&p), desc.to_string());
    }
}

const WALK_SKIP: &[&str] = &["libraries", "assets", "versions", ".git", "node_modules"];

fn walk_junk(root: &Path, dir: &Path, out: &mut Vec<CleanupItem>, depth: usize) {
    if depth > 12 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return,
    };
    let mut any = false;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        any = true;
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if name == "__pycache__" {
                // 不递归删目录：把里面的文件逐个列出来，由用户勾选后逐个 remove_file。
                if let Ok(inner) = std::fs::read_dir(&p) {
                    for f in inner.flatten() {
                        let fp = f.path();
                        if fp.is_file() {
                            push_item(
                                out,
                                "junk",
                                &fp,
                                file_size(&fp),
                                "__pycache__ 缓存文件，删除不影响游戏".to_string(),
                            );
                        }
                    }
                }
                continue;
            }
            if WALK_SKIP.contains(&name.as_str()) {
                continue;
            }
            subdirs.push(p);
        } else if p.is_file() && name == ".DS_Store" {
            push_item(
                out,
                "junk",
                &p,
                file_size(&p),
                "macOS 生成的 .DS_Store，可以安全删除".to_string(),
            );
        }
    }
    if !any && dir != root {
        push_item(
            out,
            "emptyDir",
            dir,
            0,
            "空目录，删除不影响任何数据".to_string(),
        );
    }
    for s in subdirs {
        walk_junk(root, &s, out, depth + 1);
    }
}

#[tauri::command]
pub fn cleanup_scan(version_id: String) -> Result<Vec<CleanupItem>, String> {
    let mut out = Vec::new();
    let cutoff = store::now_secs() - SEVEN_DAYS;

    collect_old_files(&paths::paths().log_dir, cutoff, "log", &mut out, false);

    for (_, dir) in instance_dirs(&version_id) {
        let mods = dir.join("mods");
        scan_duplicates(&mods, &mut out);
        scan_temp(&mods, &mut out);
        collect_old_files(&dir.join("logs"), cutoff, "log", &mut out, false);
        collect_old_files(&dir.join("crash-reports"), cutoff, "crash", &mut out, true);
        walk_junk(&dir, &dir, &mut out, 0);
    }

    out.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.path.cmp(&b.path)));
    // 把本次扫描结果记下来：cleanup_apply 只认这里的 id，防止误删任意路径。
    let _ = store::save(CLEANUP_KEY, &CleanupStore { items: out.clone() });
    Ok(out)
}

#[tauri::command]
pub fn cleanup_apply(ids: Vec<String>) -> Result<CleanupResult, String> {
    let scanned: CleanupStore = store::load(CLEANUP_KEY);
    let wanted: HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let root = paths::paths().data_root.clone();
    let mut removed = 0usize;
    let mut failed = Vec::new();
    let mut remain = Vec::new();

    for item in scanned.items {
        if !wanted.contains(item.id.as_str()) {
            remain.push(item);
            continue;
        }
        let path = PathBuf::from(&item.path);
        if !path.starts_with(&root) {
            failed.push(CleanupFailure {
                id: item.id,
                reason: "路径不在 GlassMC 数据目录内，已跳过".to_string(),
            });
            continue;
        }
        let result = if item.kind == "emptyDir" {
            std::fs::remove_dir(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match result {
            Ok(()) => removed += 1,
            Err(e) => failed.push(CleanupFailure {
                id: item.id,
                reason: e.to_string(),
            }),
        }
    }

    let _ = store::save(CLEANUP_KEY, &CleanupStore { items: remain });
    Ok(CleanupResult { removed, failed })
}

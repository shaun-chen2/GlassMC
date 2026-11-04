//! 模组加载器安装：Fabric / Quilt 直接落 profile.json，NeoForge / Forge 跑官方安装器。

use std::path::PathBuf;

use crate::config::Config;
use crate::download;
use crate::install::{self, Progress};
use crate::java;
use crate::manifest;
use crate::paths;

const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
const NEOFORGE_MIRROR: &str = "https://bmclapi2.bangbang93.com/neoforge/maven";
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";

#[allow(dead_code)]
pub const KINDS: [&str; 4] = ["fabric", "quilt", "neoforge", "forge"];

pub fn display(kind: &str) -> &'static str {
    match kind {
        "fabric" => "Fabric",
        "quilt" => "Quilt",
        "neoforge" => "NeoForge",
        "forge" => "Forge",
        _ => "加载器",
    }
}

/// 该游戏版本可安装的加载器版本（新的在前）。
pub async fn versions(kind: &str, game: &str) -> Result<Vec<String>, String> {
    match kind {
        "fabric" => {
            meta_versions(&format!(
                "https://meta.fabricmc.net/v2/versions/loader/{game}"
            ))
            .await
        }
        "quilt" => {
            meta_versions(&format!(
                "https://meta.quiltmc.org/v3/versions/loader/{game}"
            ))
            .await
        }
        "neoforge" => neoforge_versions(game).await,
        "forge" => forge_versions(game).await,
        other => Err(format!("未知的加载器：{other}")),
    }
}

async fn meta_versions(url: &str) -> Result<Vec<String>, String> {
    let body = download::text(url).await?;
    let list: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("加载器版本列表解析失败：{e}"))?;
    let mut out: Vec<String> = list
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    x.get("loader")
                        .and_then(|l| l.get("version"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    if out.is_empty() {
        return Err("该游戏版本没有可用的加载器版本".into());
    }
    out.sort_by(|a, b| {
        if is_newer(a, b) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    Ok(out)
}

async fn neoforge_versions(game: &str) -> Result<Vec<String>, String> {
    let xml = download::text(&format!(
        "{NEOFORGE_MAVEN}/net/neoforged/neoforge/maven-metadata.xml"
    ))
    .await?;
    let all = xml_versions(&xml);
    if all.is_empty() {
        return Err("NeoForge 元数据里没有版本号".into());
    }
    let prefix = neoforge_prefix(game)
        .ok_or_else(|| format!("无法由游戏版本 {game} 推导 NeoForge 版本前缀"))?;
    let mut matched: Vec<String> = all
        .into_iter()
        .filter(|v| v.starts_with(&prefix) && !v.contains("-beta") && !v.contains("-alpha"))
        .collect();
    if matched.is_empty() {
        return Err(format!(
            "NeoForge 没有适用于 {game} 的版本（需要 {prefix}x）"
        ));
    }
    matched.sort_by(|a, b| {
        if is_newer(a, b) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    Ok(matched)
}

async fn forge_versions(game: &str) -> Result<Vec<String>, String> {
    let xml = download::text(&format!(
        "{FORGE_MAVEN}/net/minecraftforge/forge/maven-metadata.xml"
    ))
    .await?;
    let prefix = format!("{game}-");
    let mut matched: Vec<String> = xml_versions(&xml)
        .into_iter()
        .filter_map(|v| v.strip_prefix(&prefix).map(|s| s.to_string()))
        .filter(|s| !s.is_empty())
        .collect();
    if matched.is_empty() {
        return Err(format!("Forge 没有适用于 {game} 的版本"));
    }
    matched.sort_by(|a, b| {
        if is_newer(a, b) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    Ok(matched)
}

/// 安装加载器，返回装好的版本 id。
pub async fn install(
    app: Option<tauri::AppHandle>,
    kind: &str,
    game: &str,
    loader_version: &str,
    cfg: &Config,
) -> Result<String, String> {
    let prog = Progress::new(app.clone(), format!("loader:{kind}"));
    prog.note(&format!("准备安装 {} {}", display(kind), loader_version));

    let version_id = if kind == "fabric" || kind == "quilt" {
        install_profile(kind, game, loader_version, &prog).await?
    } else {
        install_with_installer(kind, game, loader_version, cfg, &prog).await?
    };

    prog.note("补齐加载器依赖");
    let outcome = install::install_version(app, &version_id, cfg).await?;
    if !outcome.failed.is_empty() {
        println!("[loader] {} 个文件下载失败", outcome.failed.len());
    }
    Ok(version_id)
}

async fn install_profile(
    kind: &str,
    game: &str,
    loader_version: &str,
    prog: &Progress,
) -> Result<String, String> {
    let base = if kind == "fabric" {
        "https://meta.fabricmc.net/v2/versions/loader"
    } else {
        "https://meta.quiltmc.org/v3/versions/loader"
    };
    prog.stage("获取加载器配置", 1, 0.0, 0.2);
    let body = download::text(&format!("{base}/{game}/{loader_version}/profile/json")).await?;
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("加载器配置解析失败：{e}"))?;
    let profile_id = json
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{} 返回的配置缺少 id", display(kind)))?
        .to_string();

    let dir = paths::paths().versions_dir.join(&profile_id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建版本目录失败：{e}"))?;
    std::fs::write(
        dir.join(format!("{profile_id}.json")),
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入版本文件失败：{e}"))?;
    prog.tick(true);
    Ok(profile_id)
}

async fn install_with_installer(
    kind: &str,
    game: &str,
    loader_version: &str,
    cfg: &Config,
    prog: &Progress,
) -> Result<String, String> {
    let (mirror, official) = if kind == "neoforge" {
        (
            Some(format!(
                "{NEOFORGE_MIRROR}/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
            )),
            format!(
                "{NEOFORGE_MAVEN}/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
            ),
        )
    } else {
        let full = format!("{game}-{loader_version}");
        (
            None,
            format!("{FORGE_MAVEN}/net/minecraftforge/forge/{full}/forge-{full}-installer.jar"),
        )
    };

    let tmp = std::env::temp_dir().join(format!(
        "glassmc-{kind}-{}-{}",
        loader_version.replace(['/', '\\', ':'], "_"),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败：{e}"))?;
    let jar = tmp.join("installer.jar");

    prog.stage(&format!("下载 {} 安装器", display(kind)), 1, 0.0, 0.3);
    let mut urls = Vec::new();
    if let Some(m) = mirror {
        urls.push(m);
    }
    urls.push(official);
    if let Err(e) = download::download_any(&urls, &jar, None).await {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("下载 {} 安装器失败：{e}", display(kind)));
    }
    prog.tick(true);

    let java = java::best_for(21, &cfg.java_path)
        .or_else(|| java::best_for(17, &cfg.java_path))
        .ok_or_else(|| {
            format!(
                "安装 {} 需要 Java 21，请先到设置里装一个 JDK",
                display(kind)
            )
        })?;

    let game_dir = paths::paths().game_dir.clone();
    let before = version_dirs();

    prog.stage(&format!("运行 {} 安装器", display(kind)), 1, 0.3, 0.5);
    prog.note("安装器运行中，可能需要几分钟");
    let java_path = java.path.clone();
    let java_home = java.home.clone();
    let jar_clone = jar.clone();
    let game_clone = game_dir.clone();
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(420),
        tokio::task::spawn_blocking(move || {
            run_installer(&java_path, &java_home, &jar_clone, &game_clone)
        }),
    )
    .await
    .map_err(|_| format!("{} 安装器运行超时（7 分钟）", display(kind)))?
    .map_err(|e| format!("安装器任务失败：{e}"))?;
    let _ = std::fs::remove_dir_all(&tmp);

    let output = output?;
    prog.tick(true);

    // 扫描新增的版本目录
    let after = version_dirs();
    let mut candidates: Vec<String> = after
        .iter()
        .filter(|d| !before.contains(d) && d.as_str() != game)
        .cloned()
        .collect();
    if candidates.is_empty() {
        candidates = after
            .into_iter()
            .filter(|d| d != game && d.contains(loader_version))
            .collect();
    }
    let installed_id = candidates
        .into_iter()
        .filter(|d| {
            paths::paths()
                .versions_dir
                .join(d)
                .join(format!("{d}.json"))
                .is_file()
        })
        .max_by_key(|d| {
            std::fs::metadata(paths::paths().versions_dir.join(d))
                .and_then(|m| m.modified())
                .ok()
        })
        .ok_or_else(|| format!("安装器已结束，但没找到新版本目录。\n{}", tail(&output, 600)))?;

    Ok(installed_id)
}

fn run_installer(
    java: &str,
    java_home: &str,
    jar: &PathBuf,
    game_dir: &PathBuf,
) -> Result<String, String> {
    let mut cmd = std::process::Command::new(java);
    cmd.arg("-jar")
        .arg(jar)
        .arg("--installClient")
        .arg(game_dir)
        .current_dir(game_dir)
        .env("JAVA_HOME", java_home);
    let out = cmd
        .output()
        .map_err(|e| format!("无法启动 Java 安装器：{e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        return Err(format!(
            "安装器退出码 {:?}\n{}",
            out.status.code(),
            tail(&text, 800)
        ));
    }
    Ok(text)
}

fn tail(text: &str, max: usize) -> String {
    let t = text.trim();
    if t.chars().count() <= max {
        return t.to_string();
    }
    let skip = t.chars().count() - max;
    t.chars().skip(skip).collect()
}

fn version_dirs() -> Vec<String> {
    match std::fs::read_dir(paths::paths().versions_dir.clone()) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// `1.21.1` → `21.1.`；`1.21` → `21.0.`
fn neoforge_prefix(game: &str) -> Option<String> {
    let parts: Vec<&str> = game.split('.').collect();
    if parts.len() < 2 || parts[0] != "1" {
        return None;
    }
    let minor: u32 = parts[1].parse().ok()?;
    let patch: u32 = parts.get(2).map(|s| s.parse().ok()).unwrap_or(Some(0))?;
    Some(format!("{minor}.{patch}."))
}

fn xml_versions(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<version>") {
        let after = &rest[start + 9..];
        let Some(end) = after.find("</version>") else {
            break;
        };
        let v = after[..end].trim().to_string();
        if !v.is_empty() && !out.contains(&v) {
            out.push(v);
        }
        rest = &after[end + 10..];
    }
    out
}

fn numeric_parts(v: &str) -> Vec<u64> {
    let mut parts = Vec::new();
    let mut digits = String::new();
    for c in v.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if !digits.is_empty() {
            parts.push(digits.parse().unwrap_or(0));
            digits.clear();
        }
    }
    if !digits.is_empty() {
        parts.push(digits.parse().unwrap_or(0));
    }
    parts
}

pub fn is_newer(a: &str, b: &str) -> bool {
    let (x, y) = (numeric_parts(a), numeric_parts(b));
    for i in 0..x.len().max(y.len()) {
        let l = x.get(i).copied().unwrap_or(0);
        let r = y.get(i).copied().unwrap_or(0);
        if l != r {
            return l > r;
        }
    }
    a > b
}

/// 判断某个版本 id 属于哪种加载器（供界面显示）。
#[allow(dead_code)]
pub fn kind_of(version_id: &str, inherits: Option<&str>) -> Option<String> {
    let combined = format!(
        "{} {}",
        version_id.to_lowercase(),
        inherits.unwrap_or("").to_lowercase()
    );
    if combined.contains("neoforge") {
        Some("neoforge".into())
    } else if combined.contains("fabric") {
        Some("fabric".into())
    } else if combined.contains("quilt") {
        Some("quilt".into())
    } else if combined.contains("forge") {
        Some("forge".into())
    } else {
        None
    }
}

/// 供安装页使用：某个已装版本是否已经装好（有 json 与 jar）。
#[allow(dead_code)]
pub fn is_ready(version_id: &str) -> bool {
    manifest::local_merged(version_id).is_some()
}

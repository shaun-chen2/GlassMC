//! Modrinth 客户端与本地模组管理。
//!
//! 下载后一定校验 SHA1 —— 镜像/代理返回的半截文件如果直接进 mods，
//! 游戏只会抛一个看不懂的 `ZipException`。

use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::download;
use crate::manifest;
use crate::paths;

const API: &str = "https://api.modrinth.com/v2";
const UA: &str = "GlassMC/1.2 (Tauri; +https://github.com/shaun-chen2/GlassMC)";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub icon_url: String,
    pub downloads: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MrFile {
    pub filename: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MrDependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub kind: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MrVersion {
    pub id: String,
    pub version_number: String,
    pub name: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<MrFile>,
    pub dependencies: Vec<MrDependency>,
    pub date_published: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub file_name: String,
    pub path: String,
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub size: u64,
    pub enabled: bool,
}

async fn get(url: &str) -> Result<serde_json::Value, String> {
    let resp = crate::net::client()
        .get(url)
        .header("User-Agent", UA)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("请求 Modrinth 失败：{e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        let hint = match status {
            400 => "查询参数有误",
            401 | 403 => "访问被拒绝，请稍后重试",
            404 => "找不到该项目或版本",
            429 => "请求过于频繁，请稍后再试",
            _ => "服务端错误",
        };
        return Err(format!("Modrinth {status}：{hint}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("Modrinth 返回内容解析失败：{e}"))
}

fn query_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub async fn search(
    query: &str,
    game: &str,
    loader: &str,
    offset: u32,
    limit: u32,
) -> Result<(Vec<SearchHit>, u32), String> {
    let mut facets: Vec<String> = Vec::new();
    if !game.trim().is_empty() {
        facets.push(format!("[\"versions:{game}\"]"));
    }
    if !loader.trim().is_empty() {
        facets.push(format!("[\"categories:{loader}\"]"));
    }
    let mut url = format!(
        "{API}/search?query={}&limit={}&offset={}&index=relevance",
        query_escape(query),
        limit.clamp(1, 100),
        offset
    );
    if !facets.is_empty() {
        url.push_str(&format!(
            "&facets={}",
            query_escape(&format!("[{}]", facets.join(",")))
        ));
    }
    let v = get(&url).await?;
    let hits: Vec<SearchHit> = v
        .get("hits")
        .and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter()
                .map(|h| SearchHit {
                    project_id: h
                        .get("project_id")
                        .and_then(|x| x.as_str())
                        .or_else(|| h.get("slug").and_then(|x| x.as_str()))
                        .unwrap_or("")
                        .to_string(),
                    slug: h.get("slug").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    title: h
                        .get("title")
                        .and_then(|x| x.as_str())
                        .unwrap_or("（未命名）")
                        .to_string(),
                    description: h
                        .get("description")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    author: h.get("author").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    icon_url: h
                        .get("icon_url")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    downloads: h.get("downloads").and_then(|x| x.as_u64()).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    let total = v
        .get("total_hits")
        .and_then(|x| x.as_u64())
        .unwrap_or(hits.len() as u64) as u32;
    Ok((hits, total))
}

pub async fn versions(project_id: &str, game: &str, loader: &str) -> Result<Vec<MrVersion>, String> {
    let mut url = format!("{API}/project/{project_id}/version");
    let mut q: Vec<String> = Vec::new();
    if !loader.trim().is_empty() {
        q.push(format!("loaders={}", query_escape(&format!("[\"{loader}\"]"))));
    }
    if !game.trim().is_empty() {
        q.push(format!(
            "game_versions={}",
            query_escape(&format!("[\"{game}\"]"))
        ));
    }
    if !q.is_empty() {
        url.push('?');
        url.push_str(&q.join("&"));
    }
    let v = get(&url).await?;
    let list = v.as_array().cloned().unwrap_or_default();
    Ok(list
        .iter()
        .filter_map(|d| {
            let id = d.get("id").and_then(|x| x.as_str())?.to_string();
            let mut files: Vec<MrFile> = d
                .get("files")
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| {
                            Some(MrFile {
                                filename: f.get("filename")?.as_str()?.to_string(),
                                url: f.get("url")?.as_str()?.to_string(),
                                sha1: f
                                    .get("hashes")
                                    .and_then(|h| h.get("sha1"))
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .to_lowercase(),
                                size: f.get("size").and_then(|x| x.as_u64()).unwrap_or(0),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            // primary 文件排最前，界面直接取第一个
            files.sort_by_key(|f| {
                let is_primary = d
                    .get("files")
                    .and_then(|x| x.as_array())
                    .and_then(|arr| arr.iter().find(|x| x.get("filename").and_then(|n| n.as_str()) == Some(f.filename.as_str())))
                    .and_then(|x| x.get("primary"))
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false);
                !is_primary
            });
            let deps = d
                .get("dependencies")
                .and_then(|x| x.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|dep| MrDependency {
                            project_id: dep
                                .get("project_id")
                                .and_then(|x| x.as_str())
                                .map(|s| s.to_string()),
                            version_id: dep
                                .get("version_id")
                                .and_then(|x| x.as_str())
                                .map(|s| s.to_string()),
                            kind: dep
                                .get("dependency_type")
                                .and_then(|x| x.as_str())
                                .unwrap_or("optional")
                                .to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(MrVersion {
                version_number: d
                    .get("version_number")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                name: d
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or_else(|| d.get("version_number").and_then(|x| x.as_str()).unwrap_or(""))
                    .to_string(),
                game_versions: str_list(d.get("game_versions")),
                loaders: str_list(d.get("loaders")),
                files,
                dependencies: deps,
                date_published: d
                    .get("date_published")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                id,
            })
        })
        .collect())
}

fn str_list(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 装一个模组版本到某个实例的 mods 目录，返回落盘路径。
pub async fn install_mod(
    version_id_of_game: &str,
    project_id: &str,
    game: &str,
    loader: &str,
    mr_version: Option<&str>,
) -> Result<String, String> {
    let list = versions(project_id, game, loader).await?;
    let chosen = match mr_version {
        Some(id) => list
            .into_iter()
            .find(|v| v.id == id)
            .ok_or_else(|| "找不到指定的版本".to_string())?,
        None => list
            .into_iter()
            .next()
            .ok_or_else(|| "该模组没有适用于当前游戏版本/加载器的版本".to_string())?,
    };
    let file = chosen
        .files
        .first()
        .ok_or_else(|| "该版本没有可下载的文件".to_string())?
        .clone();

    let dir = mods_dir(version_id_of_game);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 mods 目录失败：{e}"))?;
    let dest = dir.join(&file.filename);
    download::download(&file.url, &dest, Some(&file.sha1))
        .await
        .map_err(|e| format!("下载 {} 失败：{e}", file.filename))?;
    Ok(dest.to_string_lossy().to_string())
}

/// 实例的 mods 目录：版本隔离时用 `versions/<id>/mods`，否则用全局 `mods`。
pub fn mods_dir(version_id: &str) -> PathBuf {
    if !version_id.trim().is_empty() {
        let isolated = paths::paths()
            .versions_dir
            .join(version_id)
            .join("mods");
        if isolated.is_dir() {
            return isolated;
        }
    }
    paths::paths().game_dir.join("mods")
}

/// 扫描 mods 目录（含已禁用的 `.jar.disabled`）。
pub fn scan_mods(version_id: &str) -> Vec<InstalledMod> {
    let dir = manifest::game_directory(version_id).join("mods");
    let dir = if dir.is_dir() { dir } else { mods_dir(version_id) };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<InstalledMod> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let file_name = e.file_name().to_string_lossy().to_string();
            let lower = file_name.to_lowercase();
            if !(lower.ends_with(".jar") || lower.ends_with(".jar.disabled")) {
                return None;
            }
            let path = e.path();
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            let enabled = lower.ends_with(".jar");
            let meta = read_meta(&path);
            let (fallback_name, fallback_ver) = cleaned_name(&file_name);
            Some(InstalledMod {
                file_name,
                path: path.to_string_lossy().to_string(),
                mod_id: meta.id.unwrap_or_else(|| sanitize(&fallback_name)),
                name: meta.name.unwrap_or(fallback_name),
                version: meta.version.unwrap_or(fallback_ver),
                size,
                enabled,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// 启用/禁用：改名为 `.jar` ↔ `.jar.disabled`。
pub fn set_mod_enabled(path: &str, enabled: bool) -> Result<String, String> {
    let p = PathBuf::from(path);
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or("路径不合法")?;
    let lower = name.to_lowercase();
    let target_name = if enabled {
        if !lower.ends_with(".jar.disabled") {
            return Ok(p.to_string_lossy().to_string());
        }
        name.trim_end_matches(".disabled").to_string()
    } else {
        if !lower.ends_with(".jar") || lower.ends_with(".jar.disabled") {
            return Ok(p.to_string_lossy().to_string());
        }
        format!("{name}.disabled")
    };
    let target = p.with_file_name(&target_name);
    if target.exists() {
        return Err(format!("{} 已存在", target_name));
    }
    std::fs::rename(&p, &target).map_err(|e| format!("改名失败：{e}"))?;
    Ok(target.to_string_lossy().to_string())
}

/// 删除模组：移动到 mods/.trash/，方便回滚。
pub fn delete_mod(path: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    let dir = p.parent().ok_or("路径不合法")?;
    let trash = dir.join(".trash");
    std::fs::create_dir_all(&trash).map_err(|e| format!("创建回收目录失败：{e}"))?;
    let name = p.file_name().ok_or("路径不合法")?.to_string_lossy().to_string();
    let mut dest = trash.join(&name);
    if dest.exists() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        dest = trash.join(format!("{ts}-{name}"));
    }
    std::fs::rename(&p, &dest).map_err(|e| format!("删除失败：{e}"))
}

// ---------------------------------------------------------------- jar 元数据

struct JarMeta {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
}

fn read_meta(jar: &Path) -> JarMeta {
    let mut meta = JarMeta {
        id: None,
        name: None,
        version: None,
    };
    let Ok(file) = std::fs::File::open(jar) else {
        return meta;
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return meta;
    };

    // NeoForge / Forge
    for entry in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Ok(mut f) = zip.by_name(entry) {
            let mut text = String::new();
            if f.read_to_string(&mut text).is_ok() {
                meta.id = toml_value(&text, "modId");
                meta.version = toml_value(&text, "version");
                meta.name = toml_value(&text, "displayName");
                if meta.id.is_some() {
                    break;
                }
            }
        }
    }
    // Fabric / Quilt
    if meta.id.is_none() {
        for entry in ["fabric.mod.json", "quilt.mod.json"] {
            if let Ok(mut f) = zip.by_name(entry) {
                let mut text = String::new();
                if f.read_to_string(&mut text).is_ok() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        meta.id = v.get("id").and_then(|x| x.as_str()).map(|s| s.to_string());
                        meta.name = v
                            .get("name")
                            .and_then(|x| x.as_str())
                            .map(|s| s.to_string());
                        let ver = v.get("version").and_then(|x| x.as_str());
                        meta.version = ver.map(|s| s.to_string());
                    }
                }
                if meta.id.is_some() {
                    break;
                }
            }
        }
    }
    // Forge 的 version 常常是占位符
    if let Some(v) = meta.version.clone() {
        if v.contains("${") {
            meta.version = None;
        }
    }
    meta
}

/// 从 toml 文本里取 `key = "value"` 的第一个值。
fn toml_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((k, rest)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let v = rest.trim();
        let v = v.strip_prefix('"').and_then(|s| s.split('"').next()).map(|s| s.to_string());
        if let Some(v) = v {
            if !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

fn cleaned_name(file_name: &str) -> (String, String) {
    let base = file_name
        .trim_end_matches(".disabled")
        .trim_end_matches(".jar")
        .to_string();
    // 「xxx-1.20.1-2.3.4」→ 名字 + 版本
    let mut parts = base.rsplitn(2, '-');
    let last = parts.next().unwrap_or("");
    let head = parts.next().unwrap_or("");
    if !head.is_empty() && last.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        (head.to_string(), last.to_string())
    } else {
        (base, String::new())
    }
}

fn sanitize(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

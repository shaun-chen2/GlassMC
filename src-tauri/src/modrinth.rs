//! Modrinth 客户端与本地模组管理。
//!
//! 下载后一定校验 SHA1 —— 镜像/代理返回的半截文件如果直接进 mods，
//! 游戏只会抛一个看不懂的 `ZipException`。

use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::download;
use crate::install::Progress;
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
    pub project_id: String,
    pub version_number: String,
    pub name: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<MrFile>,
    pub dependencies: Vec<MrDependency>,
    pub date_published: String,
}

/// 装一个模组时的结果汇总：装了什么、跳过了什么、哪些依赖失败。
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<DepFailure>,
    pub conflicts: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DepFailure {
    pub project_id: String,
    pub title: String,
    pub reason: String,
}

/// `.glassmc-modrinth.json`：记录「哪个 Modrinth 项目装成了哪个 jar」，
/// 用来跳过重复安装。老用户手动丢进来的 jar 不在索引里，会重新下一次同名文件。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct IndexEntry {
    project_id: String,
    version_id: String,
    file_name: String,
    sha1: String,
    game: String,
    loader: String,
    at: u64,
    /// 装这个项目时它的必需前置（project id）。
    /// 跳过一个已装项目时靠它继续往下走，不用再发请求。
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct ModIndex {
    version: u32,
    entries: Vec<IndexEntry>,
}

static INDEX_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn index_path(mods: &Path) -> PathBuf {
    mods.parent().unwrap_or(mods).join(".glassmc-modrinth.json")
}

fn load_index(mods: &Path) -> ModIndex {
    std::fs::read_to_string(index_path(mods))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_index(mods: &Path, mut idx: ModIndex) -> Result<(), String> {
    idx.version = 1;
    // 文件被手动删掉后，索引里对应的条目也该消失
    idx.entries.retain(|e| mods.join(&e.file_name).is_file());
    let p = index_path(mods);
    let tmp = p.with_extension("json.part");
    let text = serde_json::to_string_pretty(&idx).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("写入模组索引失败：{e}"))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("保存模组索引失败：{e}"))
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
                    slug: h
                        .get("slug")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
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
                    author: h
                        .get("author")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
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

pub async fn versions(
    project_id: &str,
    game: &str,
    loader: &str,
) -> Result<Vec<MrVersion>, String> {
    let mut url = format!("{API}/project/{project_id}/version");
    let mut q: Vec<String> = Vec::new();
    if !loader.trim().is_empty() {
        q.push(format!(
            "loaders={}",
            query_escape(&format!("[\"{loader}\"]"))
        ));
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
    Ok(list.iter().filter_map(parse_version).collect())
}

/// 单个版本：依赖里只给了 version_id、没给 project_id 时用。
pub async fn version_by_id(id: &str) -> Result<MrVersion, String> {
    let v = get(&format!("{API}/version/{id}")).await?;
    parse_version(&v).ok_or_else(|| "版本内容不完整".to_string())
}

fn parse_version(d: &serde_json::Value) -> Option<MrVersion> {
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
            .and_then(|arr| {
                arr.iter().find(|x| {
                    x.get("filename").and_then(|n| n.as_str()) == Some(f.filename.as_str())
                })
            })
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
        project_id: d
            .get("project_id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        version_number: d
            .get("version_number")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        name: d
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or_else(|| {
                d.get("version_number")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
            })
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

/// 一个待安装项（自己或某个必需前置）。
struct PlanItem {
    project_id: String,
    title: String,
    version_id: String,
    file: MrFile,
    game: String,
    loader: String,
    deps: Vec<String>,
}

/// 找到「适用于当前游戏版本 + 加载器」的版本：优先依赖指定的版本，其次该项目的第一个。
async fn pick_version(
    project: &str,
    version_id: Option<&str>,
    game: &str,
    loader: &str,
) -> Result<MrVersion, String> {
    if let Some(vid) = version_id.filter(|s| !s.trim().is_empty()) {
        // 依赖里写死的版本可能已被作者删除，取不到就退回「按项目 + 加载器找最新」
        if let Ok(v) = version_by_id(vid).await {
            return Ok(v);
        }
    }
    if let Some(v) = versions(project, game, loader).await?.into_iter().next() {
        return Ok(v);
    }
    let any = versions(project, game, "").await?;
    if any.is_empty() {
        return Err(format!("没有适用于 Minecraft {game} 的版本"));
    }
    let mut loaders: Vec<String> = any.iter().flat_map(|v| v.loaders.clone()).collect();
    loaders.sort();
    loaders.dedup();
    Err(format!(
        "该模组不支持 {}（可用：{}）",
        if loader.trim().is_empty() {
            "当前加载器"
        } else {
            loader
        },
        loaders.join(" / ")
    ))
}

/// 装一个模组（含必需前置）到某个实例的 mods 目录。
///
/// 依赖关系取自 Modrinth 的 `dependencies`；只补 `required`，
/// `optional` 要看调用方传的 `include_optional`，`incompatible` 只记录冲突不安装。
pub async fn install_mod(
    app: Option<tauri::AppHandle>,
    version_id_of_game: &str,
    project_id: &str,
    game: &str,
    loader: &str,
    mr_version: Option<&str>,
    include_optional: bool,
) -> Result<InstallReport, String> {
    // 依赖图上限：深度 4 层、总计 32 个节点。正常模组树远小于这个数，
    // 设上限只是防止作者把依赖写成一棵树时无限展开。
    const MAX_DEPTH: usize = 3;
    const MAX_NODES: usize = 32;

    let dir = mods_dir(version_id_of_game);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 mods 目录失败：{e}"))?;

    let mut report = InstallReport::default();
    let installed: std::collections::HashMap<String, Vec<String>> = load_index(&dir)
        .entries
        .into_iter()
        .map(|e| (e.project_id, e.dependencies))
        .collect();

    let mut plan: Vec<PlanItem> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, Option<String>, usize)> = VecDeque::new();
    queue.push_back((project_id.to_string(), mr_version.map(|s| s.to_string()), 0));

    while let Some((pid, vid, depth)) = queue.pop_front() {
        // 入队即标记，避免 A→B→A 这种环
        if !visited.insert(pid.clone()) {
            continue;
        }
        if plan.len() >= MAX_NODES {
            break;
        }
        if let Some(deps) = installed.get(&pid) {
            report.skipped.push(pid);
            // 已装的项目也要把它的前置走一遍：用户可能手动删了某个前置，
            // 或者上次装到一半失败了。依赖清单存在索引里，不用再发请求。
            if depth < MAX_DEPTH {
                for d in deps {
                    if !visited.contains(d) {
                        queue.push_back((d.clone(), None, depth + 1));
                    }
                }
            }
            continue;
        }
        let v = match pick_version(&pid, vid.as_deref(), game, loader).await {
            Ok(v) => v,
            Err(e) => {
                // 根项目都找不到兼容版本时直接报错，别让用户看到「装了 0 个」
                if pid == project_id {
                    return Err(e);
                }
                report.failed.push(DepFailure {
                    project_id: pid.clone(),
                    title: pid,
                    reason: e,
                });
                continue;
            }
        };
        let Some(file) = v.files.first().cloned() else {
            let reason = "该版本没有可下载的文件".to_string();
            if pid == project_id {
                return Err(reason);
            }
            report.failed.push(DepFailure {
                project_id: pid,
                title: v.name.clone(),
                reason,
            });
            continue;
        };

        for d in &v.dependencies {
            let wanted = match d.kind.as_str() {
                "required" => true,
                "optional" if include_optional => true,
                "incompatible" => {
                    report
                        .conflicts
                        .push(d.project_id.clone().unwrap_or_else(|| "未知项目".into()));
                    false
                }
                _ => false,
            };
            if !wanted {
                continue;
            }
            if depth >= MAX_DEPTH {
                report.failed.push(DepFailure {
                    project_id: d.project_id.clone().unwrap_or_default(),
                    title: d.project_id.clone().unwrap_or_else(|| "依赖".into()),
                    reason: "依赖层级过深，已停止自动补装".into(),
                });
                continue;
            }
            let (dpid, dvid) = match (&d.project_id, &d.version_id) {
                (Some(p), v) => (p.clone(), v.clone()),
                // 少数条目只有 version_id，查一次版本才知道它属于哪个项目
                (None, Some(vid)) => match version_by_id(vid).await {
                    Ok(dv) if !dv.project_id.is_empty() => (dv.project_id, Some(dv.id)),
                    _ => {
                        report.failed.push(DepFailure {
                            project_id: String::new(),
                            title: "依赖".into(),
                            reason: "Modrinth 未提供该依赖的项目信息".into(),
                        });
                        continue;
                    }
                },
                _ => continue,
            };
            if dpid == pid {
                continue;
            }
            queue.push_back((dpid, dvid, depth + 1));
        }

        let required_deps: Vec<String> = v
            .dependencies
            .iter()
            .filter(|d| d.kind == "required")
            .filter_map(|d| d.project_id.clone())
            .collect();
        plan.push(PlanItem {
            project_id: pid,
            title: v.name.clone(),
            version_id: v.id.clone(),
            file,
            game: game.to_string(),
            loader: loader.to_string(),
            deps: required_deps,
        });
    }

    if !plan.is_empty() {
        let prog = Progress::new(app, "mod:install");
        prog.stage("下载模组", plan.len(), 0.2, 0.78);
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
        let mut handles = Vec::new();
        let mut seen_sha: HashSet<String> = HashSet::new();

        for item in plan {
            // 同一个 jar 被两个依赖同时引用时只下一次
            if !item.file.sha1.is_empty() && !seen_sha.insert(item.file.sha1.clone()) {
                report.skipped.push(item.file.filename);
                continue;
            }
            let sem = sem.clone();
            let dir = dir.clone();
            let prog = prog.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let dest = dir.join(&item.file.filename);
                let res = if dest.is_file() {
                    Ok(())
                } else {
                    download::download(&item.file.url, &dest, Some(&item.file.sha1)).await
                };
                prog.tick(false);
                (item, res)
            }));
        }

        let mut added: Vec<IndexEntry> = Vec::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        for h in handles {
            let Ok((item, res)) = h.await else { continue };
            match res {
                Ok(()) => {
                    report.installed.push(item.file.filename.clone());
                    added.push(IndexEntry {
                        project_id: item.project_id,
                        version_id: item.version_id,
                        file_name: item.file.filename,
                        sha1: item.file.sha1,
                        game: item.game,
                        loader: item.loader,
                        at: now,
                        dependencies: item.deps,
                    });
                }
                Err(e) => report.failed.push(DepFailure {
                    project_id: item.project_id,
                    title: item.title,
                    reason: format!("下载 {} 失败：{e}", item.file.filename),
                }),
            }
        }
        prog.tick(true);
        prog.note("模组下载完成");

        if !added.is_empty() {
            // 读-改-写全程持锁：两次安装并发时不要互相覆盖
            let _guard = INDEX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let mut latest = load_index(&dir);
            for e in added {
                latest.entries.retain(|x| x.project_id != e.project_id);
                latest.entries.push(e);
            }
            let _ = save_index(&dir, latest);
        }
    } else {
        // 顺手清理已被手动删掉的条目
        let _guard = INDEX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = save_index(&dir, load_index(&dir));
    }

    Ok(report)
}

/// 实例的 mods 目录：版本隔离时用 `versions/<id>/mods`，否则用全局 `mods`。
pub fn mods_dir(version_id: &str) -> PathBuf {
    if !version_id.trim().is_empty() {
        let isolated = paths::paths().versions_dir.join(version_id).join("mods");
        if isolated.is_dir() {
            return isolated;
        }
    }
    paths::paths().game_dir.join("mods")
}

/// 扫描 mods 目录（含已禁用的 `.jar.disabled`）。
pub fn scan_mods(version_id: &str) -> Vec<InstalledMod> {
    let dir = manifest::game_directory(version_id).join("mods");
    let dir = if dir.is_dir() {
        dir
    } else {
        mods_dir(version_id)
    };
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
    out.sort_by_key(|a| a.name.to_lowercase());
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
    let name = p
        .file_name()
        .ok_or("路径不合法")?
        .to_string_lossy()
        .to_string();
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
        let v = v
            .strip_prefix('"')
            .and_then(|s| s.split('"').next())
            .map(|s| s.to_string());
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
    if !head.is_empty()
        && last
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    {
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

#[cfg(test)]
mod tests {
    use super::install_mod;

    /// 真实装一次 Iris（必需前置 Sodium），再装一次检查是否全部跳过。
    /// 需要联网；`cargo test -- --ignored modrinth`
    #[tokio::test]
    #[ignore]
    async fn installs_required_dependencies() {
        let root = std::env::temp_dir().join(format!("glassmc-mr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("GLASSMC_DATA", &root);

        let rep = install_mod(None, "1.20.1", "iris", "1.20.1", "fabric", None, false)
            .await
            .unwrap();
        let names = rep.installed.join(",");
        assert!(names.contains("iris"), "缺 iris：{names}");
        assert!(names.contains("sodium"), "缺必需前置 sodium：{names}");
        assert!(rep.failed.is_empty(), "有失败项：{:?}", rep.failed);

        // 第二次：索引命中，不再下载
        let again = install_mod(None, "1.20.1", "iris", "1.20.1", "fabric", None, false)
            .await
            .unwrap();
        assert!(
            again.installed.is_empty(),
            "重复下载了：{:?}",
            again.installed
        );
        assert!(again.skipped.len() >= 2, "没有跳过：{:?}", again.skipped);

        let idx = root.join("minecraft/.glassmc-modrinth.json");
        assert!(idx.is_file(), "没有写索引");
        let text = std::fs::read_to_string(idx).unwrap();
        assert!(text.contains("AANobbMI"), "索引里没有 sodium 的 project id");
        let _ = std::fs::remove_dir_all(&root);
    }
}

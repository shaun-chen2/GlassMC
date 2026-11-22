//! 模组进阶：组合预设、冲突检测、拖拽导入、批量更新。
//!
//! 所有命令都带 `version_id`，mods 目录解析与现有模组管理保持一致：
//! 优先 `versions/<id>/mods`（隔离实例），否则退回 `game_dir/mods`。
//! 扫描类命令只读；任何写操作都先备份，且单个文件失败不影响同批其它文件。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{manifest, modrinth, net, store};

const PRESET_KEY: &str = "mod-presets";
const BACKUP_KEY: &str = "mod-backups";

// ---------------------------------------------------------------- 公共小工具

#[derive(Clone, Debug)]
struct JarFile {
    file_name: String,
    path: PathBuf,
    /// 去掉 `.jar` / `.jar.disabled` 之后的部分
    stem: String,
    enabled: bool,
}

fn mods_dir(version_id: &str) -> PathBuf {
    let isolated = manifest::game_directory(version_id).join("mods");
    if isolated.is_dir() {
        isolated
    } else {
        modrinth::mods_dir(version_id)
    }
}

/// 列出 mods 目录里的 jar（含 `.jar.disabled`），按文件名排序，保证结果稳定。
fn list_jars(dir: &Path) -> Vec<JarFile> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<JarFile> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            let (stem, enabled) = if lower.ends_with(".jar.disabled") {
                (&name[..name.len() - ".jar.disabled".len()], false)
            } else if lower.ends_with(".jar") {
                (&name[..name.len() - ".jar".len()], true)
            } else {
                return None;
            };
            Some(JarFile {
                file_name: name.clone(),
                path: e.path(),
                stem: stem.to_string(),
                enabled,
            })
        })
        .collect();
    out.sort_by_key(|a| a.file_name.to_lowercase());
    out
}

/// 从文件名尾部推断版本号：`foo-1.21.1-2.3.4.jar` -> `2.3.4`。
fn version_from_name(stem: &str) -> String {
    let mut parts = stem.rsplitn(2, '-');
    let last = parts.next().unwrap_or("");
    let head = parts.next().unwrap_or("");
    if !head.is_empty()
        && last
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        && !last.contains(' ')
    {
        last.to_string()
    } else {
        String::new()
    }
}

fn slugify(s: &str) -> String {
    let mut out = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

// ---------------------------------------------------------------- 1. 组合预设

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub version_id: String,
    /// 启用的 jar 文件名（只有文件名，不带路径）
    pub enabled: Vec<String>,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct PresetStore {
    presets: Vec<Preset>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PresetApplyResult {
    pub enabled: usize,
    pub disabled: usize,
    pub failed: Vec<String>,
}

fn preset_store() -> PresetStore {
    store::load(PRESET_KEY)
}

fn save_presets(s: &PresetStore) -> Result<(), String> {
    store::save(PRESET_KEY, s)
}

/// `short_id()` 同一秒可能重复，补上亚秒部分。
fn unique_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{}{:x}", store::short_id(), nanos)
}

#[tauri::command]
pub fn preset_list() -> Vec<Preset> {
    preset_store().presets
}

#[tauri::command]
pub fn preset_save(name: String, version_id: String) -> Result<Vec<Preset>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("预设名称不能为空".to_string());
    }
    let enabled = list_jars(&mods_dir(&version_id))
        .into_iter()
        .filter(|j| j.enabled)
        .map(|j| format!("{}.jar", j.stem))
        .collect::<Vec<_>>();

    let mut s = preset_store();
    // 同一个版本下同名预设直接覆盖，避免越攒越多
    s.presets
        .retain(|p| !(p.name == name && p.version_id == version_id));
    s.presets.insert(
        0,
        Preset {
            id: unique_id(),
            name: name.to_string(),
            version_id,
            enabled,
            created_at: store::now_secs(),
        },
    );
    save_presets(&s)?;
    Ok(s.presets)
}

#[tauri::command]
pub fn preset_apply(preset_id: String, version_id: String) -> Result<PresetApplyResult, String> {
    let preset = preset_store()
        .presets
        .into_iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| "找不到该预设，请刷新后重试".to_string())?;

    let dir = mods_dir(&version_id);
    let wanted: HashSet<String> = preset.enabled.iter().map(|n| n.to_lowercase()).collect();
    let mut out = PresetApplyResult::default();

    for jar in list_jars(&dir) {
        let wanted_name = format!("{}.jar", jar.stem);
        let want_enabled = wanted.contains(&wanted_name.to_lowercase());
        if want_enabled {
            out.enabled += 1;
        } else {
            out.disabled += 1;
        }
        let target_name = if want_enabled {
            wanted_name
        } else {
            format!("{}.jar.disabled", jar.stem)
        };
        if jar.file_name == target_name {
            continue;
        }
        let to = dir.join(&target_name);
        if to.exists() {
            out.failed
                .push(format!("{}：同名目标已存在，已跳过", jar.file_name));
            continue;
        }
        if let Err(e) = std::fs::rename(&jar.path, &to) {
            out.failed.push(format!("{}：{e}", jar.file_name));
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn preset_delete(preset_id: String) -> Result<Vec<Preset>, String> {
    let mut s = preset_store();
    s.presets.retain(|p| p.id != preset_id);
    save_presets(&s)?;
    Ok(s.presets)
}

// ---------------------------------------------------------------- 2. 冲突检测

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    /// duplicate-id / multi-version / duplicate-file
    pub kind: String,
    /// error / warning / info
    pub severity: String,
    pub description: String,
    pub files: Vec<String>,
    pub suggestion: String,
}

#[derive(Clone, Debug, Default)]
struct ModMeta {
    ids: Vec<String>,
    name: String,
    version: String,
}

struct JarInfo {
    jar: JarFile,
    meta: ModMeta,
}

fn str_of(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
            if !s.trim().is_empty() {
                return Some(s.trim().to_string());
            }
        }
    }
    None
}

fn push_id(meta: &mut ModMeta, id: &str) {
    let id = id.trim().to_ascii_lowercase();
    if !id.is_empty() && !meta.ids.contains(&id) {
        meta.ids.push(id);
    }
}

fn parse_json_meta(text: &str, meta: &mut ModMeta) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    let entries: Vec<&serde_json::Value> = if let Some(arr) = v.as_array() {
        arr.iter().collect()
    } else if let Some(arr) = v.get("modlist").and_then(|x| x.as_array()) {
        arr.iter().collect()
    } else {
        vec![&v]
    };
    for e in entries {
        if let Some(id) = str_of(e, &["id", "modid", "modId"]) {
            push_id(meta, &id);
        }
        if meta.name.is_empty() {
            if let Some(n) = str_of(e, &["name", "displayName"]) {
                meta.name = n;
            }
        }
        if meta.version.is_empty() {
            if let Some(ver) = str_of(e, &["version"]) {
                meta.version = ver;
            }
        }
    }
}

fn parse_toml_meta(text: &str, meta: &mut ModMeta) {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, rest)) = line.split_once('=') else {
            continue;
        };
        let Some(v) = rest
            .trim()
            .strip_prefix('"')
            .and_then(|s| s.split('"').next())
        else {
            continue;
        };
        let v = v.trim();
        if v.is_empty() {
            continue;
        }
        match k.trim() {
            "modId" => push_id(meta, v),
            "version" if meta.version.is_empty() => meta.version = v.to_string(),
            "displayName" if meta.name.is_empty() => meta.name = v.to_string(),
            _ => {}
        }
    }
}

/// 只读 jar 里的元数据，不改动文件。元数据读不到时留空，由文件名兜底推断。
fn read_meta(path: &Path) -> ModMeta {
    let mut meta = ModMeta::default();
    let Ok(file) = std::fs::File::open(path) else {
        return meta;
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return meta;
    };

    for entry in ["fabric.mod.json", "mcmod.info"] {
        if let Ok(f) = zip.by_name(entry) {
            let mut text = String::new();
            let _ = f.take(1_000_000).read_to_string(&mut text);
            parse_json_meta(&text, &mut meta);
        }
    }
    for entry in ["META-INF/mods.toml", "META-INF/neoforge.mods.toml"] {
        if let Ok(f) = zip.by_name(entry) {
            let mut text = String::new();
            let _ = f.take(1_000_000).read_to_string(&mut text);
            parse_toml_meta(&text, &mut meta);
        }
    }
    // Forge 的 version 常写成 ${version}，对比较没有意义
    if meta.version.contains("${") {
        meta.version.clear();
    }
    meta
}

/// 没有 modid 时用文件名去掉版本段做身份，避免把同一模组的不同版本当成两个模组。
fn identity(info: &JarInfo) -> String {
    if let Some(id) = info.meta.ids.first() {
        return id.clone();
    }
    info.jar
        .stem
        .to_lowercase()
        .split('-')
        .filter(|p| {
            !p.chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        })
        .collect::<Vec<_>>()
        .join("-")
}

#[tauri::command]
pub fn mods_conflicts(version_id: String) -> Vec<Conflict> {
    let infos: Vec<JarInfo> = list_jars(&mods_dir(&version_id))
        .into_iter()
        .map(|jar| {
            let meta = read_meta(&jar.path);
            JarInfo { jar, meta }
        })
        .collect();
    let mut out: Vec<Conflict> = Vec::new();

    // 1. 重复 modid
    let mut by_id: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, info) in infos.iter().enumerate() {
        for id in &info.meta.ids {
            by_id.entry(id.clone()).or_default().push(i);
        }
    }
    let mut id_groups: Vec<(String, Vec<usize>)> = by_id.into_iter().collect();
    id_groups.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, idx) in id_groups {
        if idx.len() < 2 {
            continue;
        }
        out.push(Conflict {
            kind: "duplicate-id".to_string(),
            severity: "error".to_string(),
            description: format!(
                "有 {} 个模组声明了同一个 ID「{}」，通常只会加载其中一个",
                idx.len(),
                id
            ),
            files: idx
                .iter()
                .map(|&i| infos[i].jar.file_name.clone())
                .collect(),
            suggestion: "保留需要的那一个，删除或禁用其余同 ID 的 jar".to_string(),
        });
    }

    // 2. 同一模组的多个版本
    let mut by_mod: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, info) in infos.iter().enumerate() {
        by_mod.entry(identity(info)).or_default().push(i);
    }
    let mut mod_groups: Vec<(String, Vec<usize>)> = by_mod.into_iter().collect();
    mod_groups.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, idx) in mod_groups {
        if idx.len() < 2 {
            continue;
        }
        let mut versions: Vec<String> = idx
            .iter()
            .map(|&i| {
                let m = &infos[i].meta;
                if !m.version.is_empty() {
                    m.version.to_lowercase()
                } else {
                    version_from_name(&infos[i].jar.stem).to_lowercase()
                }
            })
            .filter(|v| !v.is_empty())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        versions.sort();
        if versions.len() < 2 {
            continue;
        }
        out.push(Conflict {
            kind: "multi-version".to_string(),
            severity: "warning".to_string(),
            description: format!(
                "模组「{}」同时装了 {} 个不同版本：{}",
                name,
                versions.len(),
                versions.join("、")
            ),
            files: idx
                .iter()
                .map(|&i| infos[i].jar.file_name.clone())
                .collect(),
            suggestion: "只保留与当前游戏版本匹配的一个，其余移出 mods 目录".to_string(),
        });
    }

    // 3. 同名不同后缀（例如 foo.jar 与 foo.jar.disabled 同时存在）
    let mut by_stem: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, info) in infos.iter().enumerate() {
        by_stem
            .entry(info.jar.stem.to_lowercase())
            .or_default()
            .push(i);
    }
    let mut stem_groups: Vec<(String, Vec<usize>)> = by_stem.into_iter().collect();
    stem_groups.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, idx) in stem_groups {
        if idx.len() < 2 {
            continue;
        }
        out.push(Conflict {
            kind: "duplicate-file".to_string(),
            severity: "info".to_string(),
            description: format!(
                "存在 {} 个同名文件，后缀不同（启用 / 禁用状态同时存在）",
                idx.len()
            ),
            files: idx
                .iter()
                .map(|&i| infos[i].jar.file_name.clone())
                .collect(),
            suggestion: "确认后删除不需要的副本；正常切换状态只会保留其中一个".to_string(),
        });
    }

    out.sort_by_key(|c| match c.severity.as_str() {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    });
    out
}

// ---------------------------------------------------------------- 3. 拖拽导入

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    pub file_name: String,
    pub reason: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub count: usize,
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<ImportFailure>,
}

fn find_jars(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(find_jars(&p));
        } else if p.is_file()
            && p.extension()
                .and_then(|v| v.to_str())
                .map(|v| v.eq_ignore_ascii_case("jar"))
                .unwrap_or(false)
        {
            out.push(p);
        }
    }
    out
}

fn copy_new_jar(src: &Path, dir: &Path, name: &str, out: &mut ImportReport) {
    // 只取文件名，避免传入的奇怪路径写到 mods 目录之外
    let safe = Path::new(name)
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_default();
    if safe.is_empty() {
        out.failed.push(ImportFailure {
            file_name: name.to_string(),
            reason: "文件名不合法".to_string(),
        });
        return;
    }
    let dest = dir.join(&safe);
    if dest.exists() {
        out.skipped.push(safe);
        return;
    }
    match std::fs::copy(src, &dest) {
        Ok(_) => out.imported.push(safe),
        Err(e) => out.failed.push(ImportFailure {
            file_name: safe,
            reason: format!("复制失败：{e}"),
        }),
    }
}

#[tauri::command]
pub fn mods_drop_import(version_id: String, paths: Vec<String>) -> ImportReport {
    let mut out = ImportReport::default();
    let dir = mods_dir(&version_id);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        for raw in &paths {
            out.failed.push(ImportFailure {
                file_name: store::file_name(raw),
                reason: format!("无法创建 mods 目录：{e}"),
            });
        }
        return out;
    }

    let tmp_root = store::files_dir("mods-drop-import");
    for (i, raw) in paths.iter().enumerate() {
        let src = Path::new(raw);
        let name = store::file_name(raw);
        if !src.is_file() {
            out.failed.push(ImportFailure {
                file_name: name,
                reason: "文件不存在或不是普通文件".to_string(),
            });
            continue;
        }
        let ext = src
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "jar" {
            copy_new_jar(src, &dir, &name, &mut out);
        } else if ext == "zip" {
            // 解压到隔离的临时目录；unzip_to 已经防 Zip Slip
            let tmp = tmp_root.join(format!("{}-{}", store::now_secs(), i));
            match store::unzip_to(src, &tmp) {
                Ok(_) => {
                    let mut jars = find_jars(&tmp);
                    jars.sort();
                    if jars.is_empty() {
                        out.failed.push(ImportFailure {
                            file_name: name,
                            reason: "压缩包里没有找到 .jar 文件".to_string(),
                        });
                    } else {
                        for jar in &jars {
                            let jar_name = jar
                                .file_name()
                                .map(|v| v.to_string_lossy().to_string())
                                .unwrap_or_default();
                            copy_new_jar(jar, &dir, &jar_name, &mut out);
                        }
                    }
                }
                Err(e) => out.failed.push(ImportFailure {
                    file_name: name,
                    reason: format!("解压失败：{e}"),
                }),
            }
            let _ = std::fs::remove_dir_all(&tmp);
        } else {
            out.failed.push(ImportFailure {
                file_name: name,
                reason: "仅支持 .jar 或 .zip".to_string(),
            });
        }
    }
    out.count = out.imported.len();
    out
}

// ---------------------------------------------------------------- 4. 批量更新

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCandidate {
    /// 本地 jar 文件名，应用更新时用它定位旧文件
    pub file_name: String,
    pub project: String,
    pub current: String,
    pub latest: String,
    pub download_url: String,
    /// 有更新时说明原因；查询失败时写失败原因
    pub reason: String,
    /// false 表示这一项是失败提示，不可勾选更新
    pub ok: bool,
}

fn fail_row(file_name: &str, project: &str, current: &str, reason: String) -> UpdateCandidate {
    UpdateCandidate {
        file_name: file_name.to_string(),
        project: project.to_string(),
        current: current.to_string(),
        latest: String::new(),
        download_url: String::new(),
        reason,
        ok: false,
    }
}

/// 从实例 id 里猜游戏版本，如 `1.21.4-fabric` -> `1.21.4`。
fn guess_game(s: &str) -> String {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            let cand = &s[start..i];
            let parts: Vec<&str> = cand.split('.').collect();
            if (2..=3).contains(&parts.len())
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
            {
                return cand.to_string();
            }
        } else {
            i += 1;
        }
    }
    String::new()
}

fn guess_loader(s: &str) -> String {
    let l = s.to_lowercase();
    if l.contains("neoforge") {
        "neoforge".to_string()
    } else if l.contains("fabric") {
        "fabric".to_string()
    } else if l.contains("forge") {
        "forge".to_string()
    } else if l.contains("quilt") {
        "quilt".to_string()
    } else {
        String::new()
    }
}

#[tauri::command]
pub async fn mods_update_check(version_id: String) -> Vec<UpdateCandidate> {
    let jars: Vec<JarFile> = list_jars(&mods_dir(&version_id))
        .into_iter()
        .filter(|j| j.enabled)
        .collect();
    if jars.is_empty() {
        return Vec::new();
    }
    let game = guess_game(&version_id);
    let loader = guess_loader(&version_id);
    let mut out: Vec<UpdateCandidate> = Vec::new();

    for jar in jars {
        let meta = read_meta(&jar.path);
        let query = if meta.name.is_empty() {
            jar.stem.clone()
        } else {
            meta.name.clone()
        };
        let current = if meta.version.is_empty() {
            version_from_name(&jar.stem)
        } else {
            meta.version.clone()
        };
        let mut project = meta.ids.first().cloned().unwrap_or_else(|| slugify(&query));

        // 先按 modid / 名字当 slug 查；失败再搜索一次拿项目 id。
        let versions = match modrinth::versions(&project, &game, &loader).await {
            Ok(v) => v,
            Err(_) => match modrinth::search(&query, &game, &loader, 0, 1).await {
                Ok((hits, _)) if !hits.is_empty() => {
                    project = if !hits[0].title.is_empty() {
                        hits[0].title.clone()
                    } else {
                        hits[0].slug.clone()
                    };
                    let pid = if hits[0].project_id.is_empty() {
                        hits[0].slug.clone()
                    } else {
                        hits[0].project_id.clone()
                    };
                    match modrinth::versions(&pid, &game, &loader).await {
                        Ok(v) => v,
                        Err(e) => {
                            out.push(fail_row(
                                &jar.file_name,
                                &project,
                                &current,
                                format!("查询更新失败：{}；已跳过", e),
                            ));
                            continue;
                        }
                    }
                }
                _ => {
                    out.push(fail_row(
                        &jar.file_name,
                        &project,
                        &current,
                        "查询更新失败：网络不可用或未找到项目".to_string(),
                    ));
                    continue;
                }
            },
        };

        if versions.is_empty() {
            out.push(fail_row(
                &jar.file_name,
                &project,
                &current,
                "没有找到匹配当前游戏版本 / 加载器的更新".to_string(),
            ));
            continue;
        }

        let latest = &versions[0];
        if !current.is_empty() && current.eq_ignore_ascii_case(&latest.version_number) {
            continue;
        }
        let file = latest
            .files
            .iter()
            .find(|f| f.filename.to_lowercase().ends_with(".jar"))
            .or_else(|| latest.files.first());
        let Some(file) = file else {
            out.push(fail_row(
                &jar.file_name,
                &project,
                &current,
                "最新版本没有可下载的 jar 文件".to_string(),
            ));
            continue;
        };
        out.push(UpdateCandidate {
            file_name: jar.file_name.clone(),
            project: if project.is_empty() {
                latest.name.clone()
            } else {
                project
            },
            current,
            latest: latest.version_number.clone(),
            download_url: file.url.clone(),
            reason: "发现新版本".to_string(),
            ok: true,
        });
    }
    out
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItem {
    pub file_name: String,
    pub download_url: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFailure {
    pub file_name: String,
    pub reason: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateApplyReport {
    pub succeeded: Vec<String>,
    pub failed: Vec<UpdateFailure>,
    pub backup_dir: String,
}

#[tauri::command]
pub async fn mods_update_apply(version_id: String, items: Vec<UpdateItem>) -> UpdateApplyReport {
    let mut out = UpdateApplyReport::default();
    let dir = mods_dir(&version_id);
    let backup = store::files_dir(BACKUP_KEY);
    out.backup_dir = backup.to_string_lossy().to_string();

    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::create_dir_all(&backup)) {
        for it in &items {
            out.failed.push(UpdateFailure {
                file_name: it.file_name.clone(),
                reason: format!("无法准备目录：{e}"),
            });
        }
        return out;
    }

    let client = net::client();
    for item in items {
        let name = Path::new(&item.file_name)
            .file_name()
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.is_empty() || name != item.file_name || name.contains('/') || name.contains('\\') {
            out.failed.push(UpdateFailure {
                file_name: item.file_name.clone(),
                reason: "文件名不合法".to_string(),
            });
            continue;
        }
        if !item.download_url.starts_with("https://") {
            out.failed.push(UpdateFailure {
                file_name: name,
                reason: "下载地址不合法（仅允许 https）".to_string(),
            });
            continue;
        }

        // 逐项下载：某一项断网 / 404 只标记这一项，不中断整批。
        let resp = client.get(&item.download_url).send().await;
        let bytes = match resp {
            Ok(r) if r.status().is_success() => match r.bytes().await {
                Ok(b) => b.to_vec(),
                Err(e) => {
                    out.failed.push(UpdateFailure {
                        file_name: name,
                        reason: format!("下载失败：{e}"),
                    });
                    continue;
                }
            },
            Ok(r) => {
                out.failed.push(UpdateFailure {
                    file_name: name,
                    reason: format!("下载失败：HTTP {}", r.status().as_u16()),
                });
                continue;
            }
            Err(e) => {
                out.failed.push(UpdateFailure {
                    file_name: name,
                    reason: format!("网络错误：{e}"),
                });
                continue;
            }
        };
        if bytes.is_empty() {
            out.failed.push(UpdateFailure {
                file_name: name,
                reason: "下载内容为空，已跳过".to_string(),
            });
            continue;
        }
        // jar 本质是 zip，PK 魔数能挡住「下载到 HTML 错误页」这类假文件
        if !bytes.starts_with(b"PK") {
            out.failed.push(UpdateFailure {
                file_name: name,
                reason: "下载内容不是有效的 jar 文件，已跳过".to_string(),
            });
            continue;
        }

        let target = dir.join(&name);
        if target.is_file() {
            let bak = backup.join(format!("{}.{}.bak", name, store::now_secs()));
            if let Err(e) = std::fs::copy(&target, &bak) {
                out.failed.push(UpdateFailure {
                    file_name: name,
                    reason: format!("备份旧文件失败，未替换：{e}"),
                });
                continue;
            }
        }
        // 与目标同目录写临时文件，保证 rename 是原子操作
        let tmp = dir.join(format!(".{}.glassmc-new", name));
        if let Err(e) = std::fs::write(&tmp, &bytes) {
            out.failed.push(UpdateFailure {
                file_name: name,
                reason: format!("写入临时文件失败：{e}"),
            });
            continue;
        }
        if target.is_file() {
            if let Err(e) = std::fs::remove_file(&target) {
                let _ = std::fs::remove_file(&tmp);
                out.failed.push(UpdateFailure {
                    file_name: name,
                    reason: format!("移除旧文件失败，未替换：{e}"),
                });
                continue;
            }
        }
        if let Err(e) = std::fs::rename(&tmp, &target) {
            // 尽力回滚：恢复备份并把临时文件清掉
            let bak = backup.join(format!("{}.{}.bak", name, store::now_secs()));
            if bak.is_file() {
                let _ = std::fs::copy(&bak, &target);
            }
            let _ = std::fs::remove_file(&tmp);
            out.failed.push(UpdateFailure {
                file_name: name,
                reason: format!("替换失败：{e}"),
            });
            continue;
        }
        out.succeeded.push(name);
    }
    out
}

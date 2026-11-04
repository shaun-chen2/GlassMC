//! Modrinth 整合包（.mrpack）导入与导出。
//!
//! 导入只做「铺 overrides + 按 files[] 下载」，游戏本体与加载器交给安装流程；
//! 导出只打包 `mods/*.jar`、`config/**`、`resourcepacks/*.zip`，存档与日志不会进包。

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::download;
use crate::install::Progress;
use crate::paths;

const MANIFEST: &str = ".glassmc-modpack.json";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct PackFile {
    pub path: String,
    pub hashes: std::collections::HashMap<String, String>,
    pub env: std::collections::HashMap<String, String>,
    pub downloads: Vec<String>,
    pub file_size: u64,
}

impl PackFile {
    fn client_needed(&self) -> bool {
        self.env
            .get("client")
            .map(|v| v.to_lowercase() != "unsupported")
            .unwrap_or(true)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PackIndex {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    pub summary: String,
    pub files: Vec<PackFile>,
    pub dependencies: std::collections::HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PackRecord {
    pub instance_id: String,
    pub name: String,
    pub game_version: String,
    pub loader: String,
    pub loader_version: String,
    pub mod_count: usize,
}

/// 只读索引，供界面预览。
pub fn inspect(path: &str) -> Result<PackIndex, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("打开整合包失败：{e}"))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|_| "这不是有效的 .mrpack（无法解压）".to_string())?;
    let mut entry = zip
        .by_name("modrinth.index.json")
        .map_err(|_| "整合包里缺少 modrinth.index.json".to_string())?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|e| format!("读取索引失败：{e}"))?;
    let text = text.trim_start_matches('\u{feff}').trim();
    let mut index: PackIndex =
        serde_json::from_str(text).map_err(|e| format!("整合包索引解析失败：{e}"))?;
    // 兼容 camelCase / snake_case 两种写法
    if index.format_version == 0 {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
            index.format_version =
                v.get("formatVersion").and_then(|x| x.as_u64()).unwrap_or(1) as u32;
        }
    }
    Ok(index)
}

/// 导入整合包，返回记录（调用方接着装游戏本体与加载器）。
pub async fn import(
    app: Option<tauri::AppHandle>,
    path: &str,
    target_name: Option<String>,
) -> Result<PackRecord, String> {
    let prog = Progress::new(app, "modpack:import");
    prog.note("读取整合包索引");
    let index = inspect(path)?;
    let game_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "整合包索引里没有声明 Minecraft 版本".to_string())?;
    let (loader, loader_version) = detect_loader(&index.dependencies);

    let base_name = target_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| index.name.clone());
    let instance_id = unique_instance_id(&base_name);
    let instance_dir = paths::paths().versions_dir.join(&instance_id);
    std::fs::create_dir_all(&instance_dir).map_err(|e| format!("创建实例目录失败：{e}"))?;

    // 解压 overrides / client-overrides
    prog.stage("解压覆盖文件", 1, 0.02, 0.18);
    extract_prefix(path, "overrides/", &instance_dir)?;
    extract_prefix(path, "client-overrides/", &instance_dir)?;
    prog.tick(true);

    // 下载 files[]
    let entries: Vec<PackFile> = index
        .files
        .into_iter()
        .filter(|f| f.client_needed())
        .collect();
    let mod_count = entries
        .iter()
        .filter(|f| {
            f.path.to_lowercase().starts_with("mods/") && f.path.to_lowercase().ends_with(".jar")
        })
        .count();

    if !entries.is_empty() {
        prog.stage("下载整合包文件", entries.len(), 0.20, 0.78);
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
        let mut handles = Vec::new();
        for entry in entries {
            let sem = sem.clone();
            let dir = instance_dir.clone();
            let prog = prog.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let dest = dir.join(entry.path.trim_start_matches('/'));
                // 防止 ../ 逃出实例目录
                if !dest.starts_with(&dir) {
                    prog.tick(true);
                    return;
                }
                let sha1 = entry.hashes.get("sha1").cloned();
                if dest.is_file() {
                    prog.tick(false);
                    return;
                }
                let urls: Vec<String> = entry.downloads.clone();
                if !urls.is_empty() {
                    let _ = download::download_any(&urls, &dest, sha1.as_deref()).await;
                }
                prog.tick(false);
            }));
        }
        for h in handles {
            let _ = h.await;
        }
        prog.tick(true);
    }

    let record = PackRecord {
        instance_id: instance_id.clone(),
        name: index.name,
        game_version,
        loader,
        loader_version,
        mod_count,
    };
    if let Ok(s) = serde_json::to_string_pretty(&record) {
        let _ = std::fs::write(instance_dir.join(MANIFEST), s);
    }
    Ok(record)
}

fn extract_prefix(pack: &str, prefix: &str, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(pack).map_err(|e| format!("打开整合包失败：{e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "无法解压整合包".to_string())?;
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else {
            continue;
        };
        let name = entry.name().to_string();
        let Some(rel) = name.strip_prefix(prefix) else {
            continue;
        };
        if rel.is_empty() || rel.ends_with('/') {
            continue;
        }
        if rel.contains("..") {
            continue;
        }
        let target = dest.join(rel);
        if !target.starts_with(dest) {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| format!("写入失败：{e}"))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("写入失败：{e}"))?;
    }
    Ok(())
}

/// 导出为 .mrpack。
pub fn export(
    version_id: &str,
    destination: &str,
    name: &str,
    summary: &str,
) -> Result<String, String> {
    let instance = paths::paths().versions_dir.join(version_id);
    let source = if instance.is_dir() {
        instance
    } else {
        paths::paths().game_dir.clone()
    };

    let tmp = std::env::temp_dir().join(format!("glassmc-mrpack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("overrides")).map_err(|e| format!("创建临时目录失败：{e}"))?;

    let mut index = PackIndex {
        format_version: 1,
        game: "minecraft".into(),
        version_id: version_id.to_string(),
        name: name.to_string(),
        summary: summary.to_string(),
        ..Default::default()
    };
    index
        .dependencies
        .insert("minecraft".into(), guess_game_version(version_id));
    if let Some((key, ver)) = loader_from_json(version_id) {
        index.dependencies.insert(key, ver);
    }

    // mods/*.jar
    let mods_src = source.join("mods");
    let mods_dst = tmp.join("overrides/mods");
    if mods_src.is_dir() {
        std::fs::create_dir_all(&mods_dst).map_err(|e| format!("创建目录失败：{e}"))?;
        if let Ok(rd) = std::fs::read_dir(&mods_src) {
            for e in rd.filter_map(|x| x.ok()) {
                let p = e.path();
                let lower = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !p.is_file() || !lower.ends_with(".jar") {
                    continue;
                }
                let target = mods_dst.join(p.file_name().unwrap());
                std::fs::copy(&p, &target).map_err(|e| format!("复制失败：{e}"))?;
                let mut hashes = std::collections::HashMap::new();
                if let Ok(s) = download::sha1_file(&target) {
                    hashes.insert("sha1".to_string(), s);
                }
                index.files.push(PackFile {
                    path: format!("mods/{}", p.file_name().unwrap().to_string_lossy()),
                    hashes,
                    env: [("client".to_string(), "required".to_string())]
                        .into_iter()
                        .collect(),
                    downloads: Vec::new(),
                    file_size: std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
                });
            }
        }
    }

    // config/** 与 resourcepacks/*.zip
    copy_tree(&source.join("config"), &tmp.join("overrides/config"))?;
    if let Ok(rd) = std::fs::read_dir(source.join("resourcepacks")) {
        let dst = tmp.join("overrides/resourcepacks");
        let _ = std::fs::create_dir_all(&dst);
        for e in rd.filter_map(|x| x.ok()) {
            let p = e.path();
            if p.extension().map(|x| x == "zip").unwrap_or(false) {
                let _ = std::fs::copy(&p, dst.join(p.file_name().unwrap()));
            }
        }
    }

    let mod_count = index.files.len();
    let index_text =
        serde_json::to_string_pretty(&index).map_err(|e| format!("索引序列化失败：{e}"))?;
    std::fs::write(tmp.join("modrinth.index.json"), index_text)
        .map_err(|e| format!("写入索引失败：{e}"))?;

    // 打包
    let out_path = PathBuf::from(destination);
    let _ = std::fs::remove_file(&out_path);
    let file = std::fs::File::create(&out_path).map_err(|e| format!("创建整合包文件失败：{e}"))?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    add_file(
        &mut writer,
        &tmp.join("modrinth.index.json"),
        "modrinth.index.json",
        opts,
    )?;
    add_dir(&mut writer, &tmp.join("overrides"), "overrides", opts)?;
    writer.finish().map_err(|e| format!("打包失败：{e}"))?;
    let _ = std::fs::remove_dir_all(&tmp);

    Ok(format!("{destination}（{mod_count} 个模组）"))
}

fn add_file(
    writer: &mut zip::ZipWriter<std::fs::File>,
    path: &Path,
    name: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("读取失败：{e}"))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)
        .map_err(|e| format!("读取失败：{e}"))?;
    writer
        .start_file(name, opts)
        .map_err(|e| format!("写入压缩包失败：{e}"))?;
    writer
        .write_all(&buf)
        .map_err(|e| format!("写入压缩包失败：{e}"))?;
    Ok(())
}

fn add_dir(
    writer: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    prefix: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for e in rd.filter_map(|x| x.ok()) {
        let p = e.path();
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let rel = format!("{prefix}/{name}");
        if p.is_dir() {
            add_dir(writer, &p, &rel, opts)?;
        } else {
            add_file(writer, &p, &rel, opts)?;
        }
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst).map_err(|e| format!("创建目录失败：{e}"))?;
    let Ok(rd) = std::fs::read_dir(src) else {
        return Ok(());
    };
    for e in rd.filter_map(|x| x.ok()) {
        let p = e.path();
        let target = dst.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_tree(&p, &target)?;
        } else {
            let _ = std::fs::copy(&p, &target);
        }
    }
    Ok(())
}

/// 扫描已导入的整合包。
pub fn list_imported() -> Vec<PackRecord> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(paths::paths().versions_dir.clone()) else {
        return out;
    };
    for e in rd.filter_map(|x| x.ok()) {
        let p = e.path().join(MANIFEST);
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(r) = serde_json::from_str::<PackRecord>(&s) {
                out.push(r);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn detect_loader(deps: &std::collections::HashMap<String, String>) -> (String, String) {
    for key in ["neoforge", "forge", "fabric-loader", "quilt-loader"] {
        if let Some(v) = deps.get(key) {
            let name = match key {
                "fabric-loader" => "fabric",
                "quilt-loader" => "quilt",
                other => other,
            };
            return (name.to_string(), v.clone());
        }
    }
    (String::new(), String::new())
}

fn unique_instance_id(base: &str) -> String {
    let cleaned: String = base
        .trim()
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let cleaned = if cleaned.is_empty() {
        "整合包".to_string()
    } else {
        cleaned
    };
    if !paths::paths().versions_dir.join(&cleaned).exists() {
        return cleaned;
    }
    for i in 2..1000 {
        let candidate = format!("{cleaned}-{i}");
        if !paths::paths().versions_dir.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{cleaned}-{}", std::process::id())
}

/// 沿 inheritsFrom 找到根版本（通常就是 1.21.1 这种原版号）。
fn guess_game_version(version_id: &str) -> String {
    let mut cur = version_id.to_string();
    for _ in 0..8 {
        let p = paths::paths()
            .versions_dir
            .join(&cur)
            .join(format!("{cur}.json"));
        let Ok(text) = std::fs::read_to_string(&p) else {
            break;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            break;
        };
        match v.get("inheritsFrom").and_then(|x| x.as_str()) {
            Some(parent) if !parent.is_empty() && parent != cur => cur = parent.to_string(),
            _ => break,
        }
    }
    if cur
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        cur
    } else {
        version_id.to_string()
    }
}

/// 从合并后的 libraries 里认出加载器与它的版本号。
fn loader_from_json(version_id: &str) -> Option<(String, String)> {
    let merged = crate::manifest::local_merged(version_id)?;
    let libs = merged.get("libraries")?.as_array()?;
    for lib in libs {
        let Some(name) = lib.get("name").and_then(|x| x.as_str()) else {
            continue;
        };
        let parts: Vec<&str> = name.split(':').collect();
        if parts.len() < 3 {
            continue;
        }
        let found = match (parts[0], parts[1]) {
            ("net.neoforged", "neoforge") => Some("neoforge"),
            ("net.minecraftforge", "forge") => Some("forge"),
            ("net.fabricmc", "fabric-loader") => Some("fabric-loader"),
            ("org.quiltmc", "quilt-loader") => Some("quilt-loader"),
            _ => None,
        };
        if let Some(key) = found {
            return Some((key.to_string(), parts[2].to_string()));
        }
    }
    None
}

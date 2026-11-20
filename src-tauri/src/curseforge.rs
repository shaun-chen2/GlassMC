//! CurseForge 整合包（.zip）导入。
//!
//! CurseForge 的 `manifest.json` 只给 `projectID` + `fileID`，**不给文件名**；
//! 官方 API 又要 api key。这里走官网页面自己在用的内部接口取文件名
//! （`/api/v1/mods/{pid}/files/{fid}`，实测免 key、自定义 UA 也放行），
//! 再从 `mediafilez.forgecdn.net/files/{a}/{b}/{name}` 下载。
//!
//! 这条通路不是官方承诺的接口，所以每一层都留了兜底：
//! 元数据失败只记一条失败项，直链失败退到 `/download`（307 会带 api-key 过去）。
//! 文件没有哈希，只能比对 `fileLength`。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use crate::download;
use crate::install::Progress;
use crate::modpack;
use crate::net;
use crate::paths;

const META_API: &str = "https://www.curseforge.com/api/v1/mods";
const CDN: &str = "https://mediafilez.forgecdn.net/files";

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct CfManifest {
    minecraft: CfMc,
    manifest_type: String,
    name: String,
    version: String,
    author: String,
    files: Vec<CfFile>,
    overrides: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct CfMc {
    version: String,
    loaders: Vec<CfLoader>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct CfLoader {
    id: String,
    primary: bool,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct CfFile {
    // CurseForge 的键是大写 ID（projectID / fileID）
    #[serde(rename = "projectID", alias = "projectId")]
    project_id: u64,
    #[serde(rename = "fileID", alias = "fileId")]
    file_id: u64,
    required: bool,
}

/// 预览结果：复用 .mrpack 的 `PackIndex`，前端一套 UI 通吃。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CfProbe {
    pub index: modpack::PackIndex,
    pub loader: String,
    pub loader_version: String,
}

/// 导入结果：`failed` 非 0 时前端提示「有文件没下下来」，但实例仍然可用。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CfOutcome {
    pub record: modpack::PackRecord,
    pub failed: usize,
}

struct FileMeta {
    file_name: String,
    file_length: u64,
}

/// 只读 zip 里的 manifest.json，不发网络请求。
pub fn probe(path: &str) -> Result<CfProbe, String> {
    let manifest = read_manifest(path)?;
    if manifest.manifest_type != "minecraftModpack" {
        return Err("这不是 CurseForge 整合包（manifestType 不匹配）".into());
    }
    let game = manifest.minecraft.version.trim().to_string();
    if game.is_empty() {
        return Err("整合包没有声明 Minecraft 版本".into());
    }
    let (loader, loader_version) = parse_loader(&manifest.minecraft.loaders, &game);
    let mut deps: HashMap<String, String> = HashMap::new();
    deps.insert("minecraft".into(), game.clone());
    if !loader.is_empty() && !loader_version.is_empty() {
        deps.insert(dep_key(&loader).into(), loader_version.clone());
    }
    let index = modpack::PackIndex {
        format_version: 1,
        game: "minecraft".into(),
        version_id: String::new(),
        name: if manifest.name.trim().is_empty() {
            "CurseForge 整合包".into()
        } else {
            manifest.name.clone()
        },
        summary: format!(
            "CurseForge · {}{}",
            if manifest.author.trim().is_empty() {
                "未知作者"
            } else {
                manifest.author.trim()
            },
            if manifest.version.trim().is_empty() {
                String::new()
            } else {
                format!(" · v{}", manifest.version.trim())
            }
        ),
        files: manifest
            .files
            .iter()
            .map(|f| modpack::PackFile {
                // 真实文件名要联网才知道，这里只用于预览计数
                path: format!("mods/{}.jar", f.file_id),
                env: [("client".to_string(), "required".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            })
            .collect(),
        dependencies: deps,
    };
    Ok(CfProbe {
        index,
        loader,
        loader_version,
    })
}

pub async fn import(
    app: Option<tauri::AppHandle>,
    path: &str,
    target_name: Option<String>,
) -> Result<CfOutcome, String> {
    let prog = Progress::new(app, "modpack:cf-import");
    prog.note("读取整合包清单");
    let manifest = read_manifest(path)?;
    if manifest.manifest_type != "minecraftModpack" {
        return Err("这不是 CurseForge 整合包".into());
    }
    let game_version = manifest.minecraft.version.trim().to_string();
    if game_version.is_empty() {
        return Err("整合包没有声明 Minecraft 版本".into());
    }
    let (loader, loader_version) = parse_loader(&manifest.minecraft.loaders, &game_version);

    let base_name = target_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| manifest.name.clone());
    let instance_id = modpack::unique_instance_id(&base_name);
    let instance_dir = paths::paths().versions_dir.join(&instance_id);
    std::fs::create_dir_all(&instance_dir).map_err(|e| format!("创建实例目录失败：{e}"))?;

    // overrides 字段通常是 "overrides"；老包可能写成空
    let ov = manifest.overrides.trim().trim_end_matches('/');
    let ov = if ov.is_empty() { "overrides" } else { ov };
    prog.stage("解压覆盖文件", 2, 0.04, 0.08);
    let _ = modpack::extract_prefix(path, &format!("{ov}/"), &instance_dir);
    let _ = modpack::extract_prefix(path, "client-overrides/", &instance_dir);
    prog.tick(true);

    let files: Vec<CfFile> = manifest
        .files
        .into_iter()
        .filter(|f| f.project_id > 0 && f.file_id > 0)
        .collect();
    if files.is_empty() {
        prog.tick(true);
    }

    let mut cache = load_cache();
    let mut metas: Vec<(CfFile, Option<FileMeta>)> = Vec::new();
    prog.stage("查询文件信息", files.len(), 0.14, 0.24);
    {
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
        let mut handles = Vec::new();
        for f in files {
            let key = f.file_id.to_string();
            if let Some(name) = cache.get(&key).cloned() {
                metas.push((
                    f,
                    Some(FileMeta {
                        file_name: name,
                        file_length: 0,
                    }),
                ));
                continue;
            }
            let sem = sem.clone();
            let prog = prog.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let meta = file_meta(f.project_id, f.file_id).await.ok();
                prog.tick(false);
                (f, meta)
            }));
        }
        for h in handles {
            if let Ok((f, meta)) = h.await {
                if let Some(m) = &meta {
                    cache.insert(f.file_id.to_string(), m.file_name.clone());
                }
                metas.push((f, meta));
            }
        }
    }
    prog.tick(true);

    let mut failures = 0usize;
    let mut mod_count = 0usize;
    let jobs: Vec<(CfFile, FileMeta)> = metas
        .into_iter()
        .filter_map(|(f, m)| match m {
            Some(m) => Some((f, m)),
            None => {
                failures += 1;
                None
            }
        })
        .collect();

    if !jobs.is_empty() {
        prog.stage("下载整合包文件", jobs.len(), 0.40, 0.52);
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
        let mut handles = Vec::new();
        for (f, meta) in jobs {
            let sem = sem.clone();
            let dir = instance_dir.clone();
            let prog = prog.clone();
            let urls = download_urls(f.project_id, f.file_id, &meta.file_name);
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let dest = dir.join("mods").join(&meta.file_name);
                let mut ok = true;
                if !dest.is_file() {
                    if let Some(parent) = dest.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    ok = download::download_any(&urls, &dest, None).await.is_ok();
                }
                // 没有哈希，只校验大小；大小不符当作失败重新下过一次的人会自然覆盖
                if ok && meta.file_length > 0 {
                    if let Ok(m) = std::fs::metadata(&dest) {
                        if m.len() != meta.file_length {
                            let _ = std::fs::remove_file(&dest);
                            ok = false;
                        }
                    }
                }
                prog.tick(false);
                (meta.file_name, ok)
            }));
        }
        for h in handles {
            if let Ok((name, ok)) = h.await {
                if ok {
                    if name.to_lowercase().ends_with(".jar") {
                        mod_count += 1;
                    }
                } else {
                    failures += 1;
                }
            }
        }
        prog.tick(true);
    }

    save_cache(&cache);

    let record = modpack::PackRecord {
        instance_id: instance_id.clone(),
        name: if base_name.trim().is_empty() {
            "CurseForge 整合包".into()
        } else {
            base_name
        },
        game_version,
        loader,
        loader_version,
        mod_count,
    };
    if let Ok(s) = serde_json::to_string_pretty(&record) {
        let _ = std::fs::write(instance_dir.join(modpack::MANIFEST), s);
    }
    prog.note("整合包导入完成");
    Ok(CfOutcome {
        record,
        failed: failures,
    })
}

fn read_manifest(path: &str) -> Result<CfManifest, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("打开整合包失败：{e}"))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|_| "这不是有效的整合包（无法解压）".to_string())?;
    // 少数包把 manifest 放在一层目录里，按「路径最短的 manifest.json」取
    let mut best: Option<String> = None;
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index_raw(i) else {
            continue;
        };
        let name = entry.name().to_string();
        let lower = name.to_lowercase();
        if (lower == "manifest.json" || lower.ends_with("/manifest.json"))
            && best.as_ref().map(|b| name.len() < b.len()).unwrap_or(true)
        {
            best = Some(name);
        }
    }
    let name = best.ok_or("整合包里没有 manifest.json")?;
    let mut entry = zip
        .by_name(&name)
        .map_err(|e| format!("读取 manifest 失败：{e}"))?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|e| format!("读取 manifest 失败：{e}"))?;
    let text = text.trim_start_matches('\u{feff}').trim();
    serde_json::from_str(text).map_err(|e| format!("整合包清单解析失败：{e}"))
}

/// `forge-47.2.0` / `forge-1.20.1-47.2.0` / `neoforge-21.1.1` / `fabric-0.16.9`
///
/// 返回值与 `loader::forge_versions` 等接口给的版本串保持一致（不含游戏版本）。
fn parse_loader(loaders: &[CfLoader], game: &str) -> (String, String) {
    let Some(l) = loaders
        .iter()
        .find(|l| l.primary)
        .or_else(|| loaders.first())
    else {
        return (String::new(), String::new());
    };
    let id = l.id.trim();
    let (name, rest) = if let Some(r) = id.strip_prefix("neoforge-") {
        ("neoforge", r)
    } else if let Some(r) = id.strip_prefix("forge-") {
        ("forge", r)
    } else if let Some(r) = id.strip_prefix("fabric-") {
        ("fabric", r)
    } else if let Some(r) = id.strip_prefix("quilt-") {
        ("quilt", r)
    } else {
        return (String::new(), String::new());
    };
    let prefix = format!("{game}-");
    let ver = rest.strip_prefix(&prefix).unwrap_or(rest);
    (name.to_string(), ver.trim().to_string())
}

fn dep_key(loader: &str) -> &str {
    match loader {
        "fabric" => "fabric-loader",
        "quilt" => "quilt-loader",
        other => other,
    }
}

async fn file_meta(project: u64, file: u64) -> Result<FileMeta, String> {
    let url = format!("{META_API}/{project}/files/{file}");
    let mut last = String::new();
    for attempt in 0..3u32 {
        match net::client()
            .get(&url)
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(20))
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status == 429 || status >= 500 {
                    last = format!("HTTP {status}");
                    tokio::time::sleep(Duration::from_millis(800 * (1 << attempt))).await;
                    continue;
                }
                if !(200..300).contains(&status) {
                    return Err(format!("HTTP {status}"));
                }
                let v: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| format!("返回内容解析失败：{e}"))?;
                let d = v.get("data").ok_or("返回内容缺少 data")?;
                let name = d
                    .get("fileName")
                    .and_then(|x| x.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or("返回内容缺少 fileName")?
                    .to_string();
                let size = d.get("fileLength").and_then(|x| x.as_u64()).unwrap_or(0);
                return Ok(FileMeta {
                    file_name: name,
                    file_length: size,
                });
            }
            Err(e) => {
                last = e.to_string();
                tokio::time::sleep(Duration::from_millis(800 * (1 << attempt))).await;
            }
        }
    }
    Err(format!("查询文件信息失败：{last}"))
}

fn download_urls(project: u64, file: u64, name: &str) -> Vec<String> {
    let a = file / 1000;
    let b = file % 1000;
    let enc = percent_path(name);
    vec![
        format!("{CDN}/{a}/{b}/{enc}"),
        format!("https://edge.forgecdn.net/files/{a}/{b}/{enc}"),
        // 307 → edge.forgecdn.net，URL 上会带 api-key
        format!("{META_API}/{project}/files/{file}/download"),
    ]
}

fn percent_path(s: &str) -> String {
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

fn cache_file() -> PathBuf {
    paths::paths().data_root.join("cache").join("cf-files.json")
}

fn load_cache() -> HashMap<String, String> {
    std::fs::read_to_string(cache_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_cache(cache: &HashMap<String, String>) {
    let p = cache_file();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string(cache) {
        let tmp = p.with_extension("json.part");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{dep_key, import, parse_loader, percent_path, CfLoader};

    fn l(id: &str, primary: bool) -> CfLoader {
        CfLoader {
            id: id.to_string(),
            primary,
        }
    }

    #[test]
    fn parses_loader_ids() {
        assert_eq!(
            parse_loader(&[l("forge-47.2.0", true)], "1.20.1"),
            ("forge".to_string(), "47.2.0".to_string())
        );
        assert_eq!(
            parse_loader(&[l("forge-1.20.1-47.2.0", true)], "1.20.1"),
            ("forge".to_string(), "47.2.0".to_string())
        );
        assert_eq!(
            parse_loader(&[l("neoforge-21.1.1", true)], "1.21.1"),
            ("neoforge".to_string(), "21.1.1".to_string())
        );
        assert_eq!(
            parse_loader(&[l("fabric-0.16.9", true)], "1.20.1"),
            ("fabric".to_string(), "0.16.9".to_string())
        );
        // primary 优先
        assert_eq!(
            parse_loader(
                &[l("fabric-0.15.0", false), l("quilt-0.20.0", true)],
                "1.20.1"
            )
            .0,
            "quilt"
        );
        assert_eq!(parse_loader(&[], "1.20.1"), (String::new(), String::new()));
    }

    #[test]
    fn maps_dependency_keys() {
        assert_eq!(dep_key("fabric"), "fabric-loader");
        assert_eq!(dep_key("forge"), "forge");
    }

    #[test]
    fn encodes_file_names() {
        assert_eq!(percent_path("a b+c.jar"), "a%20b%2Bc.jar");
    }

    /// 真实走一遍「查文件名 → 下载 → 解压 overrides」，需要联网。
    /// `cargo test -- --ignored curseforge`
    #[tokio::test]
    #[ignore]
    async fn imports_real_curseforge_pack() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("glassmc-cf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("GLASSMC_DATA", &root);

        let zip_path = root.join("pack.zip");
        let f = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        // 真实存在的 CF 文件：JEI 238222 / 4593548
        let manifest = r#"{
          "minecraft": { "version": "1.18.2", "loaders": [{ "id": "forge-1.18.2-40.2.0", "primary": true }] },
          "manifestType": "minecraftModpack",
          "manifestVersion": 1,
          "name": "CF 测试包",
          "version": "1.0.0",
          "author": "GlassMC",
          "files": [{ "projectID": 238222, "fileID": 4593548, "required": true }],
          "overrides": "overrides"
        }"#;
        zw.start_file("manifest.json", opts).unwrap();
        zw.write_all(manifest.as_bytes()).unwrap();
        zw.start_file("overrides/options.txt", opts).unwrap();
        zw.write_all(b"fov:0\n").unwrap();
        // Zip Slip 样本：不应落到实例目录外
        zw.start_file("overrides/../evil.txt", opts).unwrap();
        zw.write_all(b"pwned").unwrap();
        zw.finish().unwrap();

        let out = import(None, zip_path.to_str().unwrap(), None)
            .await
            .unwrap();
        let p = super::probe(zip_path.to_str().unwrap()).unwrap();
        assert_eq!(p.loader, "forge");
        assert_eq!(p.loader_version, "40.2.0");
        assert_eq!(p.index.dependencies.get("minecraft").unwrap(), "1.18.2");
        assert_eq!(p.index.files.len(), 1);
        assert_eq!(out.record.game_version, "1.18.2");
        assert_eq!(out.record.loader, "forge");
        assert_eq!(out.record.loader_version, "40.2.0");
        assert_eq!(out.record.mod_count, 1);
        assert_eq!(out.failed, 0);

        let inst = root
            .join("minecraft/versions")
            .join(&out.record.instance_id);
        assert!(inst.join("mods/jei-1.18.2-forge-10.2.1.1005.jar").is_file());
        assert!(inst.join("options.txt").is_file());
        assert!(!inst.join("evil.txt").exists());
        assert!(!root.join("minecraft/evil.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}

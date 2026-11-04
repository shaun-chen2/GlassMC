//! 版本相关：远程清单、本地已装扫描、`inheritsFrom` 深合并。
//!
//! 深合并的规则是从 macOS 原生版踩坑总结出来的，不要随意简化：
//! - `libraries` 按 `group:artifact[:classifier]`（**不含版本**）取并集，子版本覆盖父版本。
//!   否则同一个库的两个版本会同时进 classpath 与 module path，NeoForge 直接抛
//!   "Module ... was already on the JVMs module path"。
//! - `arguments.jvm` / `arguments.game` **父 + 子依次拼接**，后续再去重。
//! - 其余字段子版本覆盖父版本。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

use crate::config::Config;
use crate::net;
use crate::paths;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VersionEntry {
    pub id: String,
    /// 上游 manifest 里叫 `type`（release/snapshot/old_beta…），
    /// 但对外（前端）统一叫 `kind`。注意必须分开写，`rename = "type"`
    /// 是两个方向同时生效的，会让前端拿到 `type` 而读不到 `kind`，
    /// 结果安装页每行只剩一个空胶囊。
    #[serde(rename(deserialize = "type", serialize = "kind"))]
    pub kind: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Deserialize, Debug)]
pub struct VersionManifest {
    /// 最新正式版/快照的 id，暂时没用但保留（未来做「一键更新」会读它）
    #[serde(default)]
    #[allow(dead_code)]
    pub latest: HashMap<String, String>,
    pub versions: Vec<VersionEntry>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVersion {
    pub id: String,
    pub kind: String,
    /// fabric / quilt / neoforge / forge / null
    pub loader: Option<String>,
    pub inherits: Option<String>,
    /// 该实例目录下的 mod 数（版本隔离时看实例内，否则看全局 mods）
    pub mods: usize,
    /// 是否版本隔离（游戏数据在 versions/<id>/ 里）
    pub isolated: bool,
    pub game_dir: String,
    /// 客户端 jar 是否就位
    pub jar_ok: bool,
    /// 缺失的库文件数（0 表示可直接启动）
    pub missing_libs: usize,
    pub has_mods_dir: bool,
}

pub async fn fetch_manifest(cfg: &Config) -> Result<VersionManifest, String> {
    let url = format!("{}/mc/game/version_manifest_v2.json", cfg.download_base());
    let v = net::get_json(&url).await?;
    serde_json::from_value::<VersionManifest>(v).map_err(|e| format!("版本清单格式异常：{e}"))
}

// ---------------------------------------------------------------- 版本 JSON

/// 读取版本 JSON：优先本地，否则从镜像拉取并缓存到 versions/<id>/<id>.json。
pub async fn load_version_json(id: &str, cfg: &Config) -> Result<Value, String> {
    let p = paths::paths()
        .versions_dir
        .join(id)
        .join(format!("{id}.json"));
    if let Ok(s) = tokio::fs::read_to_string(&p).await {
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            return Ok(v);
        }
    }
    let url = format!("{}/versions/{id}/{id}.json", cfg.download_base());
    let v = net::get_json(&url)
        .await
        .map_err(|e| format!("版本 {id} 的定义拉取失败（{e}）"))?;
    if let Some(parent) = p.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    if let Ok(s) = serde_json::to_string(&v) {
        let _ = tokio::fs::write(&p, s).await;
    }
    Ok(v)
}

/// 子版本 → 父版本的完整链条（child first）。
pub async fn version_chain(version_id: &str, cfg: &Config) -> Result<Vec<Value>, String> {
    let mut chain = Vec::new();
    let mut current = version_id.to_string();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..16 {
        if !seen.insert(current.clone()) {
            break;
        }
        let json = load_version_json(&current, cfg).await?;
        let parent = json
            .get("inheritsFrom")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        chain.push(json);
        if parent.is_empty() {
            break;
        }
        current = parent;
    }
    Ok(chain)
}

fn library_key(lib: &Value, fallback: usize) -> String {
    let name = lib.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return format!("__anon_{fallback}");
    }
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() >= 4 {
        format!("{}:{}:{}", parts[0], parts[1], parts[3])
    } else if parts.len() >= 3 {
        format!("{}:{}", parts[0], parts[1])
    } else {
        name.to_string()
    }
}

pub fn merge_chain(chain: &[Value]) -> Value {
    let mut merged: Map<String, Value> = Map::new();
    let mut libs: Vec<Value> = Vec::new();
    let mut lib_index: HashMap<String, usize> = HashMap::new();
    let mut jvm: Vec<Value> = Vec::new();
    let mut game: Vec<Value> = Vec::new();
    let mut has_arguments = false;

    // 父 → 子，子版本后写，天然覆盖
    for dict in chain.iter().rev() {
        let Some(obj) = dict.as_object() else {
            continue;
        };
        for (k, v) in obj {
            match k.as_str() {
                "libraries" => {
                    if let Some(arr) = v.as_array() {
                        for lib in arr {
                            let key = library_key(lib, libs.len());
                            match lib_index.get(&key) {
                                Some(&i) => libs[i] = lib.clone(),
                                None => {
                                    lib_index.insert(key, libs.len());
                                    libs.push(lib.clone());
                                }
                            }
                        }
                    }
                }
                "arguments" => {
                    has_arguments = true;
                    if let Some(a) = v.as_object() {
                        if let Some(x) = a.get("jvm").and_then(|x| x.as_array()) {
                            jvm.extend(x.iter().cloned());
                        }
                        if let Some(x) = a.get("game").and_then(|x| x.as_array()) {
                            game.extend(x.iter().cloned());
                        }
                    }
                }
                _ => {
                    merged.insert(k.clone(), v.clone());
                }
            }
        }
    }

    if !libs.is_empty() {
        merged.insert("libraries".to_string(), Value::Array(libs));
    }
    if has_arguments {
        let mut a = Map::new();
        a.insert("jvm".into(), Value::Array(jvm));
        a.insert("game".into(), Value::Array(game));
        merged.insert("arguments".into(), Value::Object(a));
    }
    Value::Object(merged)
}

pub fn libraries(merged: &Value) -> Vec<Value> {
    merged
        .get("libraries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

pub fn client_jar_id(chain: &[Value], merged: &Value, version_id: &str) -> String {
    if let Some(jar) = merged.get("jar").and_then(|v| v.as_str()) {
        if !jar.is_empty() {
            return jar.to_string();
        }
    }
    for dict in chain {
        let has_client = dict
            .get("downloads")
            .and_then(|d| d.get("client"))
            .is_some();
        if has_client {
            if let Some(id) = dict.get("id").and_then(|v| v.as_str()) {
                if !id.is_empty() {
                    return id.to_string();
                }
            }
        }
    }
    version_id.to_string()
}

/// 纯本地沿 `inheritsFrom` 查找主 jar 名（不发网络请求）。
///
/// 整合包实例（如「新蒸程1.7」）自己的 json 里通常没有 `jar` 字段，
/// 真正的客户端 jar 放在父版本目录里（`1.21.1-NeoForge/1.21.1-NeoForge.jar`）。
/// 只看本版本会误判成「缺 jar」。
pub fn local_client_jar_id(version_id: &str) -> String {
    let mut current = version_id.to_string();
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current.clone()) {
            break;
        }
        let p = paths::paths()
            .versions_dir
            .join(&current)
            .join(format!("{current}.json"));
        let Some(v) = std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        else {
            break;
        };
        if let Some(jar) = v.get("jar").and_then(|x| x.as_str()) {
            if !jar.is_empty() {
                return jar.to_string();
            }
        }
        if v.get("downloads").and_then(|d| d.get("client")).is_some() {
            return current;
        }
        match v.get("inheritsFrom").and_then(|x| x.as_str()) {
            Some(parent) if !parent.is_empty() => current = parent.to_string(),
            _ => break,
        }
    }
    version_id.to_string()
}

pub fn client_jar_path(jar_id: &str) -> std::path::PathBuf {
    paths::paths()
        .versions_dir
        .join(jar_id)
        .join(format!("{jar_id}.jar"))
}

pub fn existing_client_jar(jar_id: &str, version_id: &str) -> Option<std::path::PathBuf> {
    for id in [jar_id, version_id] {
        let p = client_jar_path(id);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------- 本地扫描

/// 版本隔离：实例目录里有这些标记就认为游戏数据放在实例内。
const ISOLATION_MARKERS: [&str; 7] = [
    ".hmcl",
    "mods",
    "config",
    "saves",
    "options.txt",
    "resourcepacks",
    "shaderpacks",
];

pub fn is_isolated(version_id: &str) -> bool {
    let inst = paths::paths().versions_dir.join(version_id);
    ISOLATION_MARKERS.iter().any(|m| inst.join(m).exists())
}

/// 该版本实际使用的游戏目录。
pub fn game_directory(version_id: &str) -> std::path::PathBuf {
    if is_isolated(version_id) {
        paths::paths().versions_dir.join(version_id)
    } else {
        paths::paths().game_dir.clone()
    }
}

/// 纯本地合并：沿 `inheritsFrom` 读本地 json 做深合并，不发任何网络请求。
/// 用于扫描阶段统计「还缺多少个库」。
pub fn local_merged(version_id: &str) -> Option<Value> {
    let mut chain: Vec<Value> = Vec::new();
    let mut current = version_id.to_string();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..16 {
        if !seen.insert(current.clone()) {
            break;
        }
        let p = paths::paths()
            .versions_dir
            .join(&current)
            .join(format!("{current}.json"));
        let v = std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())?;
        let parent = v
            .get("inheritsFrom")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        chain.push(v);
        if parent.is_empty() {
            break;
        }
        current = parent;
    }
    if chain.is_empty() {
        None
    } else {
        Some(merge_chain(&chain))
    }
}

fn count_files(dir: &Path) -> usize {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .count(),
        Err(_) => 0,
    }
}

fn detect_loader(version_id: &str, inherits: Option<&str>) -> Option<String> {
    let mut s = version_id.to_lowercase();
    if let Some(p) = inherits {
        s.push(' ');
        s.push_str(&p.to_lowercase());
    }
    if s.contains("neoforge") {
        Some("neoforge".into())
    } else if s.contains("forge") {
        Some("forge".into())
    } else if s.contains("fabric") {
        Some("fabric".into())
    } else if s.contains("quilt") {
        Some("quilt".into())
    } else {
        None
    }
}

/// 同步扫描已安装版本。只读文件系统，不做任何网络请求，所以很快。
pub fn scan_installed() -> Vec<InstalledVersion> {
    let vdir = paths::paths().versions_dir.clone();
    let Ok(rd) = std::fs::read_dir(&vdir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let json_path = path.join(format!("{id}.json"));
        if !json_path.is_file() {
            continue;
        }
        let (kind, inherits) = match std::fs::read_to_string(&json_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        {
            Some(v) => (
                v.get("type")
                    .and_then(|x| x.as_str())
                    .unwrap_or("release")
                    .to_string(),
                v.get("inheritsFrom")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
            ),
            None => ("release".to_string(), None),
        };
        let isolated = is_isolated(&id);
        let game_dir = if isolated {
            path.clone()
        } else {
            paths::paths().game_dir.clone()
        };
        let mods_dir = game_dir.join("mods");
        let has_mods_dir = mods_dir.is_dir();
        let mods = count_files(&mods_dir);

        // jar 是否就位：沿 inheritsFrom 找到真正的主 jar
        let jar_id = local_client_jar_id(&id);
        let jar_ok = existing_client_jar(&jar_id, &id).is_some();
        // 缺多少个库：本地合并后逐个 stat，不发网络请求
        let missing_libs = local_merged(&id)
            .map(|m| crate::launch::missing_classpath_files(&m, &jar_id, &id))
            .unwrap_or(0);

        out.push(InstalledVersion {
            loader: detect_loader(&id, inherits.as_deref()),
            id,
            kind,
            inherits,
            mods,
            isolated,
            game_dir: game_dir.to_string_lossy().to_string(),
            jar_ok,
            missing_libs,
            has_mods_dir,
        });
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out
}

//! 整合包进阶：短分享码 + 增量更新。
//!
//! 只读复用 `modpack.rs` 的 `PackRecord` / `MANIFEST`，不改动原有导入导出流程。
//! 分享码把关键字段压成紧凑 JSON → zlib → base64url，做成 `GM1-` 开头的短串；
//! 增量更新用自维护的基线快照与当前实例目录比较，应用时只动变化的文件。
//! 删除一律先搬到 `feature-files/pack-backups/`，绝不直接删。

use base64::Engine;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{modpack, net, paths, store};

const CODE_PREFIX: &str = "GM1-";
const MAX_CODE_LEN: usize = 1500;

// ---------------------------------------------------------------- 内部数据结构

/// 紧凑载荷：字段名故意用单字母，压出来的短码才够短。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct SharePayload {
    /// 版本号
    v: u32,
    /// 名称
    n: String,
    /// 游戏版本
    g: String,
    /// 加载器
    l: String,
    /// [项目 id, 版本 id] 列表
    m: Vec<(String, String)>,
    /// 截断说明（解码时展示）
    #[serde(default, skip_serializing_if = "String::is_empty")]
    note: String,
}

/// 实例里 `.glassmc-modrinth.json` 的最小可读结构。
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ModIndex {
    entries: Vec<ModIndexEntry>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ModIndexEntry {
    project_id: String,
    version_id: String,
    #[allow(dead_code)]
    file_name: String,
}

/// 分享码落盘记录（前端不直接读，留作溯源）。
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ShareRecord {
    id: String,
    at: i64,
    payload: SharePayload,
}

/// 基线快照中的单个文件。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct SnapFile {
    path: String,
    size: u64,
    /// 可能为空（老快照）；为空时按大小比较，并在扫描时补算 sha256。
    sha256: String,
    #[serde(default)]
    downloads: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Snapshot {
    pack_id: String,
    at: i64,
    files: Vec<SnapFile>,
}

// ---------------------------------------------------------------- 对外可见结构

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareMod {
    pub project_id: String,
    pub version_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackPreview {
    pub name: String,
    pub game: String,
    pub loader: String,
    pub mod_count: usize,
    pub mods: Vec<ShareMod>,
    pub truncated: bool,
    pub note: String,
    pub code_length: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestItem {
    pub project_id: String,
    pub version_id: String,
    pub game: String,
    pub loader: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallManifest {
    pub id: String,
    pub name: String,
    pub game: String,
    pub loader: String,
    pub items: Vec<ManifestItem>,
    pub truncated: bool,
    pub note: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    /// 相对实例目录的路径（用 / 分隔）
    pub path: String,
    /// 文件名
    pub name: String,
    pub size: u64,
    pub sha256: String,
    /// 少数记录会带下载地址
    #[serde(default)]
    pub downloads: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReport {
    pub pack_id: String,
    pub added: Vec<DiffFile>,
    pub changed: Vec<DiffFile>,
    pub removed: Vec<DiffFile>,
    pub unchanged: Vec<DiffFile>,
    pub baseline_created: bool,
    pub note: String,
}

/// 前端勾选项：`action` 为 added / changed / removed。
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApplyItem {
    pub action: String,
    pub path: String,
    // 前端回传的校验信息：目前仅展示与后续校验用，先保留字段
    #[allow(dead_code)]
    #[serde(default)]
    pub size: u64,
    #[allow(dead_code)]
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub downloads: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub path: String,
    pub action: String,
    pub ok: bool,
    pub message: String,
}

// ---------------------------------------------------------------- 小工具

fn safe(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        "unnamed".to_string()
    } else {
        out
    }
}

/// 实例 id 来自界面，绝不允许穿出 versions_dir。
fn valid_id(id: &str) -> bool {
    !id.trim().is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains("..")
        && !id.starts_with('.')
}

fn instance_dir(pack_id: &str) -> Result<PathBuf, String> {
    let dir = paths::paths().versions_dir.join(pack_id);
    if !dir.is_dir() {
        return Err(format!("找不到整合包实例：{pack_id}"));
    }
    Ok(dir)
}

fn snapshot_path(pack_id: &str) -> PathBuf {
    store::files_dir("pack-diff").join(format!("{}.json", safe(pack_id)))
}

fn save_snapshot(path: &Path, snap: &Snapshot) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建快照目录失败：{e}"))?;
    }
    let text = serde_json::to_string_pretty(snap).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
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

/// 扫描实例目录；隐藏文件（含 .glassmc-* 元数据）、日志与崩溃报告不纳入清单。
fn scan_dir(root: &Path) -> Vec<DiffFile> {
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<DiffFile>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if name == "logs" || name == "crash-reports" {
                continue;
            }
            walk(root, &p, out);
        } else if p.is_file() {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            let sha256 = sha256_file(&p).unwrap_or_default();
            out.push(DiffFile {
                path: rel.clone(),
                name: store::file_name(&rel),
                size,
                sha256,
                downloads: Vec::new(),
            });
        }
    }
}

fn encode(payload: &SharePayload) -> Result<String, String> {
    let json = serde_json::to_vec(payload).map_err(|e| format!("短码序列化失败：{e}"))?;
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::best());
    enc.write_all(&json).map_err(|e| format!("压缩失败：{e}"))?;
    let compressed = enc.finish().map_err(|e| format!("压缩失败：{e}"))?;
    Ok(format!(
        "{CODE_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed)
    ))
}

fn decode(code: &str) -> Result<SharePayload, String> {
    let raw = code.trim();
    let body = raw
        .strip_prefix(CODE_PREFIX)
        .or_else(|| raw.strip_prefix("gm1-"))
        .ok_or_else(|| "短码格式不对：需要以 GM1- 开头".to_string())?;
    let compressed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| "短码内容损坏：base64 解码失败".to_string())?;
    let mut dec = ZlibDecoder::new(&compressed[..]);
    let mut json = Vec::new();
    dec.read_to_end(&mut json)
        .map_err(|_| "短码内容损坏：解压失败".to_string())?;
    let payload: SharePayload =
        serde_json::from_slice(&json).map_err(|e| format!("短码内容损坏：{e}"))?;
    if payload.v == 0 {
        return Err("短码版本不支持".to_string());
    }
    Ok(payload)
}

/// 单次下载，强制超时；离线时给出明确提示。
async fn fetch_to_file(url: &str, dest: &Path) -> Result<(), String> {
    let resp = net::client()
        .get(url)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("网络不可用或超时：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败：{e}"))?;
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("part");
    tokio::fs::write(&tmp, &bytes)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(|e| e.to_string())
}

/// 删除 = 搬到备份目录，失败时退回复制 + 删除源文件。
fn move_to_backup(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.is_file() {
        return Err("源文件不存在，可能已被移走".to_string());
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst).map_err(|e| e.to_string())?;
    std::fs::remove_file(src).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- 命令：短分享码

/// 把整合包的关键字段压成 `GM1-` 短码。模组列表来自实例的 .glassmc-modrinth.json。
#[tauri::command]
pub fn pack_share_code(pack_id: String) -> Result<String, String> {
    if !valid_id(&pack_id) {
        return Err("整合包 ID 不合法".to_string());
    }
    let dir = instance_dir(&pack_id)?;

    let record: modpack::PackRecord = std::fs::read_to_string(dir.join(modpack::MANIFEST))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(modpack::PackRecord {
            instance_id: pack_id.clone(),
            name: pack_id.clone(),
            game_version: String::new(),
            loader: String::new(),
            loader_version: String::new(),
            mod_count: 0,
        });

    let mut payload = SharePayload {
        v: 1,
        n: record.name,
        g: record.game_version,
        l: record.loader,
        ..Default::default()
    };

    if let Ok(text) = std::fs::read_to_string(dir.join(".glassmc-modrinth.json")) {
        if let Ok(idx) = serde_json::from_str::<ModIndex>(&text) {
            let mut seen = HashSet::new();
            for entry in idx.entries {
                if entry.project_id.is_empty() || entry.version_id.is_empty() {
                    continue;
                }
                let pair = (entry.project_id, entry.version_id);
                if seen.insert(pair.clone()) {
                    payload.m.push(pair);
                }
            }
        }
    }

    // 超长时分批砍模组列表；砍光仍超长就继续截名称，保证字符串可用。
    let total = payload.m.len();
    let mut kept = total;
    loop {
        payload.m.truncate(kept);
        payload.note = if kept < total {
            format!("模组列表已截断：仅含前 {kept}/{total} 项")
        } else {
            String::new()
        };
        let code = encode(&payload)?;
        if code.len() <= MAX_CODE_LEN || kept == 0 {
            if code.len() > MAX_CODE_LEN {
                while payload.n.chars().count() > 1
                    && encode(&payload).map(|c| c.len()).unwrap_or(0) > MAX_CODE_LEN
                {
                    payload.n.pop();
                }
                return encode(&payload);
            }
            return Ok(code);
        }
        kept = if kept <= 10 { kept - 1 } else { kept / 2 };
    }
}

/// 只解码、不安装，返回可读摘要。
#[tauri::command]
pub fn pack_preview_code(code: String) -> Result<PackPreview, String> {
    let p = decode(&code)?;
    Ok(PackPreview {
        name: p.n,
        game: p.g,
        loader: p.l,
        mod_count: p.m.len(),
        mods: p
            .m
            .iter()
            .map(|(project_id, version_id)| ShareMod {
                project_id: project_id.clone(),
                version_id: version_id.clone(),
            })
            .collect(),
        truncated: !p.note.is_empty(),
        note: p.note,
        code_length: code.trim().len(),
    })
}

/// 解码并落盘，返回可交给现有 modrinth_install 的清单（前端逐个安装）。
#[tauri::command]
pub fn pack_install_code(code: String) -> Result<InstallManifest, String> {
    let p = decode(&code)?;
    let id = store::short_id();
    let dir = store::files_dir("pack-shares");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建分享目录失败：{e}"))?;
    let record = ShareRecord {
        id: id.clone(),
        at: store::now_secs(),
        payload: p.clone(),
    };
    let text = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{id}.json")), text)
        .map_err(|e| format!("保存分享清单失败：{e}"))?;

    let items =
        p.m.iter()
            .map(|(project_id, version_id)| ManifestItem {
                project_id: project_id.clone(),
                version_id: version_id.clone(),
                game: p.g.clone(),
                loader: p.l.clone(),
            })
            .collect();
    Ok(InstallManifest {
        id,
        name: p.n,
        game: p.g,
        loader: p.l,
        items,
        truncated: !p.note.is_empty(),
        note: p.note,
    })
}

// ---------------------------------------------------------------- 命令：增量更新

/// 基线（首次调用自动建立）与当前实例目录比较，输出四类文件。
#[tauri::command]
pub fn pack_diff(pack_id: String) -> Result<DiffReport, String> {
    if !valid_id(&pack_id) {
        return Err("整合包 ID 不合法".to_string());
    }
    let dir = instance_dir(&pack_id)?;
    let current = scan_dir(&dir);

    let sp = snapshot_path(&pack_id);
    let old: Option<Snapshot> = std::fs::read_to_string(&sp)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let baseline_created = old.is_none();
    let baseline = old.unwrap_or_else(|| Snapshot {
        pack_id: pack_id.clone(),
        at: store::now_secs(),
        files: current
            .iter()
            .map(|f| SnapFile {
                path: f.path.clone(),
                size: f.size,
                sha256: f.sha256.clone(),
                downloads: f.downloads.clone(),
            })
            .collect(),
    });
    if baseline_created {
        save_snapshot(&sp, &baseline)?;
    }

    let recorded: BTreeMap<&str, &SnapFile> = baseline
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();
    let present: BTreeMap<&str, &DiffFile> = current.iter().map(|f| (f.path.as_str(), f)).collect();

    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut removed = Vec::new();
    let mut unchanged = Vec::new();

    for (path, now) in &present {
        match recorded.get(*path) {
            None => added.push((*now).clone()),
            Some(old_file) => {
                // 记录里没有哈希时，用扫描得到的 sha256 与大小判断是否变化。
                let differ = if old_file.sha256.is_empty() || now.sha256.is_empty() {
                    old_file.size != now.size
                } else {
                    old_file.sha256 != now.sha256 || old_file.size != now.size
                };
                if differ {
                    changed.push((*now).clone());
                } else {
                    unchanged.push((*now).clone());
                }
            }
        }
    }
    for (path, old_file) in &recorded {
        if !present.contains_key(*path) {
            removed.push(DiffFile {
                path: old_file.path.clone(),
                name: store::file_name(&old_file.path),
                size: old_file.size,
                sha256: old_file.sha256.clone(),
                downloads: old_file.downloads.clone(),
            });
        }
    }

    let note = if baseline_created {
        format!(
            "已建立基线（{} 个文件），再次扫描即可看到变化",
            current.len()
        )
    } else {
        String::new()
    };
    Ok(DiffReport {
        pack_id,
        added,
        changed,
        removed,
        unchanged,
        baseline_created,
        note,
    })
}

/// 只处理 changed + added 的下载/复制；删除项搬到备份目录；完成后刷新基线。
#[tauri::command]
pub async fn pack_apply_diff(
    pack_id: String,
    items: Vec<ApplyItem>,
) -> Result<Vec<ApplyResult>, String> {
    if !valid_id(&pack_id) {
        return Err("整合包 ID 不合法".to_string());
    }
    let dir = instance_dir(&pack_id)?;
    let mut results = Vec::new();

    for item in items {
        let rel = item.path.trim().trim_start_matches('/').replace('\\', "/");
        let action = item.action.trim().to_lowercase();
        if rel.is_empty() || rel.contains("..") || rel.starts_with('/') {
            results.push(ApplyResult {
                path: item.path.clone(),
                action: action.clone(),
                ok: false,
                message: "不合法的文件路径".to_string(),
            });
            continue;
        }
        let dest = dir.join(&rel);
        if !dest.starts_with(&dir) {
            results.push(ApplyResult {
                path: item.path.clone(),
                action: action.clone(),
                ok: false,
                message: "文件路径越界".to_string(),
            });
            continue;
        }

        if action == "removed" {
            let backup = store::files_dir("pack-backups")
                .join(safe(&pack_id))
                .join(store::now_secs().to_string())
                .join(&rel);
            let outcome = move_to_backup(&dest, &backup);
            let (ok, message) = match outcome {
                Ok(()) => (true, format!("已备份到 {}", backup.to_string_lossy())),
                Err(e) => (false, e),
            };
            results.push(ApplyResult {
                path: item.path.clone(),
                action,
                ok,
                message,
            });
        } else if action == "added" || action == "changed" {
            let mut done = false;
            let mut last = String::new();
            for url in item.downloads.iter().filter(|u| !u.trim().is_empty()) {
                match fetch_to_file(url, &dest).await {
                    Ok(()) => {
                        done = true;
                        break;
                    }
                    Err(e) => last = e,
                }
            }
            // 没有 URL 或全部失败时，退回本地缓存（离线可用）。
            if !done {
                let cache = store::files_dir("pack-cache").join(store::file_name(&rel));
                if cache.is_file() {
                    match std::fs::copy(&cache, &dest) {
                        Ok(_) => done = true,
                        Err(e) => last = format!("复制缓存失败：{e}"),
                    }
                }
            }
            let message = if done {
                format!("{action} 完成")
            } else if item.downloads.is_empty() {
                "离线：没有下载地址，本地缓存也没有该文件".to_string()
            } else {
                format!("下载失败（可能离线）：{last}")
            };
            results.push(ApplyResult {
                path: item.path.clone(),
                action,
                ok: done,
                message,
            });
        } else {
            results.push(ApplyResult {
                path: item.path.clone(),
                action: action.clone(),
                ok: false,
                message: format!("未知操作：{action}"),
            });
        }
    }

    // 应用成功后刷新基线，避免下次又把同一批文件报成变化。
    let sp = snapshot_path(&pack_id);
    let current = scan_dir(&dir);
    let snap = Snapshot {
        pack_id: pack_id.clone(),
        at: store::now_secs(),
        files: current
            .iter()
            .map(|f| SnapFile {
                path: f.path.clone(),
                size: f.size,
                sha256: f.sha256.clone(),
                downloads: Vec::new(),
            })
            .collect(),
    };
    let _ = save_snapshot(&sp, &snap);

    Ok(results)
}

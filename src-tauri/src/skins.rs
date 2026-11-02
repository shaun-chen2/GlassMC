//! 皮肤缓存与皮肤库（Blessing Skin）。
//!
//! 缓存文件名里带贴图地址的短哈希 —— 站点换了皮肤，地址就变，旧缓存自动失效。
//! 皮肤库接口必须带 `X-Requested-With: XMLHttpRequest`，否则站点返回 HTML 外壳。

use base64::Engine;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::account::Account;
use crate::download;
use crate::paths;

fn key_of(acc: &Account) -> String {
    let raw = acc
        .uuid
        .clone()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| acc.username.clone());
    raw.replace([':', '/', '\\'], "_")
}

/// FNV-1a 短哈希，用于把贴图地址编进文件名。
fn short_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    to_base36(h % 0x0FFF_FFFF)
}

fn to_base36(mut v: u64) -> String {
    if v == 0 {
        return "0".into();
    }
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while v > 0 {
        out.push(DIGITS[(v % 36) as usize]);
        v /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

pub fn cache_file(acc: &Account) -> PathBuf {
    let k = key_of(acc);
    let suffix = acc
        .skin_url
        .as_deref()
        .filter(|u| !u.is_empty())
        .map(|u| format!("-{}", short_hash(u)))
        .unwrap_or_default();
    paths::paths()
        .skins_dir
        .join(format!("{k}{suffix}.png"))
}

/// 确保账号皮肤在本地，返回文件路径。取不到时返回 None（前端用默认皮肤）。
pub async fn ensure(acc: &Account) -> Option<PathBuf> {
    // 1) 本地指定的皮肤优先（皮肤库套用走这里）
    if let Some(local) = acc.skin_local_path.as_deref().filter(|p| !p.is_empty()) {
        let p = PathBuf::from(local);
        if p.is_file() {
            return Some(p);
        }
    }
    // 2) 已缓存
    let file = cache_file(acc);
    if is_png(&file) {
        return Some(file);
    }
    // 3) 下载
    let url = acc.skin_url.clone().filter(|u| !u.is_empty())?;
    if let Some(parent) = file.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    if download::download(&url, &file, None).await.is_err() {
        let _ = tokio::fs::remove_file(&file).await;
        return None;
    }
    if !is_png(&file) {
        let _ = tokio::fs::remove_file(&file).await;
        println!("[skin] 站点返回的不是图片：{url}");
        return None;
    }
    purge_stale(acc, &file);
    Some(file)
}

/// 强制重新下载。
pub async fn refresh(acc: &Account) -> Option<PathBuf> {
    let f = cache_file(acc);
    let _ = tokio::fs::remove_file(&f).await;
    ensure(acc).await
}

fn purge_stale(acc: &Account, keep: &Path) {
    let k = key_of(acc);
    let dir = paths::paths().skins_dir.clone();
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    for entry in rd.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(&k) || !name.ends_with(".png") {
            continue;
        }
        if entry.path() == keep {
            continue;
        }
        let _ = std::fs::remove_file(entry.path());
    }
}

fn is_png(p: &Path) -> bool {
    let Ok(bytes) = std::fs::read(p) else { return false };
    bytes.len() > 64 && bytes.starts_with(&[0x89, b'P', b'N', b'G'])
}

/// 把皮肤读成 data URL（前端 canvas 直接用）。
pub fn data_url(p: &Path) -> Option<String> {
    let bytes = std::fs::read(p).ok()?;
    if bytes.len() < 64 {
        return None;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

// ---------------------------------------------------------------- 皮肤库

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SkinItem {
    pub tid: i64,
    pub name: String,
    /// steve / alex
    pub kind: String,
    pub likes: i64,
    pub author: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SkinPage {
    pub items: Vec<SkinItem>,
    pub page: u32,
    pub last_page: u32,
    pub total: u32,
}

fn library_base(base: &str) -> String {
    base.trim().trim_end_matches('/').to_string()
}

/// 皮肤库列表。注意：`q` 与 `category` 服务端会忽略，搜索只能前端过滤。
pub async fn library_page(base: &str, page: u32) -> Result<SkinPage, String> {
    let b = library_base(base);
    if b.is_empty() {
        return Err("皮肤站地址为空，请到设置里填写".into());
    }
    let url = format!("{b}/skinlib/list?page={}&category=skin", page.max(1));
    let resp = crate::net::client()
        .get(&url)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("访问皮肤站失败：{e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(format!("皮肤站返回 HTTP {status}"));
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "皮肤站返回的不是 JSON（可能需要登录）".to_string())?;
    let items = v
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|row| {
                    Some(SkinItem {
                        tid: row.get("tid").and_then(|x| x.as_i64())?,
                        name: row
                            .get("name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("未命名")
                            .to_string(),
                        kind: row
                            .get("type")
                            .and_then(|x| x.as_str())
                            .unwrap_or("steve")
                            .to_string(),
                        likes: row.get("likes").and_then(|x| x.as_i64()).unwrap_or(0),
                        author: row
                            .get("nickname")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(SkinPage {
        items,
        page: v.get("current_page").and_then(|x| x.as_u64()).unwrap_or(page as u64) as u32,
        last_page: v.get("last_page").and_then(|x| x.as_u64()).unwrap_or(1) as u32,
        total: v.get("total").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
    })
}

/// 下载某个条目的原始皮肤到本地缓存，返回路径。
pub async fn library_texture(base: &str, tid: i64) -> Result<PathBuf, String> {
    let b = library_base(base);
    let file = paths::paths().skins_dir.join(format!("library-{tid}.png"));
    if is_png(&file) {
        return Ok(file);
    }
    let info_url = format!("{b}/skinlib/info/{tid}");
    let resp = crate::net::client()
        .get(&info_url)
        .header("X-Requested-With", "XMLHttpRequest")
        .send()
        .await
        .map_err(|e| format!("查询皮肤详情失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("查询皮肤详情失败（HTTP {}）", resp.status().as_u16()));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| "皮肤详情不是 JSON".to_string())?;
    let hash = v
        .get("hash")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("皮肤详情里没有 hash")?;

    // 原始贴图路径不带 .png（带后缀会 404）
    download::download(&format!("{b}/textures/{hash}"), &file, None)
        .await
        .map_err(|e| format!("下载皮肤失败：{e}"))?;
    if !is_png(&file) {
        let _ = std::fs::remove_file(&file);
        return Err("皮肤站返回的不是有效图片".into());
    }
    Ok(file)
}

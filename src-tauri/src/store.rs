//! 功能模块的共享小工具：独立 JSON 存储、目录复制、压缩包、data URL。
//!
//! 每个新功能用不同的 `name` 存自己的 JSON，互不干扰，也不需要改 `config.rs`：
//!   `data_root/feature-data/<name>.json`
//!   `data_root/feature-files/<name>/...`
//!
//! 所有函数都尽量「失败不致命」，坏文件返回默认值，避免一个功能把启动器拖垮。

use base64::Engine;
use serde::{de::DeserializeOwned, Serialize};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::paths;

fn safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "unnamed".to_string()
    } else {
        s
    }
}

pub fn json_path(name: &str) -> PathBuf {
    paths::paths()
        .data_root
        .join("feature-data")
        .join(format!("{}.json", safe(name)))
}

pub fn files_dir(name: &str) -> PathBuf {
    paths::paths()
        .data_root
        .join("feature-files")
        .join(safe(name))
}

pub fn load<T: DeserializeOwned + Default>(name: &str) -> T {
    match std::fs::read_to_string(json_path(name)) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

pub fn save<T: Serialize>(name: &str, value: &T) -> Result<(), String> {
    let p = json_path(name);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 短 id：时间戳 + 进程号，够界面列表用，不追求全局唯一。
pub fn short_id() -> String {
    format!("{:x}{:x}", now_secs(), std::process::id())
}

/// 递归复制目录，返回复制的文件数。符号链接按文件复制，不跟随。
pub fn copy_dir(src: &Path, dst: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let mut count = 0usize;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            count += copy_dir(&from, &to)?;
        } else if ty.is_file() {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
}

/// 把目录打成 zip（相对路径），返回写入的文件数。
pub fn zip_dir(src: &Path, dst: &Path) -> Result<usize, String> {
    let file = std::fs::File::create(dst).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let mut count = 0;
    zip_walk(src, src, &mut zip, &opts, &mut count)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(count)
}

fn zip_walk(
    root: &Path,
    dir: &Path,
    zip: &mut zip::ZipWriter<std::fs::File>,
    opts: &zip::write::FileOptions<'_, ()>,
    count: &mut usize,
) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let rel = p.strip_prefix(root).unwrap_or(&p);
        let name = rel.to_string_lossy().replace('\\', "/");
        if name.is_empty() {
            continue;
        }
        if p.is_dir() {
            zip.add_directory(format!("{name}/"), *opts)
                .map_err(|e| e.to_string())?;
            zip_walk(root, &p, zip, opts, count)?;
        } else if p.is_file() {
            zip.start_file(&name, *opts).map_err(|e| e.to_string())?;
            let mut f = std::fs::File::open(&p).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
            *count += 1;
        }
    }
    Ok(())
}

/// 安全解压：拒绝绝对路径、盘符、`..` 与符号链接条目，防 Zip Slip。
pub fn unzip_to(archive_path: &Path, dst: &Path) -> Result<usize, String> {
    let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let mut count = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let raw = entry.name().replace('\\', "/");
        if raw.starts_with('/') || raw.contains(':') || raw.contains("..") {
            continue;
        }
        let rel = Path::new(&raw);
        let target = dst.join(rel);
        // 再次用 Component 校验，防止 `a/../b` 这类漏网之鱼
        if target.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            continue;
        }
        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        if entry.is_file() {
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
}

/// 本地文件转 data URL，给 WebView 直接显示缩略图，不需要开 asset 协议。
pub fn data_url(path: &Path, max_bytes: u64) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > max_bytes {
        return Err(format!("文件太大（{} 字节）", meta.len()));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mime = match path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "txt" | "log" => "text/plain; charset=utf-8",
        "json" => "application/json",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// 只保留文件名，避免把整条路径显示在界面上。
pub fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

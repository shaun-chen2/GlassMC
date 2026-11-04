//! 通用下载器：重试、状态码校验、SHA1 校验、原子落盘。
//!
//! 三个必须保留的细节（都是从原生版踩坑换来的）：
//! 1. 先写 `.part` 再改名 —— 半截文件不会被当成完整文件；
//! 2. 校验 HTTP 状态码 —— 否则 404 页面会被静默写成文件；
//! 3. 带 User-Agent（由 `net::client()` 统一提供）—— BMCLAPI 对空 UA 返回 403。

use sha1::{Digest, Sha1};
use std::path::Path;
use tokio::io::AsyncWriteExt;

use crate::net;

/// 单文件下载上限（防止把异常响应整个读进内存）
const MAX_FILE: u64 = 1 << 30; // 1 GiB

/// 下载到目标路径；`expect_sha1` 非空时校验内容。
pub async fn download(url: &str, dest: &Path, expect_sha1: Option<&str>) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..3u32 {
        match once(url, dest).await {
            Ok(sum) => {
                if let Some(exp) = expect_sha1.map(|s| s.trim().to_lowercase()) {
                    if !exp.is_empty() && sum != exp {
                        let _ = tokio::fs::remove_file(dest).await;
                        return Err(format!("{} 校验失败（内容与记录不一致）", file_label(dest)));
                    }
                }
                return Ok(());
            }
            Err(e) => {
                // 4xx 是确定性错误（地址不对/无权限），重试没有意义
                if e.starts_with("HTTP 4") {
                    return Err(format!("{e}（{url}）"));
                }
                last = e;
                let _ = tokio::fs::remove_file(part_path(dest)).await;
                tokio::time::sleep(std::time::Duration::from_millis(300 * (attempt as u64 + 1)))
                    .await;
            }
        }
    }
    Err(format!("{last}（{url}）"))
}

/// 依次尝试多个地址，全部失败时返回最后一次错误。
pub async fn download_any(
    urls: &[String],
    dest: &Path,
    expect_sha1: Option<&str>,
) -> Result<(), String> {
    let mut last = String::new();
    for u in urls.iter().filter(|u| !u.trim().is_empty()) {
        match download(u, dest, expect_sha1).await {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    if last.is_empty() {
        Err(format!("没有可用的下载地址：{}", file_label(dest)))
    } else {
        Err(last)
    }
}

/// 拉取文本（JSON / XML / 纯文本）。
pub async fn text(url: &str) -> Result<String, String> {
    let resp = net::client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    resp.text().await.map_err(|e| format!("读取响应失败：{e}"))
}

fn part_path(dest: &Path) -> std::path::PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".part");
    std::path::PathBuf::from(s)
}

fn file_label(dest: &Path) -> String {
    dest.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| dest.to_string_lossy().to_string())
}

async fn once(url: &str, dest: &Path) -> Result<String, String> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("创建目录失败：{e}"))?;
    }

    let resp = net::client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("网络错误：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    let expected = resp.content_length().unwrap_or(0);
    if expected > MAX_FILE {
        return Err(format!("文件过大（{} 字节）", expected));
    }

    let tmp = part_path(dest);
    let _ = tokio::fs::remove_file(&tmp).await;
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("创建文件失败：{e}"))?;

    let mut hasher = Sha1::new();
    let mut total: u64 = 0;
    let mut resp = resp;
    loop {
        let chunk = resp.chunk().await.map_err(|e| format!("传输中断：{e}"))?;
        let Some(bytes) = chunk else { break };
        if bytes.is_empty() {
            continue;
        }
        total += bytes.len() as u64;
        if total > MAX_FILE {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err("响应超出大小上限".into());
        }
        hasher.update(&bytes);
        file.write_all(&bytes)
            .await
            .map_err(|e| format!("写入失败：{e}"))?;
    }
    let _ = file.flush().await;
    drop(file);

    if total == 0 {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err("响应为空".into());
    }

    let _ = tokio::fs::remove_file(dest).await;
    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(|e| format!("落盘失败：{e}"))?;

    Ok(format!("{:x}", hasher.finalize()))
}

/// 计算文件 SHA1（小写 hex）。
pub fn sha1_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| format!("打开文件失败：{e}"))?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("读取失败：{e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

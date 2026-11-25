//! 更新检查：查 GitHub Releases 的最新 tag，按平台挑安装包，下载到「下载」目录。
//!
//! 只负责「查版本 / 挑安装包 / 下载」；自动安装与替换流程在 `autoupdate.rs`。
//! macOS 产物是 ad-hoc 签名（没有 Developer ID、没有公证），自动安装只能尽力而为：
//! 替换后去掉 quarantine 并重新打开，仍可能被 Gatekeeper 拦一次。

use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use crate::install::Progress;
use crate::net;
use crate::paths;

const RELEASE_API: &str = "https://api.github.com/repos/shaun-chen2/GlassMC/releases/latest";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub asset_name: String,
    pub asset_url: String,
    pub size: u64,
    pub page_url: String,
    pub notes: String,
}

/// 本机版本。`GLASSMC_VERSION_OVERRIDE` 只用于本机测试更新提示，
/// 正常安装不会带这个环境变量。
pub fn current_version() -> String {
    std::env::var("GLASSMC_VERSION_OVERRIDE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

pub async fn check() -> Result<UpdateInfo, String> {
    let resp = net::client()
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("检查更新失败：{e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(match status {
            403 | 429 => "查询太频繁，稍后再试".to_string(),
            404 => "还没有发布任何版本".to_string(),
            _ => format!("GitHub 返回 {status}"),
        });
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Release 内容解析失败：{e}"))?;
    let latest = v
        .get("tag_name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .trim_start_matches('v')
        .to_string();
    if latest.is_empty() {
        return Err("Release 没有标注版本号".into());
    }
    let assets = v
        .get("assets")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let asset = pick_asset(&assets);
    let current = current_version();
    Ok(UpdateInfo {
        has_update: semver_gt(&latest, &current),
        latest,
        current,
        asset_name: asset
            .and_then(|a| a.get("name").and_then(|x| x.as_str()))
            .unwrap_or("")
            .to_string(),
        asset_url: asset
            .and_then(|a| a.get("browser_download_url").and_then(|x| x.as_str()))
            .unwrap_or("")
            .to_string(),
        size: asset
            .and_then(|a| a.get("size").and_then(|x| x.as_u64()))
            .unwrap_or(0),
        page_url: v
            .get("html_url")
            .and_then(|x| x.as_str())
            .unwrap_or("https://github.com/shaun-chen2/GlassMC/releases")
            .to_string(),
        notes: v
            .get("body")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .chars()
            .take(600)
            .collect(),
    })
}

/// 下载当前平台对应的安装包，返回落盘路径。
pub async fn download(app: Option<tauri::AppHandle>) -> Result<String, String> {
    let info = check().await?;
    if info.asset_url.trim().is_empty() {
        return Err("这个版本没有发布当前平台的安装包".into());
    }
    let ext = ext_of(&info.asset_name);
    let dir = download_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建下载目录失败：{e}"))?;
    let dest = unique_path(&dir, &format!("GlassMC-{}", info.latest), &ext);
    let part = dest.with_extension(format!("{}.part", ext.trim_start_matches('.')));

    let prog = Progress::new(app, "update:download");
    prog.stage("下载更新包", 100, 0.0, 1.0);

    let resp = net::client()
        .get(&info.asset_url)
        .send()
        .await
        .map_err(|e| format!("下载失败：{e}"))?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("下载失败：HTTP {status}"));
    }
    let total = resp.content_length().unwrap_or(info.size);
    let mut out = tokio::fs::File::create(&part)
        .await
        .map_err(|e| format!("写入下载文件失败：{e}"))?;
    let mut resp = resp;
    let mut got: u64 = 0;
    let mut last_pct: u64 = 0;
    loop {
        let chunk = match resp.chunk().await {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(format!("下载中断：{e}"));
            }
        };
        if let Err(e) = out.write_all(&chunk).await {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(format!("写入下载文件失败：{e}"));
        }
        got += chunk.len() as u64;
        let pct = got
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(0)
            .min(100);
        if pct > last_pct {
            last_pct = pct;
            prog.tick(true);
        }
    }
    out.flush()
        .await
        .map_err(|e| format!("写入下载文件失败：{e}"))?;
    drop(out);

    if total > 0 && got < total {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(format!("下载不完整（{got}/{total} 字节）"));
    }
    std::fs::rename(&part, &dest).map_err(|e| format!("保存安装包失败：{e}"))?;
    prog.note("更新包已就绪");
    Ok(dest.to_string_lossy().to_string())
}

fn download_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
        .unwrap_or_else(|| paths::paths().data_root.clone())
}

fn ext_of(name: &str) -> String {
    let lower = name.to_lowercase();
    for ext in [".tar.gz", ".dmg", ".exe", ".deb", ".zip"] {
        if lower.ends_with(ext) {
            return ext.to_string();
        }
    }
    ".bin".to_string()
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{stem}{ext}"));
    let mut i = 2;
    while p.exists() && i < 1000 {
        p = dir.join(format!("{stem}-{i}{ext}"));
        i += 1;
    }
    p
}

/// 当前平台的安装包优先级；名字与 `release.yml` 里的产物重命名保持一致。
fn asset_prefs() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["GlassMC.dmg"]
    } else if cfg!(target_os = "windows") {
        &["GlassMC-Setup.exe"]
    } else {
        &["GlassMC.deb", "GlassMC-linux.tar.gz"]
    }
}

fn pick_asset(assets: &[serde_json::Value]) -> Option<&serde_json::Value> {
    for want in asset_prefs() {
        if let Some(a) = assets
            .iter()
            .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(*want))
        {
            return Some(a);
        }
    }
    let exts: &[&str] = if cfg!(target_os = "macos") {
        &[".dmg"]
    } else if cfg!(target_os = "windows") {
        &[".exe"]
    } else {
        &[".deb", ".tar.gz"]
    };
    assets.iter().find(|a| {
        a.get("name")
            .and_then(|n| n.as_str())
            .map(|n| {
                let n = n.to_lowercase();
                exts.iter().any(|e| n.ends_with(e))
            })
            .unwrap_or(false)
    })
}

/// 只比较 `x.y.z` 三段数字；预发布（带 `-`）视为小于同号正式版。
/// 解析不出来就退回字符串比较 —— 宁可多提示一次更新，也不要漏掉。
fn semver_gt(a: &str, b: &str) -> bool {
    fn parse(s: &str) -> Option<(Vec<u64>, &str)> {
        let s = s.trim().trim_start_matches('v');
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, p),
            None => (s, ""),
        };
        let mut nums: Vec<u64> = Vec::new();
        for part in core.split('.') {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                return None;
            }
            nums.push(digits.parse().ok()?);
        }
        while nums.len() < 3 {
            nums.push(0);
        }
        Some((nums, pre))
    }
    let (an, ap) = match parse(a) {
        Some(v) => v,
        None => return a.trim() > b.trim(),
    };
    let (bn, bp) = match parse(b) {
        Some(v) => v,
        None => return a.trim() > b.trim(),
    };
    if an != bn {
        return an > bn;
    }
    match (ap.is_empty(), bp.is_empty()) {
        (true, true) => false,
        (true, false) => true,
        (false, true) => false,
        (false, false) => ap > bp,
    }
}

#[cfg(test)]
mod tests {
    use super::semver_gt;

    #[test]
    fn compares_release_versions() {
        assert!(semver_gt("1.3.10", "1.3.9"));
        assert!(semver_gt("v1.4", "1.3.9"));
        assert!(semver_gt("2.0.0", "1.99.99"));
        assert!(!semver_gt("1.3.2", "1.3.2"));
        assert!(!semver_gt("1.3.1", "1.3.2"));
    }

    #[test]
    fn prerelease_is_older() {
        assert!(semver_gt("1.3.2", "1.3.2-beta"));
        assert!(!semver_gt("1.3.2-beta", "1.3.2"));
        assert!(semver_gt("1.3.2-beta.2", "1.3.2-beta.1"));
    }

    /// 真实打一次 GitHub API（需要联网）。
    /// `cargo test -- --ignored update`
    #[tokio::test]
    #[ignore]
    async fn reads_latest_release() {
        // 不写死 latest：每发一版，这个测试都应该继续成立
        std::env::set_var("GLASSMC_VERSION_OVERRIDE", "1.0.0");
        let info = super::check().await.unwrap();
        assert_eq!(info.current, "1.0.0");
        assert!(!info.latest.is_empty(), "没有读到最新版本号");
        assert!(super::semver_gt(&info.latest, "1.0.0"));
        assert!(info.has_update);
        assert!(!info.asset_url.is_empty(), "没有挑到安装包");
        if cfg!(target_os = "macos") {
            assert_eq!(info.asset_name, "GlassMC.dmg");
        }
        std::env::remove_var("GLASSMC_VERSION_OVERRIDE");
    }

    /// 真实下载安装包到临时 HOME 下的 Downloads，顺便验证流式写入与大小校验。
    /// `cargo test -- --ignored update`
    #[tokio::test]
    #[ignore]
    async fn downloads_installer_to_downloads_dir() {
        let root = std::env::temp_dir().join(format!("glassmc-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Downloads")).unwrap();
        std::env::set_var("HOME", &root);
        let path = super::download(None).await.unwrap();
        let p = std::path::PathBuf::from(&path);
        assert!(p.is_file(), "{path} 不存在");
        assert!(
            std::fs::metadata(&p).unwrap().len() > 1_000_000,
            "文件太小：{path}"
        );
        assert!(p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("GlassMC-"));
        let _ = std::fs::remove_dir_all(&root);
    }
}

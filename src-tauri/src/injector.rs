//! authlib-injector：外置登录进游戏时必须挂的 javaagent。
//!
//! 没有它，客户端会去问 Mojang 要皮肤（必然失败），结果就是「登录成功但进游戏是
//! 默认皮肤、也进不了皮肤站的正版服」。jar 从 BMCLAPI 镜像取，官方源兜底。

use sha2::{Digest, Sha256};
use std::path::PathBuf;

use crate::download;
use crate::paths;

const META_URLS: [&str; 2] = [
    "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/latest.json",
    "https://authlib-injector.yushi.moe/artifact/latest.json",
];

pub fn jar_path() -> PathBuf {
    paths::paths().data_root.join("authlib-injector.jar")
}

fn version_file() -> PathBuf {
    paths::paths().data_root.join("authlib-injector.version")
}

pub fn version() -> Option<String> {
    std::fs::read_to_string(version_file())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 本地是否已就绪（大小合理即视为可用）。
pub fn local() -> Option<PathBuf> {
    let p = jar_path();
    match std::fs::metadata(&p) {
        Ok(m) if m.is_file() && m.len() > 100_000 => Some(p),
        _ => None,
    }
}

/// 确保注入器可用；`force` 为 true 时重新下载。
pub async fn ensure(force: bool) -> Result<PathBuf, String> {
    if !force {
        if let Some(p) = local() {
            return Ok(p);
        }
    }
    let mut last = String::new();
    for meta_url in META_URLS {
        let body = match download::text(meta_url).await {
            Ok(b) => b,
            Err(e) => {
                last = e;
                continue;
            }
        };
        let v: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                last = format!("元数据解析失败：{e}");
                continue;
            }
        };
        let url = v.get("download_url").and_then(|x| x.as_str());
        let ver = v.get("version").and_then(|x| x.as_str()).unwrap_or("");
        let (Some(url), ) = (url,) else {
            last = "元数据缺少 download_url".into();
            continue;
        };

        let tmp = paths::paths().data_root.join("authlib-injector.jar.part");
        if let Err(e) = download::download(url, &tmp, None).await {
            last = e;
            continue;
        }
        // sha256 校验（元数据里带了就必须对得上）
        if let Some(expected) = v
            .get("checksums")
            .and_then(|c| c.get("sha256"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_lowercase())
            .filter(|s| !s.is_empty())
        {
            let actual = sha256_file(&tmp)?;
            if actual != expected {
                let _ = std::fs::remove_file(&tmp);
                last = "校验失败（文件与元数据不一致）".into();
                continue;
            }
        }
        let dest = jar_path();
        let _ = std::fs::remove_file(&dest);
        std::fs::rename(&tmp, &dest).map_err(|e| format!("写入注入器失败：{e}"))?;
        let _ = std::fs::write(version_file(), ver);
        return Ok(dest);
    }
    Err(format!(
        "authlib-injector 下载失败：{}",
        if last.is_empty() { "所有镜像都不可用".into() } else { last }
    ))
}

#[allow(dead_code)]
pub fn remove() -> Result<(), String> {
    let _ = std::fs::remove_file(jar_path());
    let _ = std::fs::remove_file(version_file());
    Ok(())
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| format!("打开文件失败：{e}"))?;
    let mut hasher = Sha256::new();
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

/// 注入器参数：`-javaagent:<jar>=<API 基址>`。
pub fn agent_arg(jar: &std::path::Path, base: &str) -> String {
    format!("-javaagent:{}={base}", jar.to_string_lossy())
}

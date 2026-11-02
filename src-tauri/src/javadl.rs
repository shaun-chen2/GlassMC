//! Java 运行环境安装：Adoptium（Temurin）官方 API + 清华镜像。
//!
//! 镜像路径是 `mirrors.tuna.tsinghua.edu.cn/Adoptium/{feature}/jdk/{arch}/{os}/{文件名}`，
//! 文件名必须来自官方 API（含 `%2B` 之类编码，要解码后再拼）。Apple Silicon 上 JDK 8
//! 没有 aarch64 构建，API 会返回空数组，此时回退到 x64（Rosetta 可运行）。

use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::download;
use crate::install::Progress;
use crate::java::{self, JavaInstall};
use crate::paths;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JavaCandidate {
    pub feature: u32,
    pub label: String,
    pub installed: bool,
}

fn arch_of() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    }
}

fn os_of() -> &'static str {
    if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

/// Adoptium 的镜像里使用的 os 段
fn os_segment() -> &'static str {
    if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

pub fn candidates() -> Vec<JavaCandidate> {
    let installed = java::installed("");
    [8u32, 11, 17, 21, 25]
        .into_iter()
        .map(|f| JavaCandidate {
            feature: f,
            label: format!("Java {f}"),
            installed: installed.iter().any(|j| j.version == f && j.source == "managed"),
        })
        .collect()
}

struct Package {
    url: String,
    name: String,
    arch: String,
}

/// 向 Adoptium 查询某个主版本的发行包。返回（官方下载地址、文件名、真实架构）。
async fn lookup(feature: u32, arch: &str) -> Result<Package, String> {
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{feature}/hotspot?architecture={arch}&image_type=jdk&os={}&vendor=eclipse",
        os_of()
    );
    let body = download::text(&url)
        .await
        .map_err(|e| format!("查询 Java {feature} 发行包失败：{e}"))?;
    let list: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Java 发行包信息解析失败：{e}"))?;
    let first = list
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| format!("Adoptium 没有适用于 {arch}-{} 的 Java {feature}", os_of()))?;
    let pkg = first
        .get("binary")
        .and_then(|b| b.get("package"))
        .ok_or_else(|| "发行包缺少 package 字段".to_string())?;
    let link = pkg
        .get("link")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "发行包缺少下载地址".to_string())?;
    let name = pkg
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "发行包缺少文件名".to_string())?;
    Ok(Package {
        url: link.to_string(),
        name: name.to_string(),
        arch: arch.to_string(),
    })
}

/// 安装（或重装）某个主版本的 JDK。
pub async fn install(
    app: Option<tauri::AppHandle>,
    feature: u32,
    mirror: &str,
) -> Result<JavaInstall, String> {
    let prog = Progress::new(app, format!("java:{feature}"));
    prog.note(&format!("查询 Java {feature} 发行包"));

    // Apple Silicon 上 JDK 8/11 可能没有原生架构，回退 x64
    let pkg = match lookup(feature, arch_of()).await {
        Ok(p) => p,
        Err(e) if arch_of() == "aarch64" => {
            prog.note(&format!("{e}，改用 x64 版本"));
            lookup(feature, "x64").await?
        }
        Err(e) => return Err(e),
    };

    let mirror_base = mirror.trim_end_matches('/');
    let encoded = pkg.name.clone();
    let urls = vec![
        format!(
            "{}/{}/jdk/{}/{}/{}",
            mirror_base,
            feature,
            pkg.arch,
            os_segment(),
            encoded
        ),
        pkg.url.clone(),
    ];

    let tmp_dir = std::env::temp_dir().join(format!("glassmc-java-{feature}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("创建临时目录失败：{e}"))?;
    let archive = tmp_dir.join(&pkg.name);

    prog.stage(&format!("下载 Java {feature}"), 1, 0.0, 0.85);
    download::download_any(&urls, &archive, None)
        .await
        .map_err(|e| {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            format!("下载 Java {feature} 失败：{e}")
        })?;
    prog.tick(true);

    prog.stage("解压", 1, 0.85, 0.12);
    let staging = paths::paths()
        .java_dir
        .join(format!(".staging-{feature}"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| format!("创建目录失败：{e}"))?;

    let is_zip = pkg.name.to_lowercase().ends_with(".zip");
    let extract = tokio::task::spawn_blocking({
        let archive = archive.clone();
        let staging = staging.clone();
        move || extract_archive(&archive, &staging, is_zip)
    })
    .await
    .map_err(|e| format!("解压任务失败：{e}"))?;
    if let Err(e) = extract {
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(format!("解压 Java 失败：{e}"));
    }

    let home = find_home(&staging).ok_or_else(|| {
        let _ = std::fs::remove_dir_all(&staging);
        format!("解压后没有找到 bin/java（Java {feature}）")
    })?;

    // 落位：java/{feature}；若解压结果多套了一层目录，就把这一层提上来
    let target = paths::paths().java_dir.join(feature.to_string());
    let _ = std::fs::remove_dir_all(&target);
    let moved = if home == staging {
        std::fs::rename(&staging, &target)
    } else {
        let parent = home.parent().map(|p| p.to_path_buf());
        let result = std::fs::rename(&home, &target);
        // 顺带清掉 staging
        let _ = std::fs::remove_dir_all(&staging);
        let _ = parent;
        result
    };
    let _ = std::fs::remove_dir_all(&tmp_dir);
    moved.map_err(|e| format!("安装 Java 失败：{e}"))?;

    prog.tick(true);
    java::make(target, "managed").ok_or_else(|| format!("Java {feature} 安装后校验失败"))
}

/// 删除我们托管的某个 Java。
pub fn remove(feature: u32) -> Result<(), String> {
    let dir = paths::paths().java_dir.join(feature.to_string());
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("删除失败：{e}"))?;
    }
    Ok(())
}

fn extract_archive(archive: &Path, dest: &Path, is_zip: bool) -> Result<(), String> {
    if is_zip {
        let file = std::fs::File::open(archive).map_err(|e| format!("打开压缩包失败：{e}"))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("读取 zip 失败：{e}"))?;
        zip.extract(dest).map_err(|e| format!("解压失败：{e}"))?;
        return Ok(());
    }
    let mut cmd = std::process::Command::new("tar");
    cmd.arg("-xzf").arg(archive).arg("-C").arg(dest);
    let out = cmd.output().map_err(|e| format!("调用 tar 失败：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "tar 退出码 {:?}：{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// 在解压目录里找 JAVA_HOME（mac 是 `xxx/Contents/Home`，其它平台是根）。
fn find_home(root: &Path) -> Option<PathBuf> {
    if has_java(root) {
        return Some(root.to_path_buf());
    }
    for child in dirs_in(root) {
        let mac_home = child.join("Contents").join("Home");
        if has_java(&mac_home) {
            return Some(mac_home);
        }
        if has_java(&child) {
            return Some(child);
        }
        for grand in dirs_in(&child) {
            let gh = grand.join("Contents").join("Home");
            if has_java(&gh) {
                return Some(gh);
            }
            if has_java(&grand) {
                return Some(grand);
            }
        }
    }
    None
}

fn has_java(home: &Path) -> bool {
    let exe = if cfg!(target_os = "windows") {
        home.join("bin").join("java.exe")
    } else {
        home.join("bin").join("java")
    };
    exe.is_file()
}

fn dirs_in(p: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(p) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|x| x.is_dir())
            .collect(),
        Err(_) => Vec::new(),
    }
}

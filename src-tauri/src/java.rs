//! Java 检测：环境变量、系统安装目录、我们托管的目录三路合并。
//!
//! 只认真正存在 `bin/java` 的目录 —— 半途失败留下的空目录如果进了列表，
//! 启动时会报 "file doesn't exist"。

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::paths;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JavaInstall {
    /// 可执行文件
    pub path: String,
    /// 安装根目录（JAVA_HOME）
    pub home: String,
    /// 主版本：8 / 17 / 21 …
    pub version: u32,
    pub vendor: String,
    pub arch: String,
    /// system / managed / manual
    pub source: String,
    pub label: String,
}

fn exe_in(home: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        home.join("bin").join("java.exe")
    } else {
        home.join("bin").join("java")
    }
}

fn subdirs(p: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(p) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn read_release(home: &Path) -> HashMap<String, String> {
    let mut m = HashMap::new();
    if let Ok(s) = std::fs::read_to_string(home.join("release")) {
        for line in s.lines() {
            if let Some((k, v)) = line.split_once('=') {
                m.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
            }
        }
    }
    m
}

fn parse_major(raw: &str) -> Option<u32> {
    let s = raw.trim().trim_matches('"').trim();
    let body = s.strip_prefix("1.").unwrap_or(s);
    let digits: String = body.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn probe_version(exe: &Path) -> Option<u32> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("-version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    let s = String::from_utf8_lossy(&out.stderr);
    let line = s
        .lines()
        .find(|l| l.contains("version"))
        .or_else(|| s.lines().next())?;
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    parse_major(&line[start..end])
}

pub fn make(home: PathBuf, source: &str) -> Option<JavaInstall> {
    let exe = exe_in(&home);
    if !exe.is_file() {
        return None;
    }
    let rel = read_release(&home);
    let version = rel
        .get("JAVA_VERSION")
        .and_then(|v| parse_major(v))
        .or_else(|| probe_version(&exe))?;
    let vendor = rel
        .get("IMPLEMENTOR")
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| rel.get("JAVA_VENDOR").cloned())
        .unwrap_or_else(|| name_of(&home));
    let arch = rel
        .get("OS_ARCH")
        .cloned()
        .unwrap_or_else(|| "unknown".into());
    let label = format!(
        "{vendor} {version}{}",
        if arch == "unknown" {
            String::new()
        } else {
            format!(" · {arch}")
        }
    );
    Some(JavaInstall {
        path: exe.to_string_lossy().to_string(),
        home: home.to_string_lossy().to_string(),
        version,
        vendor,
        arch,
        source: source.to_string(),
        label,
    })
}

fn candidate_homes() -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = Vec::new();

    // 环境变量
    if let Ok(home) = std::env::var("JAVA_HOME") {
        if !home.trim().is_empty() {
            out.push((PathBuf::from(home), "system"));
        }
    }

    // 托管目录：data/java/<x> 与 data/java/<x>/Contents/Home
    let managed = paths::paths().java_dir.clone();
    for d in subdirs(&managed) {
        out.push((d.clone(), "managed"));
        out.push((d.join("Contents").join("Home"), "managed"));
    }

    #[cfg(target_os = "macos")]
    {
        let mut bases: Vec<PathBuf> = vec![PathBuf::from("/Library/Java/JavaVirtualMachines")];
        if let Some(home) = dirs::home_dir() {
            bases.push(home.join("Library/Java/JavaVirtualMachines"));
        }
        for base in bases {
            for d in subdirs(&base) {
                out.push((d.join("Contents").join("Home"), "system"));
            }
        }
        for base in ["/opt/homebrew/opt", "/usr/local/opt"] {
            for d in subdirs(Path::new(base)) {
                if name_of(&d).starts_with("openjdk") {
                    out.push((d.clone(), "system"));
                }
            }
        }
        for base in ["/opt/homebrew/Cellar", "/usr/local/Cellar"] {
            for d in subdirs(Path::new(base)) {
                if name_of(&d).starts_with("openjdk") {
                    for v in subdirs(&d) {
                        out.push((v, "system"));
                    }
                }
            }
        }
        out.push((
            PathBuf::from("/Library/Internet Plug-Ins/JavaAppletPlugin.plugin/Contents/Home"),
            "system",
        ));
    }

    #[cfg(target_os = "windows")]
    {
        let mut roots: Vec<PathBuf> = vec![
            PathBuf::from(r"C:\Program Files\Java"),
            PathBuf::from(r"C:\Program Files\Eclipse Adoptium"),
            PathBuf::from(r"C:\Program Files\Microsoft"),
            PathBuf::from(r"C:\Program Files\Zulu"),
            PathBuf::from(r"C:\Program Files\BellSoft"),
            PathBuf::from(r"C:\Program Files\Amazon Corretto"),
            PathBuf::from(r"C:\Program Files\Semeru"),
        ];
        for key in [
            "LOCALAPPDATA",
            "APPDATA",
            "ProgramFiles",
            "ProgramFiles(x86)",
        ] {
            if let Ok(v) = std::env::var(key) {
                roots.push(PathBuf::from(&v).join("Programs").join("Eclipse Adoptium"));
                roots.push(
                    PathBuf::from(&v)
                        .join("Programs")
                        .join("Microsoft")
                        .join("jdk"),
                );
                roots.push(PathBuf::from(&v).join("Java"));
                roots.push(PathBuf::from(&v).join("Programs").join("Zulu"));
            }
        }
        for r in roots {
            for d in subdirs(&r) {
                out.push((d, "system"));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for base in ["/usr/lib/jvm", "/usr/java", "/opt", "/usr/share/java"] {
            for d in subdirs(Path::new(base)) {
                out.push((d.clone(), "system"));
                out.push((d.join("Contents").join("Home"), "system"));
            }
        }
    }

    out
}

/// 扫描全部可用 Java，按版本从高到低排列，同一路径只保留一次。
pub fn installed(manual: &str) -> Vec<JavaInstall> {
    let mut list: Vec<JavaInstall> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    if !manual.trim().is_empty() {
        let p = PathBuf::from(manual.trim());
        let home = if p
            .file_name()
            .map(|n| n == "java" || n == "java.exe")
            .unwrap_or(false)
        {
            p.parent().and_then(|x| x.parent()).map(|x| x.to_path_buf())
        } else {
            Some(p.clone())
        };
        if let Some(home) = home {
            if let Some(j) = make(home, "manual") {
                seen.push(PathBuf::from(&j.path));
                list.push(j);
            }
        }
    }

    for (home, source) in candidate_homes() {
        let Some(j) = make(home, source) else {
            continue;
        };
        let canon = std::fs::canonicalize(&j.path).unwrap_or_else(|_| PathBuf::from(&j.path));
        if seen.iter().any(|s| {
            let sc = std::fs::canonicalize(s).unwrap_or_else(|_| s.clone());
            sc == canon
        }) {
            continue;
        }
        seen.push(canon);
        list.push(j);
    }

    list.sort_by(|a, b| b.version.cmp(&a.version).then(a.label.cmp(&b.label)));
    list
}

/// 挑选最适合某版本要求的 Java：优先版本刚好够用的，其次版本最高的。
pub fn best_for(required: u32, manual: &str) -> Option<JavaInstall> {
    let all = installed(manual);
    if all.is_empty() {
        return None;
    }
    let mut fit: Vec<&JavaInstall> = all.iter().filter(|j| j.version >= required).collect();
    fit.sort_by_key(|j| j.version);
    fit.first()
        .map(|j| (*j).clone())
        .or_else(|| all.first().cloned())
}

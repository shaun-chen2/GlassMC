//! 分享社区 + 硬件面板。
//!
//! 设计原则：
//! - 不上传任何用户数据：分享包只写到「下载」目录，介绍文本与 BBCode 只在本地生成；
//! - 外部平台只打开搜索页，绝不声称能替用户发帖；
//! - 服务器面板读取另一个模块写进 `store("servers")` 的数组，结构不认识就返回空；
//! - 硬件信息通过外部命令获取，统一 3 秒超时，失败填「未知 / null」。

use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{paths, store};

// ---------------------------------------------------------------- 数据结构

/// 分享包导出结果。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ShareExport {
    pub path: String,
    pub files: usize,
    pub version: String,
    pub loader: String,
    pub mods: usize,
}

/// 服务器面板单项。
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerItem {
    pub name: String,
    pub address: String,
    pub last_ping_ms: i64,
    pub players_online: i64,
    pub motd: String,
    pub pinned: bool,
    pub online: bool,
}

/// 服务器面板汇总。
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerDashboard {
    pub total: usize,
    pub online: usize,
    pub pinned: usize,
    pub items: Vec<ServerItem>,
}

/// 便携版导出结果。
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PortableResult {
    pub path: String,
    pub files: usize,
    /// 实际复制了哪些部分（相对目标根目录）。
    pub copied: Vec<String>,
}

/// 静态硬件信息。
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub cpu: String,
    pub cores: usize,
    pub total_ram_gb: f64,
    pub gpu: String,
}

/// 粗略占用率；任一项都算不出来时整个命令返回 `null`。
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct HardwareUsage {
    pub cpu_percent: f64,
    pub mem_percent: f64,
}

// ---------------------------------------------------------------- 小工具

/// 路径片段清洗：版本 id 来自目录名，不能让它跑出目标目录。
fn safe_component(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let out = out.trim_matches(|c| c == '.' || c == '_').to_string();
    if out.is_empty() {
        "instance".to_string()
    } else {
        out
    }
}

/// 从 id / 继承链猜加载器；与 `manifest.rs` 的口径保持一致。
fn detect_loader(version_id: &str, inherits: Option<&str>) -> String {
    let mut s = version_id.to_lowercase();
    if let Some(parent) = inherits {
        s.push(' ');
        s.push_str(&parent.to_lowercase());
    }
    for key in ["neoforge", "forge", "fabric", "quilt"] {
        if s.contains(key) {
            return key.to_string();
        }
    }
    String::new()
}

/// 读本地版本 JSON，返回 (id, inheritsFrom, loader)。读不到就退回目录名。
fn version_meta(version_id: &str) -> (String, String, String) {
    let path = paths::paths()
        .versions_dir
        .join(version_id)
        .join(format!("{version_id}.json"));
    let value: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}));
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .unwrap_or(version_id)
        .to_string();
    let inherits = value
        .get("inheritsFrom")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let loader = value
        .get("loader")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .unwrap_or_else(|| detect_loader(version_id, Some(&inherits)));
    (id, inherits, loader)
}

/// 模组文件名清单（含被禁用的 `.jar.disabled`）。优先实例隔离目录。
fn mod_names(version_id: &str) -> Vec<String> {
    let p = paths::paths();
    let mut dirs = Vec::new();
    if !version_id.trim().is_empty() {
        dirs.push(p.versions_dir.join(version_id).join("mods"));
    }
    dirs.push(p.game_dir.join("mods"));

    let mut out: Vec<String> = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            if !entry.path().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if (lower.ends_with(".jar") || lower.ends_with(".jar.disabled")) && !out.contains(&name)
            {
                out.push(name);
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out.sort();
    out
}

fn brief_loader(loader: &str) -> &str {
    if loader.is_empty() {
        "原版"
    } else {
        loader
    }
}

// ---------------------------------------------------------------- 1. 分享

/// 把实例元数据 + 模组文件名清单 + 实例配置打成 zip，写到「下载」目录。
#[tauri::command]
pub fn share_export(version_id: String) -> Result<ShareExport, String> {
    if version_id.trim().is_empty() || version_id.contains('/') || version_id.contains("..") {
        return Err("版本 id 不合法".into());
    }
    let (id, inherits, loader) = version_meta(&version_id);
    let mods = mod_names(&version_id);

    let downloads = dirs::download_dir().ok_or_else(|| "找不到系统下载目录".to_string())?;
    let stamp = store::now_secs();
    let zip_path = downloads.join(format!(
        "GlassMC-share-{}-{}.zip",
        safe_component(&id),
        stamp
    ));

    // 暂存目录放在自己的 feature-files 下，只清理这一处，不碰用户数据。
    let stage = store::files_dir("share-stage").join(format!("{}-{}", safe_component(&id), stamp));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).map_err(|e| format!("创建暂存目录失败：{e}"))?;

    let meta = json!({
        "app": "GlassMC",
        "version": id,
        "inherits": inherits,
        "loader": loader,
        "mods": mods,
        "exportedAt": stamp,
    });
    std::fs::write(
        stage.join("share.json"),
        serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入元数据失败：{e}"))?;
    std::fs::write(stage.join("mods.txt"), mods.join("\n"))
        .map_err(|e| format!("写入模组清单失败：{e}"))?;

    let version_json = paths::paths()
        .versions_dir
        .join(&version_id)
        .join(format!("{version_id}.json"));
    if version_json.is_file() {
        let _ = std::fs::copy(&version_json, stage.join(format!("{version_id}.json")));
    }
    // 只带启动器设置，不含账号凭据。
    let cfg_file = paths::paths().config_file.clone();
    if cfg_file.is_file() {
        let _ = std::fs::copy(&cfg_file, stage.join("tauri-config.json"));
    }

    let files = store::zip_dir(&stage, &zip_path)?;
    let _ = std::fs::remove_dir_all(&stage);

    Ok(ShareExport {
        path: zip_path.to_string_lossy().to_string(),
        files,
        version: id,
        loader,
        mods: mods.len(),
    })
}

/// 生成一段可直接复制的中文介绍。
#[tauri::command]
pub fn share_text(version_id: String) -> Result<String, String> {
    if version_id.trim().is_empty() || version_id.contains('/') || version_id.contains("..") {
        return Err("版本 id 不合法".into());
    }
    let (id, _, loader) = version_meta(&version_id);
    let mods = mod_names(&version_id).len();
    Ok(format!(
        "我在玩 Minecraft {version}（加载器：{loader}），整合了 {mods} 个模组，使用 GlassMC 启动器。",
        version = id,
        loader = brief_loader(&loader),
        mods = mods,
    ))
}

// ---------------------------------------------------------------- 2. MCBBS 模板

const DEFAULT_TEMPLATE: &str = "[b]整合包分享[/b]\n游戏版本：{version}\n加载器：{loader}\n模组数量：{mods}\n介绍与下载：{link}\n—— 由 GlassMC 启动器生成";

#[tauri::command]
pub fn mcbbs_template_get() -> String {
    let t: String = store::load("mcbbs-template");
    if t.trim().is_empty() {
        DEFAULT_TEMPLATE.to_string()
    } else {
        t
    }
}

#[tauri::command]
pub fn mcbbs_template_set(template: String) -> Result<(), String> {
    store::save("mcbbs-template", &template)
}

/// 只生成 BBCode 文本；发布必须用户自己到论坛完成。
#[tauri::command]
pub fn mcbbs_render(version_id: String, link: String) -> Result<String, String> {
    if version_id.trim().is_empty() || version_id.contains('/') || version_id.contains("..") {
        return Err("版本 id 不合法".into());
    }
    let (id, _, loader) = version_meta(&version_id);
    let mods = mod_names(&version_id).len();
    let rendered = mcbbs_template_get()
        .replace("{version}", &id)
        .replace("{loader}", brief_loader(&loader))
        .replace("{mods}", &mods.to_string())
        .replace("{link}", &link);
    Ok(rendered)
}

// ---------------------------------------------------------------- 3. 服务器面板

fn pick<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    for key in keys {
        if let Some(found) = v.get(*key) {
            return Some(found);
        }
    }
    None
}

fn as_i64(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_u64().map(|x| x as i64))
            .unwrap_or(0),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or_else(|_| {
            s.split('/')
                .next()
                .and_then(|x| x.trim().parse::<i64>().ok())
                .unwrap_or(0)
        }),
        _ => 0,
    }
}

fn as_bool_or(v: Option<&Value>, fallback: bool) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => matches!(s.to_lowercase().as_str(), "1" | "true" | "yes"),
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        _ => fallback,
    }
}

fn as_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// 读取 `store("servers")`，兼容「裸数组」和 `{ servers: [...] }` 两种形状。
#[tauri::command]
pub fn server_dashboard() -> ServerDashboard {
    let root: Value = store::load("servers");
    let rows: Vec<Value> = if let Some(arr) = root.as_array() {
        arr.clone()
    } else {
        ["servers", "items", "list"]
            .iter()
            .find_map(|key| root.get(*key).and_then(|v| v.as_array()).cloned())
            .unwrap_or_default()
    };

    let mut items = Vec::new();
    for row in rows {
        if !row.is_object() {
            continue;
        }
        let name = as_text(pick(&row, &["name", "title", "alias"]));
        let address = as_text(pick(&row, &["address", "addr", "host", "server"]));
        if name.is_empty() && address.is_empty() {
            continue;
        }
        let ping_field = pick(&row, &["lastPingMs", "ping", "pingMs", "latency"]);
        let last_ping_ms = as_i64(ping_field);
        let players_online = as_i64(pick(
            &row,
            &[
                "playersOnline",
                "onlinePlayers",
                "players",
                "currentPlayers",
            ],
        ));
        let motd = as_text(pick(&row, &["motd", "description"]));
        let pinned = as_bool_or(pick(&row, &["pinned", "isPinned", "favorite"]), false);
        let online = as_bool_or(
            pick(&row, &["online", "isOnline"]),
            ping_field.is_some() && last_ping_ms > 0,
        );
        items.push(ServerItem {
            name,
            address,
            last_ping_ms,
            players_online,
            motd,
            pinned,
            online,
        });
    }

    let total = items.len();
    let online = items.iter().filter(|i| i.online).count();
    let pinned = items.iter().filter(|i| i.pinned).count();
    ServerDashboard {
        total,
        online,
        pinned,
        items,
    }
}

// ---------------------------------------------------------------- 4. 便携版

fn copy_entry(src: &Path, dst: &Path) -> Result<usize, String> {
    if !src.exists() {
        return Ok(0);
    }
    if src.is_dir() {
        store::copy_dir(src, dst)
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(src, dst).map_err(|e| e.to_string())?;
        Ok(1)
    }
}

/// 在用户选定目录下复制出一份便携数据；不删除任何源文件。
#[tauri::command]
pub fn portable_export(target_dir: String) -> Result<PortableResult, String> {
    if target_dir.trim().is_empty() {
        return Err("请先选择目标目录".into());
    }
    if target_dir.contains("..") {
        return Err("目标目录不合法".into());
    }
    let root = PathBuf::from(&target_dir).join("GlassMC-portable");
    std::fs::create_dir_all(&root).map_err(|e| format!("创建目标目录失败：{e}"))?;
    let p = paths::paths();

    let entries: [(&Path, &str); 6] = [
        (&p.versions_dir, "minecraft/versions"),
        (&p.libraries_dir, "minecraft/libraries"),
        (&p.assets_dir, "minecraft/assets"),
        (&p.java_dir, "java"),
        (&p.accounts_file, "accounts.json"),
        (&p.config_file, "tauri-config.json"),
    ];
    let mut files = 0usize;
    let mut copied = Vec::new();
    for (src, rel) in entries {
        let n = copy_entry(src, &root.join(rel))?;
        if n > 0 {
            files += n;
            copied.push(rel.to_string());
        }
    }

    let marker = json!({
        "app": "GlassMC",
        "portable": true,
        "createdAt": store::now_secs(),
    });
    std::fs::write(
        root.join("portable.json"),
        serde_json::to_string_pretty(&marker).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入标记失败：{e}"))?;
    files += 1;

    let readme = "GlassMC 便携版\n\
        ==============\n\n\
        本目录由现有数据目录复制而来，源文件未被删除。\n\n\
        使用方法：\n\
        1. Windows PowerShell（临时）：$env:GLASSMC_DATA=\"本目录的绝对路径\"\n\
        2. macOS / Linux 终端（临时）：export GLASSMC_DATA=\"本目录的绝对路径\"\n\
        3. 设置该环境变量后启动 GlassMC，即把本目录当作数据目录。\n\
        4. 想恢复默认，关闭启动器并取消该环境变量即可。\n\n\
        注意：accounts.json 含账号信息，请妥善保管整个目录。\n";
    std::fs::write(root.join("README.txt"), readme)
        .map_err(|e| format!("写入 README 失败：{e}"))?;
    files += 1;

    Ok(PortableResult {
        path: root.to_string_lossy().to_string(),
        files,
        copied,
    })
}

// ---------------------------------------------------------------- 5. 硬件面板

/// 跑一条外部命令，最多等 3 秒，超时就杀掉子进程并当作失败。
async fn run_cmd(program: &str, args: &[&str]) -> Option<String> {
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = match child.stdout.take() {
        Some(v) => v,
        None => {
            let _ = child.kill().await;
            return None;
        }
    };
    use tokio::io::AsyncReadExt;
    let read = async {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await.ok()?;
        Some(buf)
    };
    let result = tokio::time::timeout(Duration::from_secs(3), read).await;
    let _ = child.kill().await;
    let buf = result.ok()??;
    String::from_utf8(buf)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

async fn cpu_brand() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        run_cmd("sysctl", &["-n", "machdep.cpu.brand_string"]).await
    }
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        for line in s.lines() {
            if let Some(rest) = line.split_once(':') {
                if rest.0.trim() == "model name" && !rest.1.trim().is_empty() {
                    return Some(rest.1.trim().to_string());
                }
            }
        }
        s.lines()
            .next()
            .map(|l| l.trim().to_string())
            .filter(|s| !s.is_empty())
    }
    #[cfg(target_os = "windows")]
    {
        let s = run_cmd("wmic", &["cpu", "get", "name"]).await?;
        s.lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && l.to_lowercase() != "name")
            .map(str::to_string)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

async fn total_ram_gb() -> Option<f64> {
    #[cfg(target_os = "macos")]
    {
        let s = run_cmd("sysctl", &["-n", "hw.memsize"]).await?;
        let bytes: f64 = s.parse().ok()?;
        Some(round1(bytes / (1024.0 * 1024.0 * 1024.0)))
    }
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = s.lines().find(|l| l.starts_with("MemTotal:"))?;
        let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(round1(kb / (1024.0 * 1024.0)))
    }
    #[cfg(target_os = "windows")]
    {
        let s = run_cmd("wmic", &["computersystem", "get", "TotalPhysicalMemory"]).await?;
        let bytes: f64 = s
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()))?
            .parse()
            .ok()?;
        Some(round1(bytes / (1024.0 * 1024.0 * 1024.0)))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

async fn gpu_name() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let s = run_cmd("system_profiler", &["SPDisplaysDataType"]).await?;
        for line in s.lines() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("Chipset Model:") {
                return Some(rest.trim().to_string()).filter(|v| !v.is_empty());
            }
        }
        None
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let s = run_cmd("nvidia-smi", &["--query-gpu=name", "--format=csv,noheader"]).await?;
        s.lines().next().map(|l| l.trim().to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[tauri::command]
pub async fn hardware_info() -> HardwareInfo {
    let cpu = cpu_brand().await.unwrap_or_else(|| "未知".to_string());
    let gpu = gpu_name().await.unwrap_or_else(|| "未知".to_string());
    let total_ram_gb = total_ram_gb().await.unwrap_or(0.0);
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    HardwareInfo {
        cpu,
        cores,
        total_ram_gb,
        gpu,
    }
}

#[cfg(target_os = "linux")]
fn stat_cpu_sample() -> Option<(f64, f64)> {
    let s = std::fs::read_to_string("/proc/stat").ok()?;
    let line = s.lines().find(|l| l.starts_with("cpu "))?;
    let nums: Vec<f64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|x| x.parse().ok())
        .collect();
    if nums.len() < 4 {
        return None;
    }
    let idle = nums[3] + nums.get(4).copied().unwrap_or(0.0);
    let total: f64 = nums.iter().sum();
    Some((total, idle))
}

async fn cpu_usage() -> Option<f64> {
    #[cfg(target_os = "macos")]
    {
        let s = run_cmd("top", &["-l", "1", "-n", "0"]).await?;
        let line = s.lines().find(|l| l.contains("CPU usage:"))?;
        let part = line.split("idle").next()?;
        let idle = part
            .split_whitespace()
            .filter_map(|w| w.trim_end_matches('%').parse::<f64>().ok())
            .next_back()?;
        Some((100.0 - idle).clamp(0.0, 100.0))
    }
    #[cfg(target_os = "linux")]
    {
        let a = stat_cpu_sample()?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let b = stat_cpu_sample()?;
        let dt = b.0 - a.0;
        if dt <= 0.0 {
            return None;
        }
        let busy = dt - (b.1 - a.1);
        Some((busy / dt * 100.0).clamp(0.0, 100.0))
    }
    #[cfg(target_os = "windows")]
    {
        let s = run_cmd("wmic", &["cpu", "get", "loadpercentage"]).await?;
        let pct: f64 = s
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()))?
            .parse()
            .ok()?;
        Some(pct.clamp(0.0, 100.0))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn vm_pages(s: &str, key: &str) -> Option<f64> {
    s.lines()
        .find(|l| l.starts_with(key))?
        .split(':')
        .nth(1)?
        .trim()
        .trim_end_matches('.')
        .replace(',', "")
        .parse()
        .ok()
}

#[cfg(target_os = "linux")]
fn mem_kb(s: &str, key: &str) -> Option<f64> {
    s.lines()
        .find(|l| l.starts_with(key))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

async fn mem_usage() -> Option<f64> {
    #[cfg(target_os = "macos")]
    {
        let s = run_cmd("vm_stat", &[]).await?;
        let free = vm_pages(&s, "Pages free:").unwrap_or(0.0);
        let inactive = vm_pages(&s, "Pages inactive:").unwrap_or(0.0);
        let speculative = vm_pages(&s, "Pages speculative:").unwrap_or(0.0);
        let wired = vm_pages(&s, "Pages wired down:").unwrap_or(0.0);
        let active = vm_pages(&s, "Pages active:").unwrap_or(0.0);
        let total = free + inactive + speculative + wired + active;
        if total <= 0.0 {
            return None;
        }
        Some((((active + wired) / total) * 100.0).clamp(0.0, 100.0))
    }
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/meminfo").ok()?;
        let total = mem_kb(&s, "MemTotal:")?;
        if total <= 0.0 {
            return None;
        }
        let available = mem_kb(&s, "MemAvailable:").unwrap_or(total * 0.2);
        Some((((total - available) / total) * 100.0).clamp(0.0, 100.0))
    }
    #[cfg(target_os = "windows")]
    {
        let s = run_cmd(
            "wmic",
            &["os", "get", "FreePhysicalMemory,TotalVisibleMemorySize"],
        )
        .await?;
        let nums: Vec<f64> = s
            .lines()
            .flat_map(|l| l.split_whitespace())
            .filter_map(|w| w.parse().ok())
            .collect();
        if nums.len() < 2 || nums[1] <= 0.0 {
            return None;
        }
        Some((((nums[1] - nums[0]) / nums[1]) * 100.0).clamp(0.0, 100.0))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// 只做粗略估算；两边都失败时返回 `null`，前端显示「未知」。
#[tauri::command]
pub async fn hardware_usage() -> Option<HardwareUsage> {
    let cpu = cpu_usage().await;
    let mem = mem_usage().await;
    if cpu.is_none() && mem.is_none() {
        return None;
    }
    Some(HardwareUsage {
        cpu_percent: round1(cpu.unwrap_or(0.0)),
        mem_percent: round1(mem.unwrap_or(0.0)),
    })
}

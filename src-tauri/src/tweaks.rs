//! tweaks.rs —— 三大类增强的「桌面等价实现」。
//!
//! - 移动端等价：触控外观、内存推荐、外置目录、虚拟按键、512MB 模式；
//! - 游戏内辅助：离线生成 datapack / 资源包 / 说明包，绝不自动安装；
//! - 启动优化：本地扫描基准、可关闭进程名单、智能 GC、启动前脚本、内存压缩。
//!
//! 重要：这里是桌面启动器里能做到的等价物。Java 版没有真正的触控注入、
//! 没有 ModPE、没有内置小地图，全部在返回说明里如实写清，不伪造能力。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config;
use crate::paths;
use crate::store;

const PREFS_KEY: &str = "tweak-prefs";
const PACKS_DIR: &str = "assist-packs";

// ===================================================================== 偏好

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct TweakPrefs {
    /// 触控外观（大点击区、隐藏 hover 专属效果）
    pub touch_mode: bool,
    /// 悬浮虚拟操作条
    pub virtual_keys: bool,
    /// 512MB 低内存模式（launch.rs 读取后覆盖 -Xmx 并追加参数）
    pub low_memory: bool,
    /// 智能 GC（launch.rs 读取后追加参数）
    pub smart_gc: bool,
    /// 启动前脚本（用户点「试运行」或 launch.rs 集成后执行）
    pub prelaunch_script: String,
}

#[allow(clippy::derivable_impls)] // 显式默认值便于以后加字段时逐个确认
impl Default for TweakPrefs {
    fn default() -> Self {
        Self {
            touch_mode: false,
            virtual_keys: false,
            low_memory: false,
            smart_gc: false,
            prelaunch_script: String::new(),
        }
    }
}

fn prefs() -> TweakPrefs {
    store::load(PREFS_KEY)
}

fn save_prefs(p: &TweakPrefs) -> Result<(), String> {
    store::save(PREFS_KEY, p)
}

#[tauri::command]
pub fn tweak_prefs_get() -> TweakPrefs {
    prefs()
}

/// 浅合并补丁。前端传 camelCase 键（touchMode / virtualKeys / ...）。
#[tauri::command]
pub fn tweak_prefs_set(patch: serde_json::Value) -> Result<TweakPrefs, String> {
    let mut p = prefs();
    let mut value = serde_json::to_value(&p).map_err(|e| e.to_string())?;
    if let (Some(cur), Some(patch)) = (value.as_object_mut(), patch.as_object()) {
        for (k, v) in patch {
            cur.insert(k.clone(), v.clone());
        }
    }
    p = serde_json::from_value(value).map_err(|e| e.to_string())?;
    save_prefs(&p)?;
    Ok(p)
}

// ===================================================================== 功能 A：移动端等价

/// 512MB 模式。launch.rs 读取 tweak-prefs.lowMemory 后覆盖 -Xmx 并追加这些参数。
#[tauri::command]
pub fn low_memory_set(enabled: bool) -> Result<TweakPrefs, String> {
    let mut p = prefs();
    p.low_memory = enabled;
    save_prefs(&p)?;
    Ok(p)
}

#[tauri::command]
pub fn low_memory_args() -> Vec<String> {
    vec![
        "-Xmx512M".to_string(),
        "-XX:+UseSerialGC".to_string(),
        "-XX:MaxMetaspaceSize=256m".to_string(),
    ]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemRecommend {
    pub recommended_gb: usize,
    pub reason: String,
    pub mod_count: usize,
    pub system_gb: usize,
}

/// 从实例 id 里抠出 `1.20.1` 这样的版本号（兼容中文前缀与 -Forge 后缀）。
fn game_version_of(id: &str) -> String {
    let b = id.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'1' && i + 1 < b.len() && b[i + 1] == b'.' {
            let mut j = i + 2;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                j += 1;
            }
            return id[i..j].trim_end_matches('.').to_string();
        }
    }
    String::new()
}

fn parse_version(v: &str) -> (i32, i32, i32) {
    let mut it = v.split('.');
    let major = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let minor = it.next().and_then(|s| s.parse().ok()).unwrap_or(21);
    let patch = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

fn pack_format(v: &str) -> i32 {
    let (major, minor, patch) = parse_version(v);
    if major < 13 {
        return 4;
    }
    match (major, minor) {
        (1, 13) | (1, 14) => 4,
        (1, 15) => 5,
        (1, 16) => {
            if patch >= 2 {
                6
            } else {
                5
            }
        }
        (1, 17) => 7,
        (1, 18) => 8,
        (1, 19) => {
            if patch >= 4 {
                10
            } else {
                9
            }
        }
        (1, 20) => {
            if patch >= 5 {
                41
            } else if patch >= 4 {
                26
            } else if patch >= 2 {
                18
            } else {
                15
            }
        }
        (1, 21) => {
            if patch >= 4 {
                61
            } else if patch >= 2 {
                57
            } else {
                48
            }
        }
        _ => 48,
    }
}

/// 本机物理内存（GB）。失败返回 None，调用方回退 8GB。
/// 用 cfg! 运行时分支而不是 cfg 块，保证任意平台上函数尾表达式都是 None。
fn physical_memory_gb() -> Option<usize> {
    if cfg!(target_os = "macos") {
        let out = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output()
            .ok()?;
        let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        return Some((bytes / (1024 * 1024 * 1024)) as usize);
    }
    if cfg!(target_os = "linux") {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        for line in text.lines() {
            if line.starts_with("MemTotal:") {
                let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
                return Some((kb / 1024 / 1024) as usize);
            }
        }
        return None;
    }
    if cfg!(target_os = "windows") {
        let out = std::process::Command::new("wmic")
            .args(["computersystem", "get", "TotalPhysicalMemory"])
            .output()
            .ok()?;
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Ok(bytes) = line.trim().parse::<u64>() {
                return Some((bytes / (1024 * 1024 * 1024)) as usize);
            }
        }
        return None;
    }
    None
}

#[tauri::command]
pub fn memory_recommend(version_id: String) -> Result<MemRecommend, String> {
    let versions = crate::commands::scan_versions();
    let found = versions.iter().find(|v| v.id == version_id);
    let mod_count = found.map(|v| v.mods).unwrap_or(0);
    let loader = found.and_then(|v| v.loader.clone()).unwrap_or_default();
    let vstr = game_version_of(&version_id);
    let (major, minor, _) = parse_version(&vstr);
    let system_gb = physical_memory_gb().unwrap_or(8);

    let mut rec = 4usize;
    if mod_count > 10 {
        rec = 6;
    }
    if mod_count > 30 {
        rec = 8;
    }
    if mod_count > 80 {
        rec = 10;
    }
    if loader == "forge" || loader == "neoforge" {
        rec += 1;
    }
    if major == 1 && minor <= 12 {
        rec = rec.saturating_sub(1);
    }
    let cap = system_gb.saturating_sub(2).max(2);
    if rec > cap {
        rec = cap;
    }
    if rec < 2 {
        rec = 2;
    }

    let reason = format!(
        "版本 {}（{}），检测到 {} 个模组；系统物理内存 {} GB。建议 -Xmx{}G，给系统和浏览器预留至少 2 GB。{}{}",
        if vstr.is_empty() {
            version_id.as_str()
        } else {
            vstr.as_str()
        },
        if loader.is_empty() { "原版" } else { loader.as_str() },
        mod_count,
        system_gb,
        rec,
        if major == 1 && minor <= 12 {
            "老版本客户端占用较低，已相应下调。"
        } else {
            ""
        },
        if system_gb <= 4 {
            "本机内存较小，请关闭后台程序后再启动。"
        } else {
            ""
        },
    );

    Ok(MemRecommend {
        recommended_gb: rec,
        reason,
        mod_count,
        system_gb,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirTarget {
    pub path: String,
    pub label: String,
    pub kind: String,
}

/// 候选外置卷：macOS /Volumes、Linux /mnt 与 /media、Windows 盘符。
#[tauri::command]
pub fn data_dir_targets() -> Vec<DirTarget> {
    let mut out = Vec::new();

    #[cfg(target_os = "macos")]
    {
        if let Ok(rd) = std::fs::read_dir("/Volumes") {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    let label = e.file_name().to_string_lossy().to_string();
                    out.push(DirTarget {
                        path: e.path().to_string_lossy().to_string(),
                        label,
                        kind: "外置卷".to_string(),
                    });
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for root in ["/mnt", "/media"] {
            if let Ok(rd) = std::fs::read_dir(root) {
                for e in rd.flatten() {
                    if e.path().is_dir() {
                        let label = e.file_name().to_string_lossy().to_string();
                        out.push(DirTarget {
                            path: e.path().to_string_lossy().to_string(),
                            label,
                            kind: "外置目录".to_string(),
                        });
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        for c in 'A'..='Z' {
            let p = PathBuf::from(format!("{}:\\", c));
            if p.is_dir() {
                out.push(DirTarget {
                    path: p.to_string_lossy().to_string(),
                    label: format!("{} 盘", c),
                    kind: "磁盘分区".to_string(),
                });
            }
        }
    }

    out
}

#[derive(Serialize)]
pub struct PortableMarker {
    pub created_at: i64,
    pub source: String,
    pub note: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataMoveResult {
    pub path: String,
    pub files: usize,
    pub note: String,
}

/// 复制数据目录到外置卷，不删源；用户需重启并手动设 GLASSMC_DATA。
#[tauri::command]
pub fn data_dir_move(target: String) -> Result<DataMoveResult, String> {
    let src = paths::paths().data_root.clone();
    let raw = PathBuf::from(&target);
    if !raw.is_dir() {
        return Err("目标不存在或不是文件夹".to_string());
    }
    let dst_root = std::fs::canonicalize(&raw).map_err(|e| e.to_string())?;
    let src_can = std::fs::canonicalize(&src).unwrap_or_else(|_| src.clone());
    if dst_root.starts_with(&src_can) {
        return Err("目标目录不能放在 GlassMC 数据目录内部".to_string());
    }

    let probe = dst_root.join(format!(".glassmc-write-{}", store::short_id()));
    std::fs::write(&probe, b"ok").map_err(|e| format!("目标目录不可写：{}", e))?;
    let _ = std::fs::remove_file(&probe);

    let dest = dst_root.join("GlassMC");
    let files = store::copy_dir(&src, &dest)?;
    let marker = PortableMarker {
        created_at: store::now_secs(),
        source: src.to_string_lossy().to_string(),
        note: "这是 GlassMC 便携副本。源目录未被删除；请设置环境变量 GLASSMC_DATA 指向本目录后重启启动器。".to_string(),
    };
    let marker_text = serde_json::to_string_pretty(&marker).map_err(|e| e.to_string())?;
    std::fs::write(dest.join("portable.json"), marker_text).map_err(|e| e.to_string())?;

    let path = dest.to_string_lossy().to_string();
    Ok(DataMoveResult {
        note: format!(
            "已复制 {} 个文件到 {}。源目录保留不变；请重启后手动设置 GLASSMC_DATA={} 完成切换。",
            files, path, path
        ),
        path,
        files,
    })
}

// ===================================================================== 功能 B：离线辅助包

fn write_rel(root: &Path, rel: &str, content: &str) -> Result<(), String> {
    if rel.split('/').any(|p| p == ".." || p.is_empty()) {
        return Err("非法的包内路径".to_string());
    }
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, content).map_err(|e| e.to_string())
}

fn pack_meta(format: i32, desc: &str) -> String {
    format!(
        "{{\"pack\":{{\"pack_format\":{},\"description\":{}}}}}\n",
        format,
        serde_json::to_string(desc).unwrap_or_else(|_| "\"GlassMC\"".to_string())
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistPackResult {
    pub kind: String,
    pub path: String,
    pub note: String,
    pub file_count: usize,
}

#[tauri::command]
pub fn assist_pack_create(kind: String, version_id: String) -> Result<AssistPackResult, String> {
    let vstr = game_version_of(&version_id);
    let fmt = pack_format(&vstr);
    let base = store::files_dir(PACKS_DIR);
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let staging = base.join(format!(".build-{}", kind));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    // 每个分支先写文件再给 note，保留 late init 可读性
    #[allow(clippy::needless_late_init)]
    let note: String;
    match kind.as_str() {
        "keep_inventory" => {
            write_rel(
                &staging,
                "pack.mcmeta",
                &pack_meta(fmt, "GlassMC 离线辅助包：保持物品栏"),
            )?;
            write_rel(
                &staging,
                "data/minecraft/tags/functions/load.json",
                "{\"values\":[\"gm:tick\"]}\n",
            )?;
            write_rel(
                &staging,
                "data/gm/function/tick.mcfunction",
                "# GlassMC 离线生成；不会自动安装\ngamerule keepInventory true\n",
            )?;
            write_rel(
                &staging,
                "README.txt",
                "保持物品栏（keepInventory）数据包\n\
                 --------------------------------\n\
                 解压后把整个文件夹放进世界的 datapacks 目录，或在服务器控制台执行 /reload。\n\
                 需要管理员权限或开启作弊；本启动器只生成文件，不会自动放入游戏。\n",
            )?;
            note = format!("已生成 keepInventory 数据包（pack_format {}）。需手动放入世界 datapacks 目录，且需要作弊/管理员权限。", fmt);
        }
        "minimap" => {
            write_rel(
                &staging,
                "README.txt",
                "小地图说明包（Java 版）\n\
                 ----------------------\n\
                 Java 版没有内置小地图，也无法通过离线文件注入真正的小地图界面。\n\
                 本包只是说明 + 推荐清单，不会修改游戏。\n\n\
                 推荐模组（请自行从 Modrinth / CurseForge 下载并安装）：\n\
                 - Xaero's Minimap（轻量、版本更新快）\n\
                 - JourneyMap（功能全，可网页端查看）\n\n\
                 安装后进入游戏按模组默认快捷键（通常为 M）打开地图设置。\n",
            )?;
            note =
                "已生成小地图说明包。Java 版无内置小地图，包内只含推荐模组清单，不含真正地图功能。"
                    .to_string();
        }
        "xray" => {
            write_rel(
                &staging,
                "pack.mcmeta",
                &pack_meta(fmt, "GlassMC 离线辅助包：矿石高亮（仅客户端资源包）"),
            )?;
            write_rel(
                &staging,
                "assets/minecraft/models/block/gm_highlight.json",
                "{\"parent\":\"block/cube_all\",\"textures\":{\"all\":\"minecraft:block/gold_block\"}}\n",
            )?;
            for ore in ["diamond_ore", "iron_ore", "gold_ore", "coal_ore"] {
                write_rel(
                    &staging,
                    &format!("assets/minecraft/blockstates/{}.json", ore),
                    "{\"variants\":{\"\":{\"model\":\"minecraft:block/gm_highlight\"}}}\n",
                )?;
            }
            write_rel(
                &staging,
                "README.txt",
                "矿石高亮资源包（仅客户端，不改游戏规则）\n\
                 ------------------------------------------\n\
                 本资源包把常见矿石的方块模型换成高亮的金色纹理，便于辨认。\n\
                 它不会修改游戏规则、不会透视墙体，也不是作弊模组；是否使用请自行判断。\n\
                 使用方法：放入资源包目录后在游戏「选项→资源包」里启用。\n",
            )?;
            note = "已生成矿石高亮资源包。仅客户端纹理替换，不改游戏规则、不保证真正透视。"
                .to_string();
        }
        "geometry" => {
            write_rel(
                &staging,
                "README.txt",
                "几何建造助手（说明 + 常用指令）\n\
                 ------------------------------\n\
                 .litematic 是 Litematica 模组的专有投影格式，无法凭空离线生成有效投影。\n\
                 附带的 example.litematic.txt 仅说明如何在游戏里用 Litematica 保存/加载投影。\n\n\
                 常用指令：\n\
                 /fill x1 y1 z1 x2 y2 z2 minecraft:stone\n\
                 /fill ... air            （清空区域）\n\
                 /clone x1 y1 z1 x2 y2 z2 x3 y3 z3\n\
                 /setblock x y z minecraft:glass\n\
                 /execute as @p at @s run fill ...\n",
            )?;
            write_rel(
                &staging,
                "example.litematic.txt",
                "示例投影说明（非真正的 .litematic 文件）\n\
                 ------------------------------------\n\
                 1. 安装 Litematica 模组；\n\
                 2. 游戏内按 M 打开主菜单 → 投影 → 保存区域；\n\
                 3. 选中两点框出建筑后保存，文件位于 minecraft/config/litematic/；\n\
                 4. 加载投影后开启「显示投影」即可照着建造。\n",
            )?;
            note = "已生成几何建造说明包。真正的 .litematic 投影需安装 Litematica 后在游戏中生成。"
                .to_string();
        }
        "entity_health" => {
            write_rel(
                &staging,
                "pack.mcmeta",
                &pack_meta(fmt, "GlassMC 离线辅助包：实体血条显示建议"),
            )?;
            write_rel(
                &staging,
                "README.txt",
                "实体血条显示增强（说明包）\n\
                 --------------------------\n\
                 Java 版已内置生物血条（受伤后短暂显示），无需也无法通过本包新增血条系统。\n\
                 显示增强建议：\n\
                 - 安装资源包调整 gui/icons.png 里的血条样式；\n\
                 - 使用 Jade / WTHIT 等模组在准星旁显示实体信息；\n\
                 - 服务器可用 /attribute @e minecraft:generic.max_health 调整上限。\n\
                 本包不含任何可执行内容，只提供说明。\n",
            )?;
            note = "已生成血条显示说明包。Java 版已内置血条，包内只含显示增强建议。".to_string();
        }
        "modpe" => {
            write_rel(
                &staging,
                "pack.mcmeta",
                &pack_meta(fmt, "GlassMC 离线辅助包：ModPE 等价命令包"),
            )?;
            write_rel(
                &staging,
                "data/minecraft/tags/functions/load.json",
                "{\"values\":[\"gm:demo\"]}\n",
            )?;
            write_rel(
                &staging,
                "data/gm/function/demo.mcfunction",
                "# ModPE 属于基岩版；以下是 Java 版等价命令示例\n\
                 give @p minecraft:diamond 1\n\
                 effect give @p minecraft:speed 30 1\n\
                 tp @p ~ 10 ~\n",
            )?;
            write_rel(
                &staging,
                "README.txt",
                "ModPE 等价说明（Java 版）\n\
                 ------------------------\n\
                 ModPE 是基岩版（Bedrock）的脚本 API，Java 版不支持，无法离线转换。\n\
                 本包提供等价的数据包命令示例，可用于演示「给物品 / 加效果 / 传送」。\n\
                 放入世界 datapacks 目录后执行 /reload；需要作弊或管理员权限。\n",
            )?;
            note =
                "已生成 ModPE 等价命令包。ModPE 属基岩版，Java 版以数据包命令代替，不是脚本注入。"
                    .to_string();
        }
        _ => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("未知的辅助包类型：{}", kind));
        }
    }

    let zip_path = base.join(format!("{}.zip", kind));
    let file_count = store::zip_dir(&staging, &zip_path)?;
    let _ = std::fs::remove_dir_all(&staging);
    Ok(AssistPackResult {
        kind,
        path: zip_path.to_string_lossy().to_string(),
        note,
        file_count,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistPackInfo {
    pub kind: String,
    pub path: String,
    pub size: u64,
    pub created_at: i64,
}

#[tauri::command]
pub fn assist_packs_list() -> Vec<AssistPackInfo> {
    let base = store::files_dir(PACKS_DIR);
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&base) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() || p.extension().and_then(|x| x.to_str()) != Some("zip") {
                continue;
            }
            let kind = p
                .file_stem()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_string();
            let meta = match std::fs::metadata(&p) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let created_at = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or_else(store::now_secs);
            out.push(AssistPackInfo {
                kind,
                path: p.to_string_lossy().to_string(),
                size: meta.len(),
                created_at,
            });
        }
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind));
    out
}

#[tauri::command]
pub fn assist_pack_open(kind: String) -> Result<(), String> {
    const KNOWN: &[&str] = &[
        "keep_inventory",
        "minimap",
        "xray",
        "geometry",
        "entity_health",
        "modpe",
    ];
    if !KNOWN.contains(&kind.as_str()) {
        return Err("未知的辅助包类型".to_string());
    }
    let base = store::files_dir(PACKS_DIR);
    if !base.join(format!("{}.zip", kind)).is_file() {
        return Err("该辅助包尚未生成".to_string());
    }
    tauri_plugin_opener::open_path(&base, None::<&str>).map_err(|e| e.to_string())
}

// ===================================================================== 功能 C：启动优化

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FastCoreResult {
    pub scan_ms: u128,
    pub manifest_ms: u128,
    pub version_count: usize,
    pub note: String,
}

/// 诚实基准：本地 Rust 扫描 + 本地清单 JSON 读取，不声称 C++ 2ms。
#[tauri::command]
pub fn fast_core_bench() -> FastCoreResult {
    let t0 = Instant::now();
    let versions = crate::commands::scan_versions();
    let scan_ms = t0.elapsed().as_millis();

    let t1 = Instant::now();
    let mut bytes = 0usize;
    if let Ok(rd) = std::fs::read_dir(&paths::paths().versions_dir) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let id = e.file_name().to_string_lossy().to_string();
            if let Ok(buf) = std::fs::read(e.path().join(format!("{}.json", id))) {
                bytes += buf.len();
            }
        }
    }
    let manifest_ms = t1.elapsed().as_millis();

    FastCoreResult {
        scan_ms,
        manifest_ms,
        version_count: versions.len(),
        note: format!(
            "本地 Rust 快速扫描：扫描 {} 个实例、读取 {} 字节本地清单 JSON。这是实测耗时，不代表 C++ 内核，也不承诺 2ms；结果随机器与磁盘状态变化。",
            versions.len(),
            bytes
        ),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessCandidate {
    pub name: String,
    pub label: String,
    pub reason: String,
    pub selected: bool,
}

/// 仅列名单，默认不勾选；是否关闭完全由用户在界面上决定。
#[tauri::command]
pub fn cleanup_processes_scan() -> Vec<ProcessCandidate> {
    let rows = [
        ("chrome", "Google Chrome", "网页浏览器，占用内存较多"),
        ("msedge", "Microsoft Edge", "网页浏览器"),
        ("firefox", "Firefox", "网页浏览器"),
        ("Safari", "Safari", "网页浏览器"),
        ("obs", "OBS Studio", "录屏 / 直播软件"),
        ("Discord", "Discord", "聊天与语音"),
        ("Steam", "Steam", "游戏平台后台"),
        ("Spotify", "Spotify", "音乐客户端"),
        ("code", "VS Code", "代码编辑器"),
        ("Xcode", "Xcode", "IDE"),
    ];
    rows.iter()
        .map(|(name, label, reason)| ProcessCandidate {
            name: name.to_string(),
            label: label.to_string(),
            reason: reason.to_string(),
            selected: false,
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KillOutcome {
    pub name: String,
    pub ok: bool,
    pub error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessApplyResult {
    pub results: Vec<KillOutcome>,
}

fn valid_process_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '.' || c == '-' || c == '_')
}

fn run_with_timeout(program: &str, args: &[String], timeout: Duration) -> Result<bool, String> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("命令超时".to_string());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn kill_process(name: &str) -> Result<bool, String> {
    if cfg!(windows) {
        let exe = if name.to_ascii_lowercase().ends_with(".exe") {
            name.to_string()
        } else {
            format!("{}.exe", name)
        };
        run_with_timeout(
            "taskkill",
            &["/IM".to_string(), exe, "/F".to_string()],
            Duration::from_secs(5),
        )
    } else {
        run_with_timeout(
            "pkill",
            &["-f".to_string(), name.to_string()],
            Duration::from_secs(5),
        )
    }
}

#[tauri::command]
pub fn cleanup_processes_apply(names: Vec<String>) -> ProcessApplyResult {
    let results = names
        .into_iter()
        .map(|name| {
            if !valid_process_name(&name) {
                return KillOutcome {
                    name,
                    ok: false,
                    error: "进程名不合法（只允许字母、数字、空格、点、横线）".to_string(),
                };
            }
            match kill_process(&name) {
                Ok(ok) => KillOutcome {
                    name,
                    ok,
                    error: if ok {
                        String::new()
                    } else {
                        "未找到该进程或没有权限".to_string()
                    },
                },
                Err(e) => KillOutcome {
                    name,
                    ok: false,
                    error: e,
                },
            }
        })
        .collect();
    ProcessApplyResult { results }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartGcResult {
    pub enabled: bool,
    pub args: Vec<String>,
    pub note: String,
    pub mod_count: usize,
    pub system_gb: usize,
}

#[tauri::command]
pub fn smart_gc_set(enabled: bool) -> Result<TweakPrefs, String> {
    let mut p = prefs();
    p.smart_gc = enabled;
    save_prefs(&p)?;
    Ok(p)
}

#[tauri::command]
pub fn smart_gc_args(version_id: String) -> SmartGcResult {
    let p = prefs();
    let versions = crate::commands::scan_versions();
    let mod_count = versions
        .iter()
        .find(|v| v.id == version_id)
        .map(|v| v.mods)
        .unwrap_or(0);
    let system_gb = physical_memory_gb().unwrap_or(8);

    if !p.smart_gc {
        return SmartGcResult {
            enabled: false,
            args: Vec::new(),
            note: "智能 GC 未开启。开关存于 tweak-prefs.smartGc，由 launch.rs 读取。".to_string(),
            mod_count,
            system_gb,
        };
    }

    let args = if system_gb >= 8 || mod_count >= 20 {
        vec![
            "-XX:+UseG1GC".to_string(),
            "-XX:MaxGCPauseMillis=200".to_string(),
            "-XX:+ParallelRefProcEnabled".to_string(),
            "-XX:InitiatingHeapOccupancyPercent=35".to_string(),
        ]
    } else {
        vec![
            "-XX:+UseParallelGC".to_string(),
            "-XX:MaxGCPauseMillis=200".to_string(),
        ]
    };
    SmartGcResult {
        enabled: true,
        note: format!(
            "按 {} 个模组 / {} GB 物理内存选择 {}。仅在本机生成参数，实际生效需 launch.rs 追加。",
            mod_count,
            system_gb,
            if system_gb >= 8 || mod_count >= 20 {
                "G1GC"
            } else {
                "ParallelGC"
            }
        ),
        args,
        mod_count,
        system_gb,
    }
}

#[tauri::command]
pub fn prelaunch_script_get() -> String {
    prefs().prelaunch_script
}

#[tauri::command]
pub fn prelaunch_script_set(script: String) -> Result<TweakPrefs, String> {
    let mut p = prefs();
    p.prelaunch_script = script;
    save_prefs(&p)?;
    Ok(p)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptRunResult {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

/// 只在用户点「试运行」或 launch.rs 集成后调用；绝不自动执行。
#[tauri::command]
pub async fn prelaunch_script_run() -> Result<ScriptRunResult, String> {
    let script = prefs().prelaunch_script;
    if script.trim().is_empty() {
        return Err("脚本为空，请先保存一段脚本".to_string());
    }
    let mut cmd = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(&script);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(&script);
        c
    };
    cmd.kill_on_drop(true);

    let out = match tokio::time::timeout(Duration::from_secs(30), cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => {
            return Ok(ScriptRunResult {
                ok: false,
                stdout: String::new(),
                stderr: "执行超过 30 秒，已终止".to_string(),
                exit_code: None,
                timed_out: true,
            })
        }
    };
    Ok(ScriptRunResult {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        exit_code: out.status.code(),
        timed_out: false,
    })
}

/// 堆小于 28GB 时启用压缩指针，否则返回空（JVM 会自动关闭）。
#[tauri::command]
pub fn memory_compression_args() -> Vec<String> {
    if config::load().memory_gb < 28.0 {
        vec![
            "-XX:+UseCompressedOops".to_string(),
            "-XX:+UseCompressedClassPointers".to_string(),
        ]
    } else {
        Vec::new()
    }
}

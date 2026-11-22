//! 存档与数据：世界种子/游玩时长、备份恢复、截图画廊、单存档数据包、投影文件。
//!
//! 约定：
//! - 世界、截图、数据包都在「实例游戏目录」下（manifest::game_directory），
//!   备份落在 feature-files/save-backups，不碰启动配置；
//! - level.dat 是 gzip 压缩的 NBT，这里只解析需要的几个字段，
//!   解析失败仍列出世界并把种子标记为未知，不让整页崩掉；
//! - 所有来自界面的名字/路径都做白名单校验，拒绝 `..` 与路径分隔符。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{manifest, paths, store};

const BACKUP_INDEX: &str = "save-backups";
const MAX_THUMB: u64 = 3 * 1024 * 1024;
const MAX_SCHEMATICS: usize = 500;

// ---------------------------------------------------------------- 通用工具

/// 只接收单层名字：拒绝空、`.`、`..`、路径分隔符与 NUL。
fn safe_segment(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty()
        || t == "."
        || t == ".."
        || t.contains('/')
        || t.contains('\\')
        || t.contains('\0')
    {
        return None;
    }
    Some(t.to_string())
}

fn game_dir(version_id: &str) -> Result<PathBuf, String> {
    let v = safe_segment(version_id).ok_or_else(|| "实例 id 不合法".to_string())?;
    Ok(manifest::game_directory(&v))
}

fn world_dir(version_id: &str, save_id: &str) -> Result<PathBuf, String> {
    let w = safe_segment(save_id).ok_or_else(|| "世界名不合法".to_string())?;
    Ok(game_dir(version_id)?.join("saves").join(w))
}

fn dir_size(path: &Path) -> u64 {
    fn walk(p: &Path, acc: &mut u64) {
        let Ok(rd) = std::fs::read_dir(p) else {
            return;
        };
        for e in rd.flatten() {
            let Ok(meta) = e.metadata() else {
                continue;
            };
            if meta.is_dir() {
                walk(&e.path(), acc);
            } else if meta.is_file() {
                *acc = acc.saturating_add(meta.len());
            }
        }
    }
    let mut total = 0u64;
    walk(path, &mut total);
    total
}

fn modified_secs(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn any_file(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.flatten().any(|e| e.path().is_file()),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------- 最小 NBT 解析

/// 只保留我们关心的几种值，数组与列表只跳过负载，避免大文件占满内存。
#[derive(Clone, Debug)]
enum Nbt {
    Long(i64),
    Int(i32),
    Str(String),
    Compound(Vec<(String, Nbt)>),
    Other,
}

impl Nbt {
    fn get(&self, key: &str) -> Option<&Nbt> {
        match self {
            Nbt::Compound(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn num(&self) -> Option<i64> {
        match self {
            Nbt::Long(v) => Some(*v),
            Nbt::Int(v) => Some(*v as i64),
            _ => None,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Nbt::Str(s) => Some(s),
            _ => None,
        }
    }
}

struct Cursor<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Cursor<'a> {
    fn byte(&mut self) -> Option<u8> {
        let b = *self.d.get(self.p)?;
        self.p += 1;
        Some(b)
    }

    fn ushort(&mut self) -> Option<u16> {
        let a = *self.d.get(self.p)? as u16;
        let b = *self.d.get(self.p + 1)? as u16;
        self.p += 2;
        Some((a << 8) | b)
    }

    fn int(&mut self) -> Option<i32> {
        let mut v: u32 = 0;
        for i in 0..4 {
            v = (v << 8) | (*self.d.get(self.p + i)? as u32);
        }
        self.p += 4;
        Some(v as i32)
    }

    fn long(&mut self) -> Option<i64> {
        let mut v: u64 = 0;
        for i in 0..8 {
            v = (v << 8) | (*self.d.get(self.p + i)? as u64);
        }
        self.p += 8;
        Some(v as i64)
    }

    fn string(&mut self) -> Option<String> {
        let len = self.ushort()? as usize;
        let end = self.p.checked_add(len)?;
        let bytes = self.d.get(self.p..end)?;
        self.p = end;
        String::from_utf8(bytes.to_vec()).ok()
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        let end = self.p.checked_add(n)?;
        self.d.get(self.p..end)?;
        self.p = end;
        Some(())
    }

    /// 读一个标签的负载（名字由调用方单独读）。named=false 的列表元素也走这里。
    fn value(&mut self, ty: u8, depth: usize) -> Option<Nbt> {
        if depth > 12 {
            return None;
        }
        match ty {
            0 => Some(Nbt::Other),
            1 => {
                self.byte()?;
                Some(Nbt::Other)
            }
            2 => {
                self.skip(2)?;
                Some(Nbt::Other)
            }
            3 => self.int().map(Nbt::Int),
            4 => self.long().map(Nbt::Long),
            5 => {
                self.skip(4)?;
                Some(Nbt::Other)
            }
            6 => {
                self.skip(8)?;
                Some(Nbt::Other)
            }
            7 => {
                let n = self.int()?;
                if !(0..=10_000_000).contains(&n) {
                    return None;
                }
                self.skip(n as usize)?;
                Some(Nbt::Other)
            }
            8 => self.string().map(Nbt::Str),
            9 => {
                let elem = self.byte()?;
                let n = self.int()?;
                if !(0..=100_000).contains(&n) {
                    return None;
                }
                for _ in 0..n {
                    self.value(elem, depth + 1)?;
                }
                Some(Nbt::Other)
            }
            10 => self.compound(depth + 1),
            11 => {
                let n = self.int()?;
                if !(0..=10_000_000).contains(&n) {
                    return None;
                }
                self.skip((n as usize).checked_mul(4)?)?;
                Some(Nbt::Other)
            }
            12 => {
                let n = self.int()?;
                if !(0..=10_000_000).contains(&n) {
                    return None;
                }
                self.skip((n as usize).checked_mul(8)?)?;
                Some(Nbt::Other)
            }
            _ => None,
        }
    }

    fn compound(&mut self, depth: usize) -> Option<Nbt> {
        if depth > 12 {
            return None;
        }
        let mut fields: Vec<(String, Nbt)> = Vec::new();
        loop {
            let ty = self.byte()?;
            if ty == 0 {
                break;
            }
            let name = self.string()?;
            let value = self.value(ty, depth)?;
            fields.push((name, value));
        }
        Some(Nbt::Compound(fields))
    }
}

fn parse_level(path: &Path) -> Option<Nbt> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = flate2::read::GzDecoder::new(file);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf).ok()?;
    if buf.first() != Some(&10) {
        return None; // 根标签必须是 TAG_Compound
    }
    let mut cur = Cursor { d: &buf, p: 0 };
    let _ty = cur.byte()?;
    let _root_name = cur.string()?;
    cur.compound(1)
}

// ---------------------------------------------------------------- A. 世界列表

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldInfo {
    /// 世界目录名，作为 save_id 回传给后端。
    pub id: String,
    pub name: String,
    /// None 表示种子未知（解析失败或老版本没有该字段）。
    pub seed: Option<String>,
    pub day_time: i64,
    /// 估算游玩小时数：DayTime / 72000（MC 一天 24000 tick = 20 分钟真实时间）。
    pub play_hours: f64,
    /// LastPlayed，毫秒时间戳。
    pub last_played: i64,
    pub size_bytes: u64,
    pub has_icon: bool,
    pub has_screenshots: bool,
    pub path: String,
    /// level.dat 是否成功解析。
    pub parsed: bool,
}

#[tauri::command]
pub fn saves_list(version_id: String) -> Vec<WorldInfo> {
    let mut out = Vec::new();
    let Ok(base) = game_dir(&version_id) else {
        return out;
    };
    let Ok(rd) = std::fs::read_dir(base.join("saves")) else {
        return out;
    };
    let shots_dir = base.join("screenshots");

    for entry in rd.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let dir = entry.path();
        let id = entry.file_name().to_string_lossy().to_string();
        let level = dir.join("level.dat");
        let root = if level.is_file() {
            parse_level(&level)
        } else {
            None
        };
        let parsed = root.is_some();
        let data = root.as_ref().and_then(|r| r.get("Data"));

        let name = data
            .and_then(|d| d.get("LevelName"))
            .and_then(|n| n.text())
            .unwrap_or(&id)
            .to_string();
        let seed = data
            .and_then(|d| {
                d.get("RandomSeed")
                    .or_else(|| d.get("WorldGenSettings").and_then(|w| w.get("seed")))
            })
            .and_then(|n| n.num())
            .map(|v| v.to_string());
        let day_time = data
            .and_then(|d| d.get("DayTime"))
            .and_then(|n| n.num())
            .unwrap_or(0);
        let play_hours = ((day_time as f64) / 72_000.0 * 10.0).round() / 10.0;
        let last_played = data
            .and_then(|d| d.get("LastPlayed"))
            .and_then(|n| n.num())
            .unwrap_or(0);

        out.push(WorldInfo {
            id,
            name,
            seed,
            day_time,
            play_hours,
            last_played,
            size_bytes: dir_size(&dir),
            has_icon: dir.join("icon.png").is_file(),
            has_screenshots: any_file(&dir.join("screenshots")) || any_file(&shots_dir),
            path: dir.to_string_lossy().to_string(),
            parsed,
        });
    }

    out.sort_by_key(|a| std::cmp::Reverse(a.last_played));
    out
}

// ---------------------------------------------------------------- B. 备份恢复

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct BackupRecord {
    version_id: String,
    world: String,
    /// 备份目录名：`<world>-<timestamp>`。
    folder: String,
    label: String,
    created: i64,
    files: usize,
}

type BackupIndex = BTreeMap<String, BackupRecord>;

fn backup_root() -> PathBuf {
    paths::paths()
        .data_root
        .join("feature-files")
        .join("save-backups")
}

fn version_backup_dir(version_id: &str) -> Result<PathBuf, String> {
    let v = safe_segment(version_id).ok_or_else(|| "实例 id 不合法".to_string())?;
    Ok(backup_root().join(v))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: String,
    pub version_id: String,
    pub world: String,
    pub label: String,
    pub created: i64,
    pub files: usize,
    pub folder: String,
}

fn to_info(id: String, r: BackupRecord) -> BackupInfo {
    BackupInfo {
        id,
        version_id: r.version_id,
        world: r.world,
        label: r.label,
        created: r.created,
        files: r.files,
        folder: r.folder,
    }
}

#[tauri::command]
pub fn saves_backup(
    version_id: String,
    save_id: String,
    label: String,
) -> Result<BackupInfo, String> {
    let src = world_dir(&version_id, &save_id)?;
    if !src.is_dir() {
        return Err("找不到世界目录".to_string());
    }
    let vbase = version_backup_dir(&version_id)?;
    let stamp = store::now_secs();
    let mut folder = format!("{save_id}-{stamp}");
    let mut dest = vbase.join(&folder);
    let mut n = 2;
    while dest.exists() {
        folder = format!("{save_id}-{stamp}-{n}");
        dest = vbase.join(&folder);
        n += 1;
    }

    let files = store::copy_dir(&src, &dest)?;
    let rec = BackupRecord {
        version_id: version_id.clone(),
        world: save_id,
        folder: folder.clone(),
        label: label.trim().to_string(),
        created: stamp,
        files,
    };
    let manifest = serde_json::to_string_pretty(&rec).unwrap_or_default();
    let _ = std::fs::write(dest.join("manifest.json"), manifest);

    let id = format!("{version_id}::{folder}");
    let mut index: BackupIndex = store::load(BACKUP_INDEX);
    index.insert(id.clone(), rec.clone());
    store::save(BACKUP_INDEX, &index)?;
    Ok(to_info(id, rec))
}

#[tauri::command]
pub fn saves_list_backups(version_id: String) -> Vec<BackupInfo> {
    let index: BackupIndex = store::load(BACKUP_INDEX);
    let mut out: Vec<BackupInfo> = index
        .into_iter()
        .filter(|(_, r)| r.version_id == version_id)
        .map(|(id, r)| to_info(id, r))
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.created));
    out
}

/// 把备份目录里的世界内容复制回目标；manifest.json 不属于世界，跳过。
fn copy_backup_contents(src: &Path, dst: &Path) -> Result<(), String> {
    let rd = std::fs::read_dir(src).map_err(|e| e.to_string())?;
    for entry in rd {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name.to_str() == Some("manifest.json") {
            continue;
        }
        let to = dst.join(&name);
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            store::copy_dir(&entry.path(), &to)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn saves_restore(version_id: String, backup_id: String) -> Result<(), String> {
    let index: BackupIndex = store::load(BACKUP_INDEX);
    let rec = index
        .get(&backup_id)
        .ok_or_else(|| "备份记录不存在".to_string())?;
    if rec.version_id != version_id {
        return Err("备份与当前实例不匹配".to_string());
    }
    let src = version_backup_dir(&rec.version_id)?.join(&rec.folder);
    if !src.is_dir() {
        return Err("备份目录不存在".to_string());
    }

    let world = safe_segment(&rec.world).ok_or_else(|| "世界名不合法".to_string())?;
    let saves = game_dir(&version_id)?.join("saves");
    let dst = saves.join(&world);

    // 不删用户数据：先把当前世界改名留底，再把备份复制成一个全新的世界目录。
    if dst.exists() {
        let bak = saves.join(format!("{world}.bak-{}", store::short_id()));
        std::fs::rename(&dst, &bak).map_err(|e| format!("重命名当前世界失败：{e}"))?;
    }
    std::fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    copy_backup_contents(&src, &dst)
}

#[tauri::command]
pub fn saves_delete_backup(backup_id: String) -> Result<(), String> {
    let mut index: BackupIndex = store::load(BACKUP_INDEX);
    let rec = index
        .remove(&backup_id)
        .ok_or_else(|| "备份记录不存在".to_string())?;
    let dir = version_backup_dir(&rec.version_id)?.join(&rec.folder);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    store::save(BACKUP_INDEX, &index)
}

// ---------------------------------------------------------------- C. 截图画廊

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotInfo {
    pub name: String,
    pub size: u64,
    pub modified: i64,
    /// 超过 3MB 或读取失败时为空串。
    pub thumb: String,
    pub too_large: bool,
    pub path: String,
}

fn scan_shots(dir: &Path, seen: &mut HashSet<PathBuf>, out: &mut Vec<ScreenshotInfo>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
            continue;
        }
        let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !seen.insert(key) {
            continue;
        }
        let (thumb, too_large) = match store::data_url(&path, MAX_THUMB) {
            Ok(url) => (url, false),
            Err(_) => (String::new(), true),
        };
        out.push(ScreenshotInfo {
            name: path
                .file_name()
                .map(|v| v.to_string_lossy().to_string())
                .unwrap_or_default(),
            size: meta.len(),
            modified: modified_secs(&path),
            thumb,
            too_large,
            path: path.to_string_lossy().to_string(),
        });
    }
}

#[tauri::command]
pub fn screenshots_list(version_id: String) -> Vec<ScreenshotInfo> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    if let Ok(base) = game_dir(&version_id) {
        scan_shots(&base.join("screenshots"), &mut seen, &mut out);
    }
    scan_shots(
        &paths::paths().game_dir.join("screenshots"),
        &mut seen,
        &mut out,
    );
    out.sort_by_key(|a| std::cmp::Reverse(a.modified));
    out
}

#[tauri::command]
pub fn screenshots_delete(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_file() {
        return Err("文件不存在".to_string());
    }
    if !p
        .components()
        .any(|c| c.as_os_str().to_str() == Some("screenshots"))
    {
        return Err("只能删除 screenshots 目录下的文件".to_string());
    }
    let raw_root = paths::paths().data_root.clone();
    let root = raw_root.canonicalize().unwrap_or(raw_root);
    let canon = p.canonicalize().map_err(|e| e.to_string())?;
    if !canon.starts_with(&root) {
        return Err("路径不在 GlassMC 数据目录内".to_string());
    }
    std::fs::remove_file(&canon).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- D. 单存档数据包

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatapackInfo {
    /// 逻辑名（去掉 `.disabled` 后缀），用于切换。
    pub name: String,
    /// 磁盘上的真实名字，仅展示用。
    pub file: String,
    pub enabled: bool,
    pub is_dir: bool,
    pub size: u64,
}

#[tauri::command]
pub fn saves_datapacks(version_id: String, save_id: String) -> Vec<DatapackInfo> {
    let mut out = Vec::new();
    let Ok(world) = world_dir(&version_id, &save_id) else {
        return out;
    };
    let Ok(rd) = std::fs::read_dir(world.join("datapacks")) else {
        return out;
    };
    for entry in rd.flatten() {
        let file = entry.file_name().to_string_lossy().to_string();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let is_dir = meta.is_dir();
        let (name, enabled) = match file.strip_suffix(".disabled") {
            Some(rest) => (rest.to_string(), false),
            None => (file.clone(), true),
        };
        if !is_dir && !name.to_ascii_lowercase().ends_with(".zip") {
            continue;
        }
        out.push(DatapackInfo {
            name,
            file,
            enabled,
            is_dir,
            size: if is_dir {
                dir_size(&entry.path())
            } else {
                meta.len()
            },
        });
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

#[tauri::command]
pub fn saves_toggle_datapack(
    version_id: String,
    save_id: String,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    let world = world_dir(&version_id, &save_id)?;
    let n = safe_segment(&name).ok_or_else(|| "数据包名不合法".to_string())?;
    let dp = world.join("datapacks");
    let (from, to) = if enabled {
        (dp.join(format!("{n}.disabled")), dp.join(&n))
    } else {
        (dp.join(&n), dp.join(format!("{n}.disabled")))
    };
    if !from.exists() {
        return Err(format!("找不到数据包：{n}"));
    }
    if to.exists() {
        return Err("目标名称已被占用".to_string());
    }
    std::fs::rename(from, to).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn saves_add_datapack(version_id: String, save_id: String, path: String) -> Result<(), String> {
    let world = world_dir(&version_id, &save_id)?;
    let src = Path::new(&path);
    if !src.is_file() {
        return Err("文件不存在".to_string());
    }
    // 必须是真 zip，不能只看后缀。
    let is_zip = std::fs::File::open(src)
        .ok()
        .and_then(|f| zip::ZipArchive::new(f).ok())
        .is_some();
    if !is_zip {
        return Err("不是有效的 zip 数据包".to_string());
    }
    let file_name = safe_segment(
        src.file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| "文件名不合法".to_string())?,
    )
    .ok_or_else(|| "文件名不合法".to_string())?;
    if !file_name.to_ascii_lowercase().ends_with(".zip") {
        return Err("请选择 .zip 文件".to_string());
    }

    let dp = world.join("datapacks");
    std::fs::create_dir_all(&dp).map_err(|e| e.to_string())?;
    let dst = dp.join(&file_name);
    if dst.exists() {
        return Err("同名数据包已存在".to_string());
    }
    std::fs::copy(src, &dst).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------- E. 投影文件

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicInfo {
    pub name: String,
    pub size: u64,
    pub modified: i64,
    pub format: String,
    pub path: String,
}

fn schematic_format(ext: &str) -> Option<&'static str> {
    match ext {
        "litematic" => Some("Litematic"),
        "schematic" => Some("Schematic"),
        "nbt" => Some("NBT"),
        _ => None,
    }
}

fn walk_schematics(
    dir: &Path,
    depth: usize,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<SchematicInfo>,
) {
    if depth > 6 || out.len() >= MAX_SCHEMATICS {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        if out.len() >= MAX_SCHEMATICS {
            break;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            // 跳过 versions/<id>/jars：里面全是启动器自己的 jar，不是投影。
            let parent_versions = dir
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                == Some("versions");
            if parent_versions && name.eq_ignore_ascii_case("jars") {
                continue;
            }
            walk_schematics(&path, depth + 1, seen, out);
        } else if meta.is_file() {
            let ext = path
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if let Some(fmt) = schematic_format(&ext) {
                let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
                if !seen.insert(key) {
                    continue;
                }
                out.push(SchematicInfo {
                    name,
                    size: meta.len(),
                    modified: modified_secs(&path),
                    format: fmt.to_string(),
                    path: path.to_string_lossy().to_string(),
                });
            }
        }
    }
}

#[tauri::command]
pub fn schematic_list(version_id: String) -> Vec<SchematicInfo> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    if let Ok(inst) = game_dir(&version_id) {
        walk_schematics(&inst, 0, &mut seen, &mut out);
    }
    let root = paths::paths().game_dir.clone();
    if root.exists() {
        walk_schematics(&root, 0, &mut seen, &mut out);
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.modified));
    out
}

#[tauri::command]
pub fn schematic_import(version_id: String, path: String) -> Result<(), String> {
    let src = Path::new(&path);
    if !src.is_file() {
        return Err("文件不存在".to_string());
    }
    let ext = src
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if schematic_format(&ext).is_none() {
        return Err("不支持的格式（仅 .litematic / .schematic / .nbt）".to_string());
    }
    let name = safe_segment(
        src.file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| "文件名不合法".to_string())?,
    )
    .ok_or_else(|| "文件名不合法".to_string())?;

    let dst_dir = game_dir(&version_id)?.join("schematics");
    std::fs::create_dir_all(&dst_dir).map_err(|e| e.to_string())?;
    let dst = dst_dir.join(&name);
    if dst.exists() {
        return Err("同名文件已存在".to_string());
    }
    std::fs::copy(src, &dst).map_err(|e| e.to_string())?;
    Ok(())
}

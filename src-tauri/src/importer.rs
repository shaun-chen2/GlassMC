//! 从其它启动器导入游戏数据。
//!
//! 覆盖三个来源：官方启动器、HMCL、Prism/MultiMC。
//! 账号凭证在钥匙串或加密字段里，文件读不到，所以导入的账号一律标记「需要重新登录」；
//! HMCL 的 `hmcl.json` 里若带明文 token 则尽量复用。

use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::account::{self, Account};
use crate::manifest;
use crate::paths;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// 用「启动器|游戏目录」做稳定 id，前端回传即可定位
    pub id: String,
    pub launcher: String,
    pub name: String,
    pub root_path: String,
    pub game_dir: String,
    pub account_count: usize,
    pub version_count: usize,
}

const MERGE_INTO: [&str; 7] = [
    "versions",
    "mods",
    "saves",
    "resourcepacks",
    "shaderpacks",
    "config",
    "libraries",
];

pub fn detect() -> Vec<Candidate> {
    let mut out = Vec::new();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    // 官方启动器
    let official = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/minecraft")
    } else if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(|p| PathBuf::from(p).join(".minecraft"))
            .unwrap_or_else(|_| home.join(".minecraft"))
    } else {
        home.join(".minecraft")
    };
    push_candidate(&mut out, "official", "官方启动器", &official, &official, 0);

    // HMCL（含 app 内嵌目录这个常见位置）
    let hmcl_roots = [
        PathBuf::from("/Applications/HMCL.app/Contents/Resources/.minecraft"),
        home.join(".minecraft"),
        home.join("HMCL/.minecraft"),
        home.join("Library/Application Support/HMCL/.minecraft"),
    ];
    for r in hmcl_roots {
        if !r.is_dir() {
            continue;
        }
        if out.iter().any(|c| c.game_dir == r.to_string_lossy()) {
            continue;
        }
        let count = hmcl_accounts(&r).len();
        push_candidate(&mut out, "hmcl", "HMCL", &r, &r, count);
    }

    // Prism / MultiMC / PolyMC
    let prism_roots = [
        (
            "Prism/MultiMC",
            home.join("Library/Application Support/PrismLauncher"),
        ),
        (
            "Prism/MultiMC",
            home.join("Library/Application Support/MultiMC"),
        ),
        (
            "Prism/MultiMC",
            home.join("Library/Application Support/PolyMC"),
        ),
        ("Prism/MultiMC", home.join(".local/share/PrismLauncher")),
        (
            "Prism/MultiMC",
            std::env::var("APPDATA")
                .map(|p| PathBuf::from(p).join("PrismLauncher"))
                .unwrap_or_else(|_| home.join("PrismLauncher")),
        ),
    ];
    for (name, root) in prism_roots {
        let instances = root.join("instances");
        if !instances.is_dir() {
            continue;
        }
        if out
            .iter()
            .any(|c| c.game_dir == instances.to_string_lossy())
        {
            continue;
        }
        push_candidate(&mut out, "prism", name, &root, &instances, 0);
    }

    out
}

fn push_candidate(
    out: &mut Vec<Candidate>,
    launcher: &str,
    name: &str,
    root: &Path,
    game_dir: &Path,
    accounts: usize,
) {
    if !root.is_dir() {
        return;
    }
    let versions = count_dirs(&game_dir.join("versions"));
    if versions == 0 && launcher == "prism" {
        // Prism 的 instances 下每个子目录就是一个实例，数量即版本数
        let n = count_dirs(game_dir);
        if n == 0 {
            return;
        }
        out.push(Candidate {
            id: format!("{launcher}|{}", game_dir.to_string_lossy()),
            launcher: launcher.to_string(),
            name: name.to_string(),
            root_path: root.to_string_lossy().to_string(),
            game_dir: game_dir.to_string_lossy().to_string(),
            account_count: accounts,
            version_count: n,
        });
        return;
    }
    if versions == 0 {
        return;
    }
    out.push(Candidate {
        id: format!("{launcher}|{}", game_dir.to_string_lossy()),
        launcher: launcher.to_string(),
        name: name.to_string(),
        root_path: root.to_string_lossy().to_string(),
        game_dir: game_dir.to_string_lossy().to_string(),
        account_count: accounts,
        version_count: versions,
    });
}

fn count_dirs(p: &Path) -> usize {
    match std::fs::read_dir(p) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .count(),
        Err(_) => 0,
    }
}

/// 执行导入：把来源的版本/模组/存档/资源包合并进 GlassMC，并尽量迁移 HMCL 账号。
pub fn import(launcher: &str, game_dir: &str) -> Result<usize, String> {
    let src = PathBuf::from(game_dir);
    if !src.is_dir() {
        return Err(format!("目录不存在：{game_dir}"));
    }
    let dst = paths::paths().game_dir.clone();
    std::fs::create_dir_all(&dst).map_err(|e| format!("创建目标目录失败：{e}"))?;

    if launcher == "prism" {
        let Ok(rd) = std::fs::read_dir(&src) else {
            return Err("读取实例目录失败".into());
        };
        for entry in rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()) {
            let mc = entry.path().join(".minecraft");
            let source = if mc.is_dir() { mc } else { entry.path() };
            merge_dir(&source, &dst, true)?;
        }
    } else {
        merge_dir(&src, &dst, true)?;
    }

    if launcher == "hmcl" {
        for mut acc in hmcl_accounts(&src) {
            if account::load_all()
                .iter()
                .any(|a| a.username == acc.username)
            {
                continue;
            }
            acc.id = account::random_id();
            let _ = account::upsert(acc);
        }
    }

    Ok(manifest::scan_installed().len())
}

/// 递归合并目录：同名目录继续递归，同名文件覆盖。
fn merge_dir(src: &Path, dst: &Path, top: bool) -> Result<(), String> {
    let entries: Vec<PathBuf> = if top {
        MERGE_INTO
            .iter()
            .map(|n| src.join(n))
            .filter(|p| p.exists())
            .collect()
    } else {
        match std::fs::read_dir(src) {
            Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).collect(),
            Err(_) => return Ok(()),
        }
    };

    for item in entries {
        let Some(name) = item.file_name() else {
            continue;
        };
        let target = dst.join(name);
        if item.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| format!("创建目录失败：{e}"))?;
            merge_dir(&item, &target, false)?;
        } else {
            if target.exists() {
                let _ = std::fs::remove_file(&target);
            }
            std::fs::copy(&item, &target)
                .map_err(|e| format!("复制 {} 失败：{e}", item.display()))?;
        }
    }
    Ok(())
}

/// 解析 HMCL 的 hmcl.json，提取账号（拿不到 token 的标记为需要重新登录）。
pub fn hmcl_accounts(game_dir: &Path) -> Vec<Account> {
    let candidates = [
        game_dir.join("hmcl.json"),
        game_dir
            .parent()
            .map(|p| p.join("hmcl.json"))
            .unwrap_or_else(|| PathBuf::from("hmcl.json")),
    ];
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let mut out = Vec::new();
        for key in ["authInfo", "accounts"] {
            if let Some(list) = v.get(key).and_then(|x| x.as_array()) {
                for item in list {
                    out.push(account_from_hmcl(item));
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

fn account_from_hmcl(v: &serde_json::Value) -> Account {
    let username = v
        .get("username")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("displayName").and_then(|x| x.as_str()))
        .unwrap_or("未知用户")
        .to_string();
    let token = v
        .get("accessToken")
        .and_then(|x| x.as_str())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string());
    let kind = v
        .get("type")
        .and_then(|x| x.as_str())
        .unwrap_or("microsoft")
        .to_string();
    Account {
        id: account::random_id(),
        kind,
        username,
        uuid: v
            .get("uuid")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        access_token: token.clone(),
        source: "hmcl".into(),
        needs_reauth: token.is_none(),
        ..Default::default()
    }
}

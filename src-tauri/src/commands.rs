//! 前端可调用的命令。所有耗时操作都在这里异步执行，避免卡界面。

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::account::{self, Account};
use crate::config::{self, Config, ConfigState};
use crate::game;
use crate::java::{self, JavaInstall};
use crate::launch::{self, BuiltCommand};
use crate::manifest::{self, InstalledVersion, VersionEntry};
use crate::paths;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub version: String,
    pub data_root: String,
    pub game_dir: String,
    pub os: String,
    pub arch: String,
    pub platform: String,
    pub glass_hint: String,
    pub config: Config,
    pub accounts: Vec<Account>,
    pub active_account: Account,
    pub installed: Vec<InstalledVersion>,
    pub java: Vec<JavaInstall>,
    pub running: bool,
}

fn platform_label() -> String {
    if cfg!(target_os = "macos") {
        "macOS".into()
    } else if cfg!(target_os = "windows") {
        "Windows".into()
    } else {
        "Linux".into()
    }
}

/// 后端对玻璃能力的建议值。真正的判定还要叠加前端的 CSS 能力检测。
fn glass_hint() -> String {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        "liquid".into()
    } else {
        "frosted".into()
    }
}

pub fn resolve_account(cfg: &Config) -> Account {
    if !cfg.active_account.is_empty() {
        if let Some(a) = account::find(&cfg.active_account) {
            return a;
        }
    }
    if let Some(a) = account::load_all().into_iter().next() {
        return a;
    }
    Account::offline(if cfg.offline_name.trim().is_empty() {
        "Steve"
    } else {
        cfg.offline_name.trim()
    })
}

#[tauri::command]
pub fn bootstrap(state: State<'_, ConfigState>) -> Bootstrap {
    let cfg = state.0.lock().unwrap().clone();
    let accounts = account::load_all();
    let active = resolve_account(&cfg);
    Bootstrap {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_root: paths::paths().data_root.to_string_lossy().to_string(),
        game_dir: paths::paths().game_dir.to_string_lossy().to_string(),
        os: launch::os_name().to_string(),
        arch: launch::arch_name(),
        platform: platform_label(),
        glass_hint: glass_hint(),
        installed: manifest::scan_installed(),
        java: java::installed(&cfg.java_path),
        running: game::is_running(),
        active_account: active,
        accounts,
        config: cfg,
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, ConfigState>) -> Config {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_config(
    state: State<'_, ConfigState>,
    patch: serde_json::Value,
) -> Result<Config, String> {
    let mut guard = state.0.lock().unwrap();
    let mut current = serde_json::to_value(&*guard).map_err(|e| e.to_string())?;
    if let (Some(cur), Some(patch)) = (current.as_object_mut(), patch.as_object()) {
        for (k, v) in patch {
            cur.insert(k.clone(), v.clone());
        }
    }
    let next: Config =
        serde_json::from_value(current).map_err(|e| format!("设置字段不合法：{e}"))?;
    config::save(&next)?;
    *guard = next.clone();
    Ok(next)
}

#[tauri::command]
pub fn scan_versions() -> Vec<InstalledVersion> {
    manifest::scan_installed()
}

#[tauri::command]
pub async fn list_remote(
    state: State<'_, ConfigState>,
    include_snapshots: bool,
) -> Result<Vec<VersionEntry>, String> {
    let cfg = state.0.lock().unwrap().clone();
    let m = manifest::fetch_manifest(&cfg).await?;
    let mut v: Vec<VersionEntry> = if include_snapshots {
        m.versions
    } else {
        m.versions.into_iter().filter(|x| x.kind == "release").collect()
    };
    v.truncate(600);
    Ok(v)
}

#[tauri::command]
pub fn list_java(state: State<'_, ConfigState>) -> Vec<JavaInstall> {
    let cfg = state.0.lock().unwrap().clone();
    java::installed(&cfg.java_path)
}

/// 只构建命令不启动，用于界面展示「这一版会怎么启动」。
#[tauri::command]
pub async fn preview_command(
    state: State<'_, ConfigState>,
    version_id: String,
) -> Result<BuiltCommand, String> {
    let cfg = state.0.lock().unwrap().clone();
    let acc = resolve_account(&cfg);
    let required = launch::required_java(&version_id, &cfg).await;
    let java = java::best_for(required, &cfg.java_path)
        .ok_or_else(|| "没有找到可用的 Java，请先在设置里指定".to_string())?;
    launch::build_command(&version_id, &acc, &java, &cfg).await
}

#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    state: State<'_, ConfigState>,
    version_id: String,
) -> Result<u32, String> {
    if game::is_running() {
        return Err("已经有一个游戏在运行了".into());
    }
    let cfg = {
        let mut guard = state.0.lock().unwrap();
        guard.last_version = version_id.clone();
        let snapshot = guard.clone();
        let _ = config::save(&snapshot);
        snapshot
    };
    let acc = resolve_account(&cfg);
    let required = launch::required_java(&version_id, &cfg).await;
    let java = java::best_for(required, &cfg.java_path)
        .ok_or_else(|| "没有找到可用的 Java，请先在设置里指定一个".to_string())?;
    let built = launch::build_command(&version_id, &acc, &java, &cfg).await?;
    game::spawn(app, &built)
}

#[tauri::command]
pub fn kill_game() -> Result<(), String> {
    game::kill()
}

#[tauri::command]
pub fn game_running() -> bool {
    game::is_running()
}

#[tauri::command]
pub fn open_path(_app: AppHandle, path: String) -> Result<(), String> {
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn window_action(window: tauri::WebviewWindow, action: String) -> Result<(), String> {
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string()),
        "maximize" => {
            if window.is_maximized().unwrap_or(false) {
                window.unmaximize().map_err(|e| e.to_string())
            } else {
                window.maximize().map_err(|e| e.to_string())
            }
        }
        "close" => {
            if let Some(app) = window.app_handle().get_webview_window("main") {
                let _ = app.close();
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 玻璃不透明度由前端 CSS 变量控制（`--glass-alpha`）。
/// 窗口级 alpha 在三个平台表现不一致，所以后端不插手，只保留这个空实现给旧前端兼容。
#[tauri::command]
pub fn set_opacity(_window: tauri::WebviewWindow, _value: f64) {}

#[tauri::command]
pub fn account_list() -> Vec<Account> {
    account::load_all()
}

#[tauri::command]
pub fn set_active_account(state: State<'_, ConfigState>, id: String) -> Result<Config, String> {
    let mut guard = state.0.lock().unwrap();
    guard.active_account = id;
    let next = guard.clone();
    config::save(&next)?;
    Ok(next)
}

#[tauri::command]
pub fn create_offline_account(
    state: State<'_, ConfigState>,
    name: String,
) -> Result<Vec<Account>, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("名字不能为空".into());
    }
    let mut list = account::load_all();
    if list.iter().any(|a| a.username.eq_ignore_ascii_case(&name)) {
        return Err(format!("已经有一个叫 {name} 的账号了"));
    }
    let acc = Account {
        id: account::random_id(),
        kind: "offline".into(),
        username: name.clone(),
        uuid: Some(account::offline_uuid(&name)),
        source: "glassmc".into(),
        ..Default::default()
    };
    list.push(acc.clone());
    let p = paths::paths().accounts_file.clone();
    let s = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(&p, s).map_err(|e| format!("写入账号失败：{e}"))?;

    let mut guard = state.0.lock().unwrap();
    guard.active_account = acc.id.clone();
    let next = guard.clone();
    config::save(&next)?;
    Ok(list)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
    pub name: String,
    /// 展示名（去掉 .jar / .disabled 后缀）
    pub label: String,
    pub size: u64,
    pub enabled: bool,
    /// 修改时间（Unix 秒）
    pub mtime: u64,
}

/// 列出某个版本对应的 mods 目录内容。只读，不做 jar 元数据解析。
#[tauri::command]
pub fn list_mods(version_id: String) -> Vec<ModFile> {
    let dir = manifest::game_directory(&version_id).join("mods");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<ModFile> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if !(lower.ends_with(".jar") || lower.ends_with(".jar.disabled") || lower.ends_with(".disabled")) {
                return None;
            }
            let meta = e.metadata().ok()?;
            let enabled = !lower.ends_with(".disabled");
            let label = name
                .trim_end_matches(".disabled")
                .trim_end_matches(".jar")
                .to_string();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some(ModFile {
                name,
                label,
                size: meta.len(),
                enabled,
                mtime,
            })
        })
        .collect();
    out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    out
}

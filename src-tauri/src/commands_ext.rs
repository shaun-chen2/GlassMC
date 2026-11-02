//! 第二阶段命令：安装、账号、皮肤、模组、导入、联机。
//!
//! 约定：所有耗时命令都是 `async`，进度通过 `progress` 事件推给前端。

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::account::{self, Account};
use crate::config::{self, AuthServer, ConfigState};
use crate::importer;
use crate::injector;
use crate::install::{self, InstallOutcome, VersionDetail};
use crate::java::{self, JavaInstall};
use crate::javadl;
use crate::loader;
use crate::microsoft;
use crate::modpack;
use crate::modrinth;
use crate::paths;
use crate::skins;
use crate::terracotta;
use crate::yggdrasil;

fn cfg(state: &State<'_, ConfigState>) -> crate::config::Config {
    state.0.lock().unwrap().clone()
}

fn save(state: &State<'_, ConfigState>, f: impl FnOnce(&mut crate::config::Config)) -> Result<crate::config::Config, String> {
    let mut guard = state.0.lock().unwrap();
    f(&mut guard);
    let next = guard.clone();
    config::save(&next)?;
    Ok(next)
}

// ---------------------------------------------------------------- 安装

#[tauri::command]
pub async fn install_version(
    app: AppHandle,
    state: State<'_, ConfigState>,
    version_id: String,
) -> Result<InstallOutcome, String> {
    let cfg = cfg(&state);
    install::install_version(Some(app), &version_id, &cfg).await
}

#[tauri::command]
pub async fn version_detail(
    state: State<'_, ConfigState>,
    version_id: String,
) -> Result<VersionDetail, String> {
    let cfg = cfg(&state);
    install::version_detail(&version_id, &cfg).await
}

#[tauri::command]
pub fn delete_version(version_id: String) -> Result<(), String> {
    install::delete_version(&version_id)
}

#[tauri::command]
pub async fn loader_versions(kind: String, game: String) -> Result<Vec<String>, String> {
    loader::versions(&kind, &game).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderInstallResult {
    pub version_id: String,
}

#[tauri::command]
pub async fn install_loader(
    app: AppHandle,
    state: State<'_, ConfigState>,
    kind: String,
    game: String,
    loader_version: String,
) -> Result<LoaderInstallResult, String> {
    let cfg = cfg(&state);
    let version_id = loader::install(Some(app), &kind, &game, &loader_version, &cfg).await?;
    Ok(LoaderInstallResult { version_id })
}

// ---------------------------------------------------------------- Java

#[tauri::command]
pub fn java_candidates() -> Vec<javadl::JavaCandidate> {
    javadl::candidates()
}

#[tauri::command]
pub async fn install_java(
    app: AppHandle,
    state: State<'_, ConfigState>,
    feature: u32,
) -> Result<JavaInstall, String> {
    let cfg = cfg(&state);
    let j = javadl::install(Some(app), feature, &cfg.java_mirror).await?;
    Ok(j)
}

#[tauri::command]
pub fn remove_java(feature: u32) -> Result<(), String> {
    javadl::remove(feature)
}

#[tauri::command]
pub fn list_java_all(state: State<'_, ConfigState>) -> Vec<JavaInstall> {
    let cfg = cfg(&state);
    java::installed(&cfg.java_path)
}

// ---------------------------------------------------------------- 账号

#[tauri::command]
pub fn accounts_all() -> Vec<Account> {
    account::load_all()
}

#[tauri::command]
pub fn remove_account(state: State<'_, ConfigState>, id: String) -> Result<Vec<Account>, String> {
    if let Some(acc) = account::find(&id) {
        if acc.is_microsoft() {
            microsoft::forget(&id);
        }
    }
    let list = account::remove(&id)?;
    let mut guard = state.0.lock().unwrap();
    if guard.active_account == id {
        guard.active_account = list.first().map(|a| a.id.clone()).unwrap_or_default();
        let next = guard.clone();
        let _ = config::save(&next);
    }
    Ok(list)
}

/// 把新账号写进列表并设为当前账号。
fn add_account(state: &State<'_, ConfigState>, acc: Account) -> Result<Vec<Account>, String> {
    let list = account::upsert(acc.clone())?;
    let mut guard = state.0.lock().unwrap();
    guard.active_account = acc.id.clone();
    let next = guard.clone();
    let _ = config::save(&next);
    Ok(list)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResult {
    pub accounts: Vec<Account>,
    pub account: Account,
}

#[tauri::command]
pub async fn ygg_login(
    state: State<'_, ConfigState>,
    server_id: String,
    username: String,
    password: String,
) -> Result<LoginResult, String> {
    let cfg = cfg(&state);
    let server = yggdrasil::find_server(&cfg, &server_id)
        .ok_or_else(|| "找不到该认证服务器，请重新选择".to_string())?;
    let acc = yggdrasil::authenticate(&server, username.trim(), &password).await?;
    let accounts = add_account(&state, acc.clone())?;
    Ok(LoginResult { accounts, account: acc })
}

#[tauri::command]
pub fn add_auth_server(
    state: State<'_, ConfigState>,
    name: String,
    base_url: String,
) -> Result<Vec<AuthServer>, String> {
    let name = name.trim().to_string();
    let base = base_url.trim().trim_end_matches('/').to_string();
    if name.is_empty() || base.is_empty() {
        return Err("名称和地址都不能为空".into());
    }
    if !base.starts_with("http") {
        return Err("地址要以 https:// 开头".into());
    }
    let next = save(&state, |c| {
        if !c.auth_servers.iter().any(|s| s.normalized_base() == base) {
            c.auth_servers.push(AuthServer::new(&name, &base, false));
        }
    })?;
    Ok(next.auth_servers)
}

#[tauri::command]
pub fn remove_auth_server(
    state: State<'_, ConfigState>,
    id: String,
) -> Result<Vec<AuthServer>, String> {
    let next = save(&state, |c| {
        c.auth_servers.retain(|s| s.id != id || s.is_preset);
    })?;
    Ok(next.auth_servers)
}

#[tauri::command]
pub async fn ms_start(state: State<'_, ConfigState>) -> Result<microsoft::DeviceCode, String> {
    let cfg = cfg(&state);
    microsoft::start(&cfg).await
}

#[tauri::command]
pub async fn ms_poll(
    state: State<'_, ConfigState>,
    device_code: String,
) -> Result<microsoft::PollResult, String> {
    let cfg = cfg(&state);
    let result = microsoft::poll(&cfg, &device_code).await?;
    if let Some(acc) = result.account.clone() {
        let _ = add_account(&state, acc);
    }
    Ok(result)
}

/// 刷新任意账号（微软重新换令牌，外置登录 refresh，离线不动）。
#[tauri::command]
pub async fn refresh_account(
    state: State<'_, ConfigState>,
    id: String,
) -> Result<Account, String> {
    let cfg = cfg(&state);
    let acc = account::find(&id).ok_or_else(|| "账号不存在".to_string())?;
    let updated = match acc.kind.as_str() {
        "microsoft" | "msa" => microsoft::refresh(&cfg, &acc).await?,
        "yggdrasil" => {
            let server = acc
                .auth_server_id
                .as_deref()
                .and_then(|sid| yggdrasil::find_server(&cfg, sid))
                .ok_or_else(|| "该账号关联的认证服务器已被删除".to_string())?;
            yggdrasil::refresh(&server, &acc).await?
        }
        _ => acc.clone(),
    };
    let _ = account::upsert(updated.clone())?;
    Ok(updated)
}

/// 同步站点皮肤（外置登录换皮肤后调它）。
#[tauri::command]
pub async fn sync_account_skin(
    state: State<'_, ConfigState>,
    id: String,
) -> Result<Account, String> {
    let cfg = cfg(&state);
    let acc = account::find(&id).ok_or_else(|| "账号不存在".to_string())?;
    match acc.kind.as_str() {
        "yggdrasil" => {
            let server = acc
                .auth_server_id
                .as_deref()
                .and_then(|sid| yggdrasil::find_server(&cfg, sid))
                .ok_or_else(|| "该账号关联的认证服务器已被删除".to_string())?;
            if let Some(updated) = yggdrasil::sync_skin(&server, &acc).await {
                let _ = account::upsert(updated.clone())?;
                return Ok(updated);
            }
            Err("没有从皮肤站查到新的贴图（可能仍是同一张皮肤）".into())
        }
        "microsoft" | "msa" => {
            let updated = microsoft::refresh(&cfg, &acc).await?;
            let _ = account::upsert(updated.clone())?;
            Ok(updated)
        }
        _ => Ok(acc),
    }
}

/// 账号皮肤（PNG data URL），前端 canvas 直接画 3D 头。
#[tauri::command]
pub async fn account_skin(id: String) -> Option<String> {
    let acc = account::find(&id)?;
    let file = skins::ensure(&acc).await?;
    skins::data_url(&file)
}

#[tauri::command]
pub async fn refresh_account_skin(id: String) -> Option<String> {
    let acc = account::find(&id)?;
    let file = skins::refresh(&acc).await?;
    skins::data_url(&file)
}

// ---------------------------------------------------------------- 皮肤库

#[tauri::command]
pub async fn skinlib_page(
    state: State<'_, ConfigState>,
    page: u32,
) -> Result<skins::SkinPage, String> {
    let cfg = cfg(&state);
    skins::library_page(&cfg.skin_library_base, page).await
}

/// 把皮肤库里的某款皮肤套到账号上（写 skinLocalPath）。
#[tauri::command]
pub async fn skinlib_apply(
    state: State<'_, ConfigState>,
    tid: i64,
    account_id: String,
) -> Result<Option<String>, String> {
    let cfg = cfg(&state);
    let file = skins::library_texture(&cfg.skin_library_base, tid).await?;
    let mut acc = account::find(&account_id).ok_or_else(|| "账号不存在".to_string())?;
    acc.skin_local_path = Some(file.to_string_lossy().to_string());
    let _ = account::upsert(acc)?;
    Ok(skins::data_url(&file))
}

#[tauri::command]
pub fn skinlib_clear(account_id: String) -> Result<(), String> {
    let mut acc = account::find(&account_id).ok_or_else(|| "账号不存在".to_string())?;
    acc.skin_local_path = None;
    let _ = account::upsert(acc)?;
    Ok(())
}

/// 皮肤库缩略图（data URL），列表里直接显示。
#[tauri::command]
pub async fn skinlib_thumb(
    state: State<'_, ConfigState>,
    tid: i64,
) -> Result<String, String> {
    let cfg = cfg(&state);
    let file = skins::library_texture(&cfg.skin_library_base, tid).await?;
    skins::data_url(&file).ok_or_else(|| "皮肤文件无效".to_string())
}

// ---------------------------------------------------------------- authlib-injector

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectorStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: String,
}

#[tauri::command]
pub fn injector_status() -> InjectorStatus {
    InjectorStatus {
        installed: injector::local().is_some(),
        version: injector::version(),
        path: injector::jar_path().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub async fn injector_install(force: bool) -> Result<InjectorStatus, String> {
    injector::ensure(force).await?;
    Ok(injector_status())
}

// ---------------------------------------------------------------- 模组

#[tauri::command]
pub fn mods_scan(version_id: String) -> Vec<modrinth::InstalledMod> {
    modrinth::scan_mods(&version_id)
}

#[tauri::command]
pub fn mods_toggle(path: String, enabled: bool) -> Result<String, String> {
    modrinth::set_mod_enabled(&path, enabled)
}

#[tauri::command]
pub fn mods_delete(path: String) -> Result<(), String> {
    modrinth::delete_mod(&path)
}

#[tauri::command]
pub fn mods_dir(version_id: String) -> String {
    modrinth::mods_dir(&version_id).to_string_lossy().to_string()
}

#[tauri::command]
pub async fn modrinth_search(
    query: String,
    game: String,
    loader: String,
    offset: u32,
) -> Result<(Vec<modrinth::SearchHit>, u32), String> {
    modrinth::search(&query, &game, &loader, offset, 20).await
}

#[tauri::command]
pub async fn modrinth_versions(
    project_id: String,
    game: String,
    loader: String,
) -> Result<Vec<modrinth::MrVersion>, String> {
    modrinth::versions(&project_id, &game, &loader).await
}

#[tauri::command]
pub async fn modrinth_install(
    version_id: String,
    project_id: String,
    game: String,
    loader: String,
    mr_version: Option<String>,
) -> Result<String, String> {
    modrinth::install_mod(
        &version_id,
        &project_id,
        &game,
        &loader,
        mr_version.as_deref(),
    )
    .await
}

// ---------------------------------------------------------------- 导入

#[tauri::command]
pub fn import_detect() -> Vec<importer::Candidate> {
    importer::detect()
}

#[tauri::command]
pub async fn import_run(launcher: String, game_dir: String) -> Result<usize, String> {
    tokio::task::spawn_blocking(move || importer::import(&launcher, &game_dir))
        .await
        .map_err(|e| format!("导入任务失败：{e}"))?
}

#[tauri::command]
pub fn modpack_list() -> Vec<modpack::PackRecord> {
    modpack::list_imported()
}

#[tauri::command]
pub fn modpack_inspect(path: String) -> Result<modpack::PackIndex, String> {
    modpack::inspect(&path)
}

#[tauri::command]
pub async fn modpack_import(
    app: AppHandle,
    path: String,
    name: Option<String>,
) -> Result<modpack::PackRecord, String> {
    modpack::import(Some(app), &path, name).await
}

#[tauri::command]
pub fn modpack_export(
    version_id: String,
    destination: String,
    name: String,
    summary: String,
) -> Result<String, String> {
    modpack::export(&version_id, &destination, &name, &summary)
}

// ---------------------------------------------------------------- 联机

#[tauri::command]
pub fn tc_snapshot() -> terracotta::Snapshot {
    terracotta::snapshot()
}

#[tauri::command]
pub async fn tc_bootstrap() -> Result<terracotta::Snapshot, String> {
    terracotta::bootstrap().await
}

#[tauri::command]
pub async fn tc_refresh() -> Result<serde_json::Value, String> {
    terracotta::refresh().await
}

#[tauri::command]
pub async fn tc_create(player: String) -> Result<serde_json::Value, String> {
    terracotta::create_room(&player).await
}

#[tauri::command]
pub async fn tc_join(room: String, player: String) -> Result<serde_json::Value, String> {
    terracotta::join_room(&room, &player).await
}

#[tauri::command]
pub async fn tc_leave() -> Result<serde_json::Value, String> {
    terracotta::leave().await
}

#[tauri::command]
pub fn tc_shutdown() {
    terracotta::shutdown();
}

// ---------------------------------------------------------------- 文件选择

/// 打开系统文件选择框。`kind` = mrpack / skin / jar / any
#[tauri::command]
pub async fn pick_file(app: AppHandle, kind: String) -> Option<String> {
    let (name, exts): (String, Vec<String>) = match kind.as_str() {
        "mrpack" => ("Modrinth 整合包".into(), vec!["mrpack".into()]),
        "png" => ("皮肤图片".into(), vec!["png".into()]),
        "jar" => ("Jar 文件".into(), vec!["jar".into()]),
        "java" => ("Java 可执行文件".into(), vec![]),
        _ => ("所有文件".into(), vec![]),
    };
    let path = tokio::task::spawn_blocking(move || {
        let mut dlg = app.dialog().file().set_title("选择文件");
        if exts.is_empty() {
            dlg = dlg.add_filter(&name, &["*"]);
        } else {
            let refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dlg = dlg.add_filter(&name, &refs);
        }
        dlg.blocking_pick_file()
    })
    .await
    .ok()??;
    Some(path.to_string())
}

/// 另存为（导出整合包用）。
#[tauri::command]
pub async fn pick_save_path(app: AppHandle, default_name: String) -> Option<String> {
    let path = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("保存整合包")
            .set_file_name(&default_name)
            .add_filter("Modrinth 整合包", &["mrpack"])
            .blocking_save_file()
    })
    .await
    .ok()??;
    Some(path.to_string())
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    let path = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("选择目录")
            .blocking_pick_folder()
    })
    .await
    .ok()??;
    Some(path.to_string())
}

// ---------------------------------------------------------------- 其它

#[tauri::command]
pub fn open_dir(path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        let _ = std::fs::create_dir_all(&p);
    }
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn data_root() -> String {
    paths::paths().data_root.to_string_lossy().to_string()
}

/// 打开某个版本的实例目录（版本隔离时是 versions/<id>，否则是游戏根目录）。
#[tauri::command]
pub fn version_dir(version_id: String) -> String {
    crate::manifest::game_directory(&version_id)
        .to_string_lossy()
        .to_string()
}

#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

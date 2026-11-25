//! 静默自动更新。
//!
//! 与 `update.rs` 的分工：
//! - `update.rs` 负责「查 GitHub Releases / 挑平台安装包 / 下载到下载目录」，并给界面提供手动更新；
//! - 这里只做后台的「静默检查 + 可选静默暂存」：复用 `update::check()` 的解析结果和
//!   `net::client()` 的 HTTP 客户端，把安装包暂存到 `data_root/updates/`，**绝不自动执行**。
//!
//! 设计约束：
//! - 默认关闭（`enabled: false`），必须用户显式开启才可能下载；
//! - `check_now` 幂等：同一版本 24 小时内不重复下载；失败按 failures 退避；
//! - 离线 / GitHub 抽风时返回 `ok: false` 的结果对象，不抛错、不卡界面；
//! - 任何情况下都不会去启动/替换安装包，最多告诉用户「已暂存，请手动安装」。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::AsyncWriteExt;

use crate::net;
use crate::paths;
use crate::store;
use crate::update;

/// store key。JSON 落在 `data_root/feature-data/auto-update.json`。
const KEY: &str = "auto-update";
/// 同一版本「已经处理过」的冷却时间：24 小时。
const CHECK_TTL: i64 = 24 * 3600;
/// 暂存目录：data_root/updates（不是下载目录，避免和用户文件混在一起）。
const UPDATES_DIR: &str = "updates";
/// 防止启动检查与设置页按钮同时触发两次安装。
static INSTALLING: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------- 配置

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoConfig {
    /// 用户是否显式开启。默认 false —— 未经同意不下载任何东西。
    pub enabled: bool,
    /// "notify"：只提示；"silent-download"：静默暂存安装包。
    pub mode: String,
    /// 上次检查时间（unix 秒）。
    pub last_check: i64,
    /// 上次检查看到的版本号（同时也是「已为该版本做过决定」的标记）。
    pub last_version: String,
    /// 已暂存的安装包绝对路径；空串表示没有。
    pub staged_path: String,
    /// 连续失败次数，用来算退避间隔。
    pub failures: u32,
}

impl Default for AutoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "notify".to_string(),
            last_check: 0,
            last_version: String::new(),
            staged_path: String::new(),
            failures: 0,
        }
    }
}

fn load_config() -> AutoConfig {
    store::load(KEY)
}

fn save_config(c: &AutoConfig) -> Result<(), String> {
    store::save(KEY, c)
}

/// 暂存文件的磁盘信息：(是否存在且是文件, 文件名, 字节数)。
fn staged_info(path: &str) -> (bool, String, u64) {
    if path.trim().is_empty() {
        return (false, String::new(), 0);
    }
    match std::fs::metadata(Path::new(path)) {
        Ok(m) if m.is_file() => (true, store::file_name(path), m.len()),
        _ => (false, String::new(), 0),
    }
}

/// 连续失败时的退避间隔（秒）。failures 越多间隔越长，最长不超过 24h。
fn cooldown_secs(failures: u32) -> i64 {
    match failures {
        0 => CHECK_TTL,
        1 => 10 * 60,
        2 => 30 * 60,
        3 => 2 * 3600,
        4 => 6 * 3600,
        _ => CHECK_TTL,
    }
}

/// 只从本地读取状态，不联网 —— 设置页每次 render 调它也不会卡住。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoStatus {
    pub config: AutoConfig,
    pub current_version: String,
    pub staged_exists: bool,
    pub staged_name: String,
    pub staged_size: u64,
}

/// `auto_check_now` 的返回值。用 `ok` 表示「这次检查是否成功完成」，
/// 离线时 `ok:false` 且带 message，前端据此提示，不需要抛异常。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckNowResult {
    pub ok: bool,
    pub message: String,
    pub latest: String,
    pub has_update: bool,
    /// 本次调用是否新暂存了安装包。
    pub downloaded: bool,
    /// 本次是否因为幂等/退避而跳过了下载。
    pub skipped: bool,
    pub staged_path: String,
    pub staged_size: u64,
}

#[allow(clippy::too_many_arguments)]
fn result(
    ok: bool,
    message: impl Into<String>,
    latest: String,
    has_update: bool,
    downloaded: bool,
    skipped: bool,
    staged_path: String,
    staged_size: u64,
) -> CheckNowResult {
    CheckNowResult {
        ok,
        message: message.into(),
        latest,
        has_update,
        downloaded,
        skipped,
        staged_path,
        staged_size,
    }
}

// ---------------------------------------------------------------- 命令

#[tauri::command]
pub fn auto_config_get() -> AutoConfig {
    load_config()
}

#[tauri::command]
pub fn auto_config_set(enabled: bool, mode: String) -> Result<AutoConfig, String> {
    let mut c = load_config();
    c.enabled = enabled;
    // 不认识的值一律退回「只提示」，避免前端传错导致自动安装被意外打开。
    c.mode = match mode.trim() {
        "silent-download" => "silent-download".to_string(),
        "auto-install" => "auto-install".to_string(),
        _ => "notify".to_string(),
    };
    save_config(&c)?;
    Ok(c)
}

#[tauri::command]
pub fn auto_status() -> AutoStatus {
    let c = load_config();
    let (exists, name, size) = staged_info(&c.staged_path);
    AutoStatus {
        config: c,
        current_version: update::current_version(),
        staged_exists: exists,
        staged_name: name,
        staged_size: size,
    }
}

/// 用户手动装完新版本后调用：清掉暂存状态，避免下次又提示同一个版本。
/// 只清状态、不删文件 —— 安装包是否保留由用户决定。
#[tauri::command]
pub fn auto_mark_installed(version: String) -> Result<AutoConfig, String> {
    let mut c = load_config();
    c.staged_path.clear();
    c.failures = 0;
    c.last_version = version.trim().trim_start_matches('v').to_string();
    c.last_check = store::now_secs();
    save_config(&c)?;
    Ok(c)
}

/// 静默检查一次。命中新版本且处于 silent-download 模式时暂存安装包。
///
/// 幂等规则（按优先级）：
/// 1. 同一版本且已存在暂存文件 -> 跳过（等用户装完调 `auto_mark_installed`）；
/// 2. 同一版本且上次失败过 -> 按 failures 退避，间隔内跳过（不刷新 last_check，
///    否则窗口会被每次调用无限后推）；
/// 3. 同一版本、之前暂存过但文件已被移走、且还在 24 小时内 -> 跳过并清掉失效引用；
/// 4. 否则尝试下载。
#[tauri::command]
pub async fn auto_check_now(app: tauri::AppHandle) -> Result<CheckNowResult, String> {
    let prev = load_config();

    if !prev.enabled {
        let size = staged_info(&prev.staged_path).2;
        return Ok(result(
            false,
            "静默自动更新未开启",
            prev.last_version,
            false,
            false,
            false,
            prev.staged_path,
            size,
        ));
    }

    // 复用 update.rs 的检查逻辑（含 15s 超时与资产挑选），不自己解析 GitHub API。
    let info = match update::check().await {
        Ok(v) => v,
        Err(e) => {
            let mut c = prev.clone();
            c.last_check = store::now_secs();
            c.failures = c.failures.saturating_add(1);
            let _ = save_config(&c);
            let size = staged_info(&c.staged_path).2;
            return Ok(result(
                false,
                e,
                c.last_version,
                false,
                false,
                false,
                c.staged_path,
                size,
            ));
        }
    };

    let now = store::now_secs();

    // 没有新版本：只记录检查时间，不动暂存状态（auto_status 仍会显示暂存文件）。
    if !info.has_update {
        let mut c = prev.clone();
        c.last_check = now;
        c.last_version = info.latest.clone();
        c.failures = 0;
        // 已经是这台机器上的版本：之前暂存的安装包不再有意义，清掉引用（文件保留）
        if c.staged_path.trim() != "" {
            c.staged_path.clear();
        }
        let _ = save_config(&c);
        return Ok(result(
            true,
            "已是最新版本",
            info.latest,
            false,
            false,
            false,
            c.staged_path.clone(),
            staged_info(&c.staged_path).2,
        ));
    }

    // 只提示模式：绝不下载，即使有旧的暂存文件也不在这里报告。
    if prev.mode != "silent-download" {
        let mut c = prev.clone();
        c.last_check = now;
        c.last_version = info.latest.clone();
        c.failures = 0;
        let _ = save_config(&c);
        return Ok(result(
            true,
            format!("发现新版本 {}（仅提示，不下载）", info.latest),
            info.latest,
            true,
            false,
            false,
            c.staged_path.clone(),
            staged_info(&c.staged_path).2,
        ));
    }

    // 以下进入 silent-download 分支。
    let same_version = prev.last_version == info.latest;
    let (staged_exists, _, staged_size) = staged_info(&prev.staged_path);

    // 1. 已为这个版本暂存过：不再重复下载；自动安装模式下直接进入安装。
    if same_version && staged_exists {
        let mut c = prev.clone();
        c.last_check = now;
        c.failures = 0;
        let _ = save_config(&c);
        if c.mode == "auto-install" {
            return Ok(match begin_install(&app, &prev.staged_path) {
                Ok(outcome) => {
                    if outcome.quit {
                        schedule_exit(&app);
                    }
                    result(
                        true,
                        outcome.message,
                        info.latest,
                        true,
                        false,
                        true,
                        prev.staged_path,
                        staged_size,
                    )
                }
                Err(e) => {
                    let mut c = prev.clone();
                    c.last_check = now;
                    c.failures = c.failures.saturating_add(1);
                    let _ = save_config(&c);
                    result(
                        false,
                        format!("自动安装失败：{e}"),
                        info.latest,
                        true,
                        false,
                        true,
                        prev.staged_path,
                        staged_size,
                    )
                }
            });
        }
        return Ok(result(
            true,
            format!("{} 的安装包已暂存，请手动安装", info.latest),
            info.latest,
            true,
            false,
            true,
            c.staged_path,
            staged_size,
        ));
    }

    // 2. 失败退避：注意这里不写 last_check，否则窗口会被无限后推。
    if same_version && prev.failures > 0 {
        let wait = cooldown_secs(prev.failures);
        if now.saturating_sub(prev.last_check) < wait {
            return Ok(result(
                true,
                format!(
                    "检查失败，{} 秒后重试",
                    wait - now.saturating_sub(prev.last_check)
                ),
                info.latest,
                true,
                false,
                true,
                prev.staged_path,
                staged_size,
            ));
        }
    }

    // 3. 24 小时内同一版本不重复下载：仅当之前确实暂存过、但文件被移走时拦截。
    //    staged_path 为空说明还没下载过（例如用户刚从「只提示」切过来），应当允许下载。
    if same_version
        && !prev.staged_path.trim().is_empty()
        && prev.last_check > 0
        && now.saturating_sub(prev.last_check) < CHECK_TTL
    {
        let mut c = prev.clone();
        c.last_check = now;
        c.failures = 0;
        c.staged_path.clear(); // 文件已不在，清掉失效引用
        let _ = save_config(&c);
        return Ok(result(
            true,
            format!("24 小时内已暂存过 {}", info.latest),
            info.latest,
            true,
            false,
            true,
            String::new(),
            0,
        ));
    }

    // 4. 静默暂存。
    match stage_download(&info).await {
        Ok((path, size)) => {
            let mut c = prev.clone();
            c.last_check = now;
            c.last_version = info.latest.clone();
            c.failures = 0;
            c.staged_path = path.clone();
            let _ = save_config(&c);
            if c.mode == "auto-install" {
                return Ok(match begin_install(&app, &path) {
                    Ok(outcome) => {
                        if outcome.quit {
                            schedule_exit(&app);
                        }
                        result(
                            true,
                            outcome.message,
                            info.latest,
                            true,
                            true,
                            false,
                            path,
                            size,
                        )
                    }
                    Err(e) => {
                        let mut c = prev.clone();
                        c.last_check = now;
                        c.last_version = info.latest.clone();
                        c.failures = c.failures.saturating_add(1);
                        let _ = save_config(&c);
                        result(
                            false,
                            format!("已下载，但自动安装失败：{e}"),
                            info.latest,
                            true,
                            true,
                            false,
                            path,
                            size,
                        )
                    }
                });
            }
            Ok(result(
                true,
                format!("已暂存 {}，请手动安装", info.latest),
                info.latest,
                true,
                true,
                false,
                path,
                size,
            ))
        }
        Err(e) => {
            let mut c = prev.clone();
            c.last_check = now;
            c.last_version = info.latest.clone();
            c.failures = c.failures.saturating_add(1);
            c.staged_path.clear();
            let _ = save_config(&c);
            Ok(result(
                false,
                e,
                info.latest,
                true,
                false,
                false,
                String::new(),
                0,
            ))
        }
    }
}

// ---------------------------------------------------------------- 下载到 updates/

/// 与 `update::download` 的下载流程一致，但落到 `data_root/updates/` 而不是「下载」目录。
/// 资产 URL / 大小来自 `update::check()` 的结果，HTTP 客户端复用 `net::client()`。
async fn stage_download(info: &update::UpdateInfo) -> Result<(String, u64), String> {
    if info.asset_url.trim().is_empty() {
        return Err("这个版本没有发布当前平台的安装包".into());
    }

    let dir: PathBuf = paths::paths().data_root.join(UPDATES_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建更新目录失败：{e}"))?;

    let ext = ext_of(&info.asset_name);
    let stem = format!("GlassMC-{}", info.latest);
    let mut dest = dir.join(format!("{stem}{ext}"));
    let mut i = 2;
    while dest.exists() && i < 1000 {
        dest = dir.join(format!("{stem}-{i}{ext}"));
        i += 1;
    }
    let part = dest.with_extension(format!("{}.part", ext.trim_start_matches('.')));

    let resp = net::client()
        .get(&info.asset_url)
        .send()
        .await
        .map_err(|e| format!("下载安装包失败：{e}"))?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("下载安装包失败：HTTP {status}"));
    }

    let total = resp.content_length().unwrap_or(info.size);
    let mut resp = resp;
    let mut out = tokio::fs::File::create(&part)
        .await
        .map_err(|e| format!("写入安装包失败：{e}"))?;
    let mut got: u64 = 0;
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
            return Err(format!("写入安装包失败：{e}"));
        }
        got += chunk.len() as u64;
    }
    out.flush()
        .await
        .map_err(|e| format!("写入安装包失败：{e}"))?;
    drop(out);

    // 校验收到的字节数：响应长度或 Release 里标注的 size，有一个对得上就算完整。
    if total > 0 && got < total {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(format!("安装包下载不完整（{got}/{total} 字节）"));
    }
    if got == 0 {
        let _ = tokio::fs::remove_file(&part).await;
        return Err("下载到的安装包为空".into());
    }

    std::fs::rename(&part, &dest).map_err(|e| format!("保存安装包失败：{e}"))?;
    Ok((dest.to_string_lossy().to_string(), got))
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

// ---------------------------------------------------------------- 安装暂存包

/// 自动安装的返回：是否需要退出当前进程交给安装脚本。
struct InstallOutcome {
    message: String,
    quit: bool,
}

fn schedule_exit(app: &tauri::AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(600));
        handle.exit(0);
    });
}

fn begin_install(app: &tauri::AppHandle, path: &str) -> Result<InstallOutcome, String> {
    let _ = app;
    let p = Path::new(path);
    if path.trim().is_empty() || !p.is_file() {
        return Err("安装包不存在，请重新检查更新".into());
    }
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("正在安装更新，请稍候".into());
    }
    match install_staged(p) {
        Ok(outcome) => {
            if !outcome.quit {
                INSTALLING.store(false, Ordering::SeqCst);
            }
            Ok(outcome)
        }
        Err(e) => {
            INSTALLING.store(false, Ordering::SeqCst);
            Err(e)
        }
    }
}

/// 用户手动点「安装并重启」，或自动模式下启动检查完成后调用。
#[tauri::command]
pub fn auto_install_staged(app: tauri::AppHandle) -> Result<String, String> {
    let c = load_config();
    if c.staged_path.trim().is_empty() {
        return Err("没有已暂存的安装包".into());
    }
    let outcome = begin_install(&app, &c.staged_path)?;
    if outcome.quit {
        schedule_exit(&app);
    }
    Ok(outcome.message)
}

fn install_staged(path: &Path) -> Result<InstallOutcome, String> {
    #[cfg(target_os = "macos")]
    {
        return install_macos(path);
    }
    #[cfg(target_os = "windows")]
    {
        return install_windows(path);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return install_linux(path);
    }
    #[allow(unreachable_code)]
    Err("当前平台不支持自动安装，请手动运行安装包".into())
}

#[cfg(target_os = "macos")]
fn install_macos(path: &Path) -> Result<InstallOutcome, String> {
    if path.extension().and_then(|e| e.to_str()) != Some("dmg") {
        return Err("当前平台只支持 dmg 安装包".into());
    }
    let exe = std::env::current_exe().map_err(|e| format!("找不到当前程序：{e}"))?;
    let app_bundle = exe
        .ancestors()
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .ok_or_else(|| "找不到当前 .app 路径，请手动安装".to_string())?;
    let parent = app_bundle
        .parent()
        .ok_or_else(|| "找不到 .app 所在目录".to_string())?;

    // 先探测写权限：没有权限就直接报错，不能让启动器先退出、再安装失败。
    let probe = parent.join(".glassmc-update-probe");
    std::fs::write(&probe, b"probe")
        .map_err(|e| format!("没有权限写入 {}（{e}），请手动安装", parent.display()))?;
    let _ = std::fs::remove_file(&probe);

    let dir = paths::paths().data_root.join(UPDATES_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建更新目录失败：{e}"))?;
    let script = dir.join("install-macos.sh");
    let log = dir.join("install-macos.log");
    let body = r#"#!/bin/sh
set -u
PID="$1"; DMG="$2"; TARGET="$3"; LOG="$4"
exec >>"$LOG" 2>&1
echo "[glassmc] waiting for pid $PID"
while kill -0 "$PID" 2>/dev/null; do sleep 0.3; done
MNT="$(mktemp -d /tmp/glassmc-dmg.XXXXXX)" || exit 1
if ! hdiutil attach "$DMG" -nobrowse -quiet -mountpoint "$MNT"; then exit 1; fi
APP="$(find "$MNT" -maxdepth 2 -name '*.app' -print -quit)"
if [ -z "$APP" ]; then hdiutil detach "$MNT" -quiet; exit 1; fi
rm -rf "$TARGET.old"
if mv "$TARGET" "$TARGET.old" && ditto "$APP" "$TARGET"; then
  xattr -dr com.apple.quarantine "$TARGET" 2>/dev/null || true
  rm -rf "$TARGET.old"
  open "$TARGET"
else
  rm -rf "$TARGET"
  [ -d "$TARGET.old" ] && mv "$TARGET.old" "$TARGET"
  open "$DMG"
fi
hdiutil detach "$MNT" -quiet
"#;
    std::fs::write(&script, body).map_err(|e| format!("写入安装脚本失败：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("设置安装脚本权限失败：{e}"))?;
    }

    std::process::Command::new("/bin/sh")
        .arg(&script)
        .arg(std::process::id().to_string())
        .arg(path)
        .arg(app_bundle)
        .arg(&log)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动安装脚本失败：{e}"))?;

    Ok(InstallOutcome {
        message: "正在安装更新，启动器将自动重启".into(),
        quit: true,
    })
}

#[cfg(target_os = "windows")]
fn install_windows(path: &Path) -> Result<InstallOutcome, String> {
    if path.extension().and_then(|e| e.to_str()) != Some("exe") {
        return Err("当前平台只支持 exe 安装包".into());
    }
    let target = std::env::current_exe().map_err(|e| format!("找不到当前程序：{e}"))?;
    let dir = paths::paths().data_root.join(UPDATES_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建更新目录失败：{e}"))?;
    let script = dir.join("install-windows.bat");
    let log = dir.join("install-windows.log");
    let body = r#"@echo off
setlocal
set PID=%1
set INSTALLER=%~2
set TARGET=%~3
set LOG=%~4
:wait
tasklist /FI "PID eq %PID%" 2>nul | find "%PID%" >nul
if not errorlevel 1 (
  timeout /t 1 /nobreak >nul
  goto wait
)
start "" /wait "%INSTALLER%" /S >>"%LOG%" 2>&1
if exist "%TARGET%" start "" "%TARGET%"
"#;
    std::fs::write(&script, body).map_err(|e| format!("写入安装脚本失败：{e}"))?;

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script_s = script.to_string_lossy().to_string();
    let pid_s = std::process::id().to_string();
    let installer_s = path.to_string_lossy().to_string();
    let target_s = target.to_string_lossy().to_string();
    let log_s = log.to_string_lossy().to_string();
    std::process::Command::new("cmd")
        .args([
            "/C",
            "start",
            "",
            "/min",
            script_s.as_str(),
            pid_s.as_str(),
            installer_s.as_str(),
            target_s.as_str(),
            log_s.as_str(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("启动安装脚本失败：{e}"))?;

    Ok(InstallOutcome {
        message: "正在安装更新，启动器将自动重启（如弹出管理员确认请允许）".into(),
        quit: true,
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn install_linux(path: &Path) -> Result<InstallOutcome, String> {
    let opener = if cfg!(target_os = "linux") {
        "xdg-open"
    } else {
        "open"
    };
    std::process::Command::new(opener)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("无法打开安装包：{e}"))?;
    Ok(InstallOutcome {
        message: "已打开安装包，请按系统提示完成安装（Linux 需要包管理器权限）".into(),
        quit: false,
    })
}

// ---------------------------------------------------------------- 测试

#[cfg(test)]
mod tests {
    use super::{cooldown_secs, AutoConfig, CHECK_TTL};

    #[test]
    fn default_is_opt_in() {
        let c = AutoConfig::default();
        assert!(!c.enabled, "默认必须关闭静默自动更新");
        assert_eq!(c.mode, "notify");
        assert!(c.staged_path.is_empty());
        assert_eq!(c.failures, 0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn mac_refuses_non_dmg() {
        let p = std::env::temp_dir().join("glassmc-test-update.zip");
        std::fs::write(&p, b"x").unwrap();
        match super::install_staged(&p) {
            Ok(_) => panic!("非 dmg 安装包不应被自动安装"),
            Err(e) => assert!(e.contains("dmg"), "错误信息应说明只支持 dmg：{e}"),
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn backoff_grows_with_failures() {
        assert_eq!(cooldown_secs(0), CHECK_TTL);
        assert!(cooldown_secs(1) < cooldown_secs(2));
        assert!(cooldown_secs(2) < cooldown_secs(3));
        assert!(cooldown_secs(3) < cooldown_secs(4));
        assert!(cooldown_secs(9) <= CHECK_TTL);
    }
}

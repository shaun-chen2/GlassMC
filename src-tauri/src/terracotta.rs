//! 陶瓦联机（Terracotta）集成。
//!
//! 做法与原生版一致：下载官方发行包，只取出裸二进制（不装 .pkg，不需要管理员权限），
//! 以 `--daemon` 拉起本地服务，然后通过 `127.0.0.1:<port>` 的 HTTP 接口操作房间。
//!
//! 两个必须注意的点：
//! - 端口写在 `~/terracotta/terracotta.lock`（2 字节大端），进程被强杀时会残留，要先清；
//! - 本地请求不能走系统代理（`net::local_client()` 已禁用）。

use serde::Serialize;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::download;
use crate::net;
use crate::paths;

pub const VERSION: &str = "0.4.2";

fn support_dir() -> PathBuf {
    paths::paths().data_root.join("terracotta")
}

fn log_file() -> PathBuf {
    support_dir().join("terracotta.log")
}

fn lock_file() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("terracotta/terracotta.lock"))
}

fn archive_name() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        // mac 与 linux 用 arm64，windows 用 arm64
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "x86_64"
    };
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    format!("terracotta-{VERSION}-{os}-{arch}-pkg.tar.gz")
}

fn exe_name() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let base = format!("terracotta-{VERSION}-{os}-{arch}");
    if cfg!(target_os = "windows") {
        format!("{base}.exe")
    } else {
        base
    }
}

fn binary_path() -> PathBuf {
    support_dir().join(exe_name())
}

struct Inner {
    ready: bool,
    port: Option<u16>,
    version: String,
    easytier: String,
    owns_process: bool,
    child: Option<Child>,
    error: Option<String>,
    prepare_text: String,
    state: serde_json::Value,
}

static INNER: Mutex<Option<Inner>> = Mutex::new(None);
static PREPARING: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub ready: bool,
    pub preparing: bool,
    pub prepare_text: String,
    pub error: Option<String>,
    pub port: Option<u16>,
    pub version: String,
    pub easytier: String,
    /// /state 的原始返回，前端自己取字段
    pub state: serde_json::Value,
    pub binary_installed: bool,
}

fn with<R>(f: impl FnOnce(&mut Inner) -> R) -> R {
    let mut guard = INNER.lock().unwrap();
    if guard.is_none() {
        *guard = Some(Inner {
            ready: false,
            port: None,
            version: VERSION.to_string(),
            easytier: String::new(),
            owns_process: false,
            child: None,
            error: None,
            prepare_text: String::new(),
            state: serde_json::Value::Null,
        });
    }
    f(guard.as_mut().unwrap())
}

pub fn snapshot() -> Snapshot {
    with(|s| Snapshot {
        ready: s.ready,
        preparing: PREPARING.load(Ordering::Relaxed),
        prepare_text: s.prepare_text.clone(),
        error: s.error.clone(),
        port: s.port,
        version: s.version.clone(),
        easytier: s.easytier.clone(),
        state: s.state.clone(),
        binary_installed: binary_path().is_file(),
    })
}

fn set_text(text: &str) {
    with(|s| s.prepare_text = text.to_string());
}

fn set_error(msg: Option<String>) {
    with(|s| {
        s.error = msg;
        s.prepare_text = String::new();
    });
}

/// 确保组件可用并把守护进程拉起来；多次调用无副作用。
pub async fn bootstrap() -> Result<Snapshot, String> {
    if PREPARING.swap(true, Ordering::SeqCst) {
        return Err("正在准备中，请稍候".into());
    }
    let result = bootstrap_inner().await;
    PREPARING.store(false, Ordering::SeqCst);
    match result {
        Ok(()) => Ok(snapshot()),
        Err(e) => {
            set_error(Some(e.clone()));
            Err(e)
        }
    }
}

async fn bootstrap_inner() -> Result<(), String> {
    with(|s| s.error = None);
    if !binary_path().is_file() {
        set_text("正在下载联机组件…");
        download_binary().await?;
    }
    set_text("正在启动联机组件…");
    start_daemon().await?;
    set_text("正在读取组件信息…");
    let meta = fetch_meta().await?;
    with(|s| {
        s.version = meta.0;
        s.easytier = meta.1;
        s.ready = true;
        s.prepare_text.clear();
    });
    let _ = refresh().await;
    Ok(())
}

async fn download_binary() -> Result<(), String> {
    std::fs::create_dir_all(support_dir()).map_err(|e| format!("创建目录失败：{e}"))?;
    let tmp = std::env::temp_dir().join(format!("glassmc-terracotta-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败：{e}"))?;
    let archive = tmp.join(archive_name());

    let urls = vec![
        format!(
            "https://github.com/burningtnt/Terracotta/releases/download/v{VERSION}/{}",
            archive_name()
        ),
        format!(
            "https://gitee.com/burningtnt/Terracotta/releases/download/v{VERSION}/{}",
            archive_name()
        ),
    ];
    if let Err(e) = download::download_any(&urls, &archive, None).await {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("下载联机组件失败：{e}"));
    }

    set_text("正在解压联机组件…");
    let extract_dir = tmp.join("x");
    std::fs::create_dir_all(&extract_dir).map_err(|e| format!("创建目录失败：{e}"))?;
    let archive_clone = archive.clone();
    let extract_clone = extract_dir.clone();
    let status = tokio::task::spawn_blocking(move || {
        Command::new("tar")
            .arg("-xzf")
            .arg(&archive_clone)
            .arg("-C")
            .arg(&extract_clone)
            .output()
    })
    .await
    .map_err(|e| format!("解压任务失败：{e}"))?;
    match status {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(format!(
                "解压联机组件失败：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(format!("调用 tar 失败：{e}"));
        }
    }

    // 在解压结果里找可执行文件（排除 .pkg 安装包）
    let found = find_binary(&extract_dir).ok_or_else(|| {
        let _ = std::fs::remove_dir_all(&tmp);
        "压缩包里没有找到可执行文件".to_string()
    })?;
    let dest = binary_path();
    let _ = std::fs::remove_file(&dest);
    std::fs::copy(&found, &dest).map_err(|e| format!("写入组件失败：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
        // 去掉隔离属性，否则 Gatekeeper 会拦住子进程
        let _ = Command::new("xattr").arg("-c").arg(&dest).output();
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

fn find_binary(dir: &std::path::Path) -> Option<PathBuf> {
    let mut fallback: Option<PathBuf> = None;
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.filter_map(|x| x.ok()) {
        let p = e.path();
        if p.is_dir() {
            if let Some(found) = find_binary(&p) {
                return Some(found);
            }
            continue;
        }
        let name = p.file_name()?.to_string_lossy().to_lowercase();
        if !name.starts_with("terracotta") {
            continue;
        }
        if name.ends_with(".pkg") || name.ends_with(".tar.gz") || name.ends_with(".zip") {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&p) {
                if meta.permissions().mode() & 0o111 != 0 {
                    return Some(p);
                }
            }
        }
        if fallback.is_none() {
            fallback = Some(p);
        }
    }
    fallback
}

fn read_lock_port() -> Option<u16> {
    let data = std::fs::read(lock_file()?).ok()?;
    if data.len() < 2 {
        return None;
    }
    let port = ((data[0] as u16) << 8) | data[1] as u16;
    if port > 0 {
        Some(port)
    } else {
        None
    }
}

async fn ping(port: u16) -> bool {
    net::local_client()
        .get(format!("http://127.0.0.1:{port}/state"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn start_daemon() -> Result<(), String> {
    // 已有实例在跑就复用
    if let Some(p) = read_lock_port() {
        if ping(p).await {
            with(|s| {
                s.port = Some(p);
                s.owns_process = false;
            });
            return Ok(());
        }
    }
    if let Some(lock) = lock_file() {
        let _ = std::fs::remove_file(lock);
    }

    let bin = binary_path();
    if !bin.is_file() {
        return Err("联机组件尚未下载".into());
    }
    std::fs::create_dir_all(support_dir()).ok();
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file())
        .ok();
    let mut cmd = Command::new(&bin);
    cmd.arg("--daemon")
        .current_dir(support_dir())
        .stdin(Stdio::null());
    if let Some(f) = log {
        let f2 = f.try_clone().ok();
        cmd.stdout(Stdio::from(f));
        if let Some(f2) = f2 {
            cmd.stderr(Stdio::from(f2));
        }
    }

    let child = cmd.spawn().map_err(|e| format!("无法启动联机组件：{e}"))?;
    with(|s| {
        s.child = Some(child);
        s.owns_process = true;
    });

    for _ in 0..60 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if let Some(p) = read_lock_port() {
            if ping(p).await {
                with(|s| s.port = Some(p));
                return Ok(());
            }
        }
    }
    Err("联机组件没能启动，请重试一次".into())
}

async fn fetch_meta() -> Result<(String, String), String> {
    let port = with(|s| s.port).ok_or("组件未启动")?;
    let resp = net::local_client()
        .get(format!("http://127.0.0.1:{port}/meta"))
        .send()
        .await
        .map_err(|e| format!("读取组件信息失败：{e}"))?;
    let v: serde_json::Value = resp.json().await.map_err(|e| format!("组件信息解析失败：{e}"))?;
    Ok((
        v.get("version")
            .and_then(|x| x.as_str())
            .unwrap_or(VERSION)
            .to_string(),
        v.get("easytier_version")
            .or_else(|| v.get("easytier"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
    ))
}

/// 拉一次 /state。
pub async fn refresh() -> Result<serde_json::Value, String> {
    let port = with(|s| s.port).ok_or("组件未启动")?;
    let resp = net::local_client()
        .get(format!("http://127.0.0.1:{port}/state"))
        .send()
        .await
        .map_err(|e| format!("无法连接本地组件：{e}"))?;
    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        with(|s| {
            s.ready = false;
            s.error = Some(format!("联机组件已退出（HTTP {code}），请重新启动"));
        });
        return Err(format!("联机组件响应异常（HTTP {code}）"));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| format!("状态解析失败：{e}"))?;
    with(|s| s.state = v.clone());
    Ok(v)
}

async fn command(path: &str) -> Result<u16, String> {
    let port = with(|s| s.port).ok_or("组件未启动")?;
    let resp = net::local_client()
        .get(format!("http://127.0.0.1:{port}{path}"))
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;
    Ok(resp.status().as_u16())
}

fn esc(s: &str) -> String {
    let mut out = String::new();
    for b in s.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 以房主身份创建房间（要求游戏已「对局域网开放」）。
pub async fn create_room(player: &str) -> Result<serde_json::Value, String> {
    with(|s| s.error = None);
    command(&format!("/state/scanning?player={}", esc(player))).await?;
    refresh().await
}

/// 加入别人的房间。
pub async fn join_room(room: &str, player: &str) -> Result<serde_json::Value, String> {
    with(|s| s.error = None);
    let code = room.trim().to_string();
    if !code.to_uppercase().starts_with("U/") {
        return Err("邀请码格式不正确，应形如 U/XXXX-XXXX-XXXX-XXXX".into());
    }
    let status = command(&format!(
        "/state/guesting?room={}&player={}",
        esc(&code),
        esc(player)
    ))
    .await?;
    if status == 400 {
        return Err("邀请码无效，或当前状态不允许加入".into());
    }
    refresh().await
}

/// 回到待机。
pub async fn leave() -> Result<serde_json::Value, String> {
    with(|s| s.error = None);
    command("/state/ide").await?;
    refresh().await
}

/// 退出组件（只关我们自己拉起来的那个）。
pub fn shutdown() {
    let (owns, port, mut child) = with(|s| {
        let owns = s.owns_process;
        let port = s.port;
        let child = s.child.take();
        s.ready = false;
        s.port = None;
        s.owns_process = false;
        (owns, port, child)
    });
    if !owns {
        return;
    }
    if let Some(p) = port {
        tauri::async_runtime::spawn(async move {
            let _ = net::local_client()
                .get(format!("http://127.0.0.1:{p}/panic?peaceful=true"))
                .send()
                .await;
        });
    }
    if let Some(c) = child.as_mut() {
        // 组件可能 fork 出自己的子进程，只 kill 父进程会留下孤儿，所以连子进程一起收掉。
        #[cfg(unix)]
        {
            let pid = c.id().to_string();
            let _ = std::process::Command::new("pkill")
                .args(["-TERM", "-P", &pid])
                .output();
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &pid])
                .output();
        }
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// 应用退出时务必调用。
pub fn kill_on_exit() {
    shutdown();
}

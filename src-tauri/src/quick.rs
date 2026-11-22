//! 快捷功能：固定世界/服务器、快速游玩的一步准备、Discord 状态（IPC）。
//!
//! 设计要点：
//! - 固定项存 store key `quick-pins`；待连接服务器存 `quick-pending`，由 launch.rs 消费。
//! - **不使用** game 参数 `demo` / `quickPlay`：快速加入靠 `extraGameArgs`（--server/--port），
//!   固定世界只负责启动对应版本，并提示玩家进游戏后点「上次世界」。
//! - Discord RPC 走 Discord 本地 IPC（Unix socket / Windows 命名管道），不引入新依赖；
//!   任何 IPC 失败都静默降级，绝不影响启动器本身。

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::store;

// ---------------------------------------------------------------- 数据结构

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    pub id: String,
    pub kind: String,
    pub version_id: String,
    pub name: String,
    #[serde(rename = "ref")]
    pub pin_ref: String,
    pub created_at: i64,
}

/// launch.rs 读取的契约：`data_root/feature-data/quick-pending.json`
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub address: String,
    pub port: u16,
    pub version_id: String,
    pub at: i64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct QuickPrepare {
    pub version_id: String,
    pub server: String,
    pub extra_game_args: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcConfig {
    pub enabled: bool,
    pub details: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcStatus {
    pub enabled: bool,
    pub connected: bool,
    pub last_error: String,
}

// ---------------------------------------------------------------- 固定项

#[tauri::command]
pub fn pins_list() -> Vec<Pin> {
    store::load("quick-pins")
}

#[tauri::command]
pub fn pin_add(kind: String, version_id: String, name: String, reference: String) -> Vec<Pin> {
    let mut list: Vec<Pin> = store::load("quick-pins");
    list.push(Pin {
        id: store::short_id(),
        kind,
        version_id,
        name,
        pin_ref: reference,
        created_at: store::now_secs(),
    });
    let _ = store::save("quick-pins", &list);
    list
}

#[tauri::command]
pub fn pin_remove(id: String) -> Vec<Pin> {
    let mut list: Vec<Pin> = store::load("quick-pins");
    list.retain(|p| p.id != id);
    let _ = store::save("quick-pins", &list);
    list
}

// ---------------------------------------------------------------- 快速游玩一步准备

/// 把 "host:port"（也支持 [ipv6]:port）拆成地址与端口，缺省端口 25565。
fn parse_server(server: &str) -> (String, u16) {
    let s = server.trim();
    if let Some(rest) = s.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let host = &rest[..end];
            let tail = &rest[end + 1..];
            let port = tail
                .strip_prefix(':')
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(25565);
            if !host.is_empty() {
                return (host.to_string(), port);
            }
        }
    }
    if let Some(idx) = s.rfind(':') {
        let host = &s[..idx];
        let port = s[idx + 1..].parse::<u16>().unwrap_or(25565);
        if !host.is_empty() {
            return (host.to_string(), port);
        }
    }
    (s.to_string(), 25565)
}

/// 写入 `quick-pending` 供 launch.rs 追加真实启动参数；返回给界面展示的额外参数。
#[tauri::command]
pub fn quick_prepare(version_id: String, server: String) -> QuickPrepare {
    let (address, port) = parse_server(&server);
    let pending = Pending {
        address: address.clone(),
        port,
        version_id: version_id.clone(),
        at: store::now_secs(),
    };
    let _ = store::save("quick-pending", &pending);
    QuickPrepare {
        version_id,
        server,
        extra_game_args: vec![
            "--server".to_string(),
            address,
            "--port".to_string(),
            port.to_string(),
        ],
    }
}

// ---------------------------------------------------------------- Discord IPC

const CLIENT_ID: &str = "123456789012345678";
const RESPEND_SECS: u64 = 20;

static RPC_TASK: OnceLock<()> = OnceLock::new();
static LAST_ACTIVITY: Mutex<Option<(String, String)>> = Mutex::new(None);
static RPC_CONNECTED: AtomicBool = AtomicBool::new(false);
static RPC_LAST_ERROR: Mutex<String> = Mutex::new(String::new());

trait IpcConn: Read + Write {}
#[cfg(unix)]
impl IpcConn for std::os::unix::net::UnixStream {}
#[cfg(windows)]
impl IpcConn for std::fs::File {}

#[cfg(unix)]
fn default_ipc_path(i: u32) -> String {
    let base = dirs::home_dir()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|| "/tmp".to_string());
    format!("{base}/.discord/ipc/discord-ipc-{i}")
}

fn connect_ipc() -> Result<Box<dyn IpcConn>, String> {
    #[cfg(unix)]
    {
        for i in 0..10u32 {
            let path = if i == 0 {
                std::env::var("DISCORD_IPC_PATH").unwrap_or_else(|_| default_ipc_path(0))
            } else {
                default_ipc_path(i)
            };
            if let Ok(s) = std::os::unix::net::UnixStream::connect(&path) {
                let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(500)));
                let _ = s.set_write_timeout(Some(std::time::Duration::from_millis(500)));
                return Ok(Box::new(s));
            }
        }
        Err("discord ipc not found".to_string())
    }
    #[cfg(windows)]
    {
        for i in 0..10u32 {
            let path = format!(r"\\.\pipe\discord-ipc-{i}");
            if let Ok(f) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
            {
                return Ok(Box::new(f));
            }
        }
        Err("discord ipc not found".to_string())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("unsupported platform".to_string())
    }
}

fn write_frame(conn: &mut dyn IpcConn, opcode: u32, body: &str) -> Result<(), String> {
    let mut buf = Vec::with_capacity(8 + body.len());
    buf.extend_from_slice(&opcode.to_le_bytes());
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(body.as_bytes());
    conn.write_all(&buf).map_err(|e| e.to_string())
}

#[cfg(unix)]
fn read_frame(conn: &mut dyn IpcConn) -> Result<String, String> {
    let mut header = [0u8; 8];
    conn.read_exact(&mut header).map_err(|e| e.to_string())?;
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if len > 1_048_576 {
        return Err("frame too large".to_string());
    }
    let mut body = vec![0u8; len];
    conn.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

fn send_activity_inner(details: &str, state: &str) -> Result<(), String> {
    let mut conn = connect_ipc()?;
    let hello =
        serde_json::json!({"v": 1, "client_id": CLIENT_ID, "cmd": "DISCORD_READY"}).to_string();
    write_frame(conn.as_mut(), 0, &hello)?;
    // 握手回包忽略即可；Unix 上已设置 500ms 读超时，不会卡住启动器。
    #[cfg(unix)]
    let _ = read_frame(conn.as_mut());

    let payload = serde_json::json!({
        "cmd": "SET_ACTIVITY",
        "args": {
            "pid": std::process::id(),
            "activity": {
                "details": details,
                "state": state,
                "assets": { "large_text": "GlassMC" },
                "timestamps": { "start": store::now_secs() }
            }
        },
        "nonce": store::short_id()
    })
    .to_string();
    write_frame(conn.as_mut(), 1, &payload)?;
    Ok(())
}

/// 真实 IPC 在独立线程里做，外层加超时：Discord 不在线也最多等 ~1.5 秒。
fn send_activity(details: &str, state: &str) -> Result<(), String> {
    let d = details.to_string();
    let s = state.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = send_activity_inner(&d, &s);
        let _ = tx.send(r);
    });
    match rx.recv_timeout(std::time::Duration::from_millis(1500)) {
        Ok(r) => r,
        Err(_) => Err("timeout".to_string()),
    }
}

fn record_result(ok: bool, err: String) {
    RPC_CONNECTED.store(ok, Ordering::Relaxed);
    if let Ok(mut slot) = RPC_LAST_ERROR.lock() {
        *slot = err;
    }
}

fn last_activity() -> (String, String) {
    LAST_ACTIVITY
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(|| ("使用 GlassMC 启动器".to_string(), "GlassMC".to_string()))
}

/// 用 OnceLock 保证后台重发任务只起一次；enabled 时才真正发。
pub fn ensure_rpc_task() {
    RPC_TASK.get_or_init(|| {
        let handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h,
            Err(_) => return,
        };
        handle.spawn(async {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(RESPEND_SECS));
            loop {
                ticker.tick().await;
                let cfg: RpcConfig = store::load("rpc-config");
                if !cfg.enabled {
                    continue;
                }
                let act = last_activity();
                match tokio::task::spawn_blocking(move || send_activity(&act.0, &act.1)).await {
                    Ok(Ok(())) => record_result(true, String::new()),
                    Ok(Err(e)) => record_result(false, e),
                    Err(_) => record_result(false, "task failed".to_string()),
                }
            }
        });
    });
}

// ---------------------------------------------------------------- 命令

#[tauri::command]
pub fn rpc_config(enabled: bool, details: String) -> RpcConfig {
    let cfg = RpcConfig { enabled, details };
    let _ = store::save("rpc-config", &cfg);
    if enabled {
        let mut slot = LAST_ACTIVITY.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some((cfg.details.clone(), "GlassMC".to_string()));
        }
    }
    ensure_rpc_task();
    cfg
}

#[tauri::command]
pub fn rpc_status() -> RpcStatus {
    ensure_rpc_task();
    let cfg: RpcConfig = store::load("rpc-config");
    RpcStatus {
        enabled: cfg.enabled,
        connected: RPC_CONNECTED.load(Ordering::Relaxed),
        last_error: RPC_LAST_ERROR
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone(),
    }
}

#[tauri::command]
pub fn rpc_send_activity(details: String, state: String) -> RpcStatus {
    {
        let mut slot = LAST_ACTIVITY.lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some((details.clone(), state.clone()));
    }
    match send_activity(&details, &state) {
        Ok(()) => record_result(true, String::new()),
        Err(e) => record_result(false, e),
    }
    rpc_status()
}

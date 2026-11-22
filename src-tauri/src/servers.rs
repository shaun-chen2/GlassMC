//! 联机功能：Java 版服务器列表 / 延迟探测、公共服务器浏览器、FRP 内网穿透客户端。
//!
//! 设计要点：
//! - 所有数据都进 `store`（独立 JSON），不碰 config.rs；
//! - Server List Ping 用 tokio TCP + 2 秒超时，失败返回 `ok:false`，不抛异常；
//! - FRP 二进制只在用户显式点「下载」时才下载，下载后也不会自动启动；
//! - 平台包格式不同：Windows 是 zip（走 store::unzip_to），macOS/Linux 是 tar.gz
//!   （用 flate2 解压 + 内置极简 tar 读取，避免新增 tar 依赖）。

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::{commands, config, download, game, java, launch, net, store};

// ============================================================================
// 数据结构
// ============================================================================

const STORE_SERVERS: &str = "servers";
const STORE_FRP: &str = "frp-config";
const PROTOCOL_VERSION: i32 = 765; // 1.21.x 的状态协议号，仅用于 SLP
const PING_TIMEOUT: Duration = Duration::from_secs(2);
const FRP_VERSION: &str = "0.61.1";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerEntry {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub pinned: bool,
    pub icon: Option<String>,
    pub last_ping_ms: Option<i64>,
    pub players_online: Option<i64>,
    pub motd: Option<String>,
    pub updated_at: i64,
}

/// 单次 ping 的结果。失败时 `ok=false` 且 `error` 有简短原因，不抛异常。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub ok: bool,
    pub address: String,
    pub port: u16,
    pub latency_ms: Option<i64>,
    pub players_online: Option<i64>,
    pub players_max: Option<i64>,
    pub motd: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrowserServer {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub tags: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrowserList {
    pub notice: String,
    pub items: Vec<BrowserServer>,
}

/// mcsrvstat.us 的可选补充信息；离线或解析失败时命令返回 `None`，前端忽略。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInfo {
    pub description: String,
    pub players_online: i64,
    pub players_max: i64,
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FrpConfig {
    pub server_addr: String,
    pub server_port: u16,
    pub token: String,
    pub remote_port: u16,
    pub local_port: u16,
    pub protocol: String,
}

impl Default for FrpConfig {
    fn default() -> Self {
        Self {
            server_addr: String::new(),
            server_port: 7000,
            token: String::new(),
            remote_port: 25565,
            local_port: 25565,
            protocol: "tcp".to_string(),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FrpStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub binary_installed: bool,
    pub config_exists: bool,
    pub log_exists: bool,
    pub binary_path: String,
}

// FRP 子进程只在本模块内管理，进程退出/异常时由 status/start 兜底清理。
static FRP_CHILD: Mutex<Option<Child>> = Mutex::new(None);

// ============================================================================
// 我的服务器
// ============================================================================

fn load_servers() -> Vec<ServerEntry> {
    store::load::<Vec<ServerEntry>>(STORE_SERVERS)
}

fn persist(list: &[ServerEntry]) -> Vec<ServerEntry> {
    if let Err(e) = store::save(STORE_SERVERS, &list.to_vec()) {
        eprintln!("[servers] 保存失败：{e}");
    }
    list.to_vec()
}

/// 置顶的排前面，其余按名字排，方便界面展示。
fn ordered(mut list: Vec<ServerEntry>) -> Vec<ServerEntry> {
    list.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    list
}

/// 地址支持 `ip:port` / `[ipv6]:port` 简写；没写端口时用传入的默认值。
fn split_host_port(address: &str, default_port: u16) -> (String, u16) {
    let s = address.trim();
    if s.starts_with('[') {
        if let Some(end) = s.find(']') {
            let host = s[1..end].to_string();
            let rest = &s[end + 1..];
            if let Some(p) = rest.strip_prefix(':') {
                if let Ok(np) = p.trim().parse::<u16>() {
                    return (host, np);
                }
            }
            return (host, default_port);
        }
    }
    if let Some((host, port)) = s.rsplit_once(':') {
        if !host.is_empty() {
            if let Ok(np) = port.trim().parse::<u16>() {
                return (host.trim().to_string(), np);
            }
        }
    }
    (s.to_string(), default_port)
}

#[tauri::command]
pub fn servers_list() -> Vec<ServerEntry> {
    ordered(load_servers())
}

#[tauri::command]
pub fn servers_add(name: String, address: String, port: u16) -> Result<Vec<ServerEntry>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("名称不能为空".into());
    }
    let (host, p) = split_host_port(&address, if port == 0 { 25565 } else { port });
    if host.is_empty() {
        return Err("地址不能为空".into());
    }
    let mut list = load_servers();
    // 同地址同端口不重复添加
    if list.iter().any(|e| e.address == host && e.port == p) {
        return Ok(ordered(list));
    }
    list.push(ServerEntry {
        id: store::short_id(),
        name: name.to_string(),
        address: host,
        port: p,
        pinned: false,
        icon: None,
        last_ping_ms: None,
        players_online: None,
        motd: None,
        updated_at: store::now_secs(),
    });
    Ok(ordered(persist(&list)))
}

#[tauri::command]
pub fn servers_remove(id: String) -> Vec<ServerEntry> {
    let list: Vec<ServerEntry> = load_servers().into_iter().filter(|e| e.id != id).collect();
    ordered(persist(&list))
}

#[tauri::command]
pub fn servers_pin(id: String, pinned: bool) -> Vec<ServerEntry> {
    let mut list = load_servers();
    if let Some(e) = list.iter_mut().find(|e| e.id == id) {
        e.pinned = pinned;
    }
    ordered(persist(&list))
}

#[tauri::command]
pub async fn servers_ping(address: String, port: u16) -> PingResult {
    ping_server(
        address.trim().to_string(),
        if port == 0 { 25565 } else { port },
    )
    .await
}

#[tauri::command]
pub async fn servers_ping_all() -> Vec<ServerEntry> {
    let mut list = load_servers();
    if list.is_empty() {
        return list;
    }
    // 并发探测，保留原顺序；单台失败只留空延迟，不影响别的服务器。
    let mut tasks = Vec::with_capacity(list.len());
    for e in &list {
        tasks.push(tokio::spawn(ping_server(e.address.clone(), e.port)));
    }
    for (i, task) in tasks.into_iter().enumerate() {
        if let Ok(res) = task.await {
            if let Some(slot) = list.get_mut(i) {
                slot.last_ping_ms = res.latency_ms;
                slot.players_online = res.players_online;
                slot.motd = res.motd;
                slot.updated_at = store::now_secs();
            }
        }
    }
    ordered(persist(&list))
}

/// 「加入」：用当前选中的游戏版本 + 服务器参数启动游戏。
/// 没启动过任何版本时返回友好错误，前端会回退成「复制地址」。
#[tauri::command]
pub async fn servers_join(
    app: tauri::AppHandle,
    address: String,
    port: u16,
) -> Result<u32, String> {
    if game::is_running() {
        return Err("已经有一个游戏在运行了".into());
    }
    let cfg = config::load();
    let version_id = cfg.last_version.trim().to_string();
    if version_id.is_empty() {
        return Err("请先在首页启动过一个游戏版本".into());
    }
    let acc = commands::resolve_account(&cfg);
    let required = launch::required_java(&version_id, &cfg).await;
    let java = java::best_for(required, &cfg.java_path)
        .ok_or_else(|| "没有找到可用的 Java，请先在设置里指定".to_string())?;
    let mut built = launch::build_command(&version_id, &acc, &java, &cfg).await?;
    built.args.push("--server".to_string());
    built.args.push(address.trim().to_string());
    built.args.push("--port".to_string());
    built.args.push(port.to_string());
    game::spawn(app, &built)
}

// ============================================================================
// Server List Ping（Java 版状态协议）
// ============================================================================

fn write_varint(mut value: i32, out: &mut Vec<u8>) {
    loop {
        let mut b = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            b |= 0x80;
        }
        out.push(b);
        if value == 0 {
            break;
        }
    }
}

async fn read_varint_async<R: AsyncRead + Unpin>(r: &mut R) -> Result<i32, String> {
    let mut num = 0i32;
    let mut shift = 0u32;
    for _ in 0..5 {
        let mut b = [0u8; 1];
        r.read_exact(&mut b).await.map_err(|e| e.to_string())?;
        num |= ((b[0] & 0x7F) as i32) << shift;
        if b[0] & 0x80 == 0 {
            return Ok(num);
        }
        shift += 7;
    }
    Err("VarInt 过长".into())
}

fn empty_ping(address: &str, port: u16) -> PingResult {
    PingResult {
        ok: false,
        address: address.to_string(),
        port,
        latency_ms: None,
        players_online: None,
        players_max: None,
        motd: None,
        error: None,
    }
}

fn description_text(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Object(o)) => {
            let mut out = o
                .get("text")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(extra) = o.get("extra").and_then(|x| x.as_array()) {
                for e in extra {
                    if let Some(s) = e.as_str() {
                        out.push_str(s);
                    } else if let Some(t) = e.get("text").and_then(|x| x.as_str()) {
                        out.push_str(t);
                    }
                }
            }
            out
        }
        _ => String::new(),
    }
}

async fn ping_server(address: String, port: u16) -> PingResult {
    let start = Instant::now();
    // 接收 owned String，便于 tokio::spawn 并发探测时满足 'static
    let host = address.trim();
    if host.is_empty() {
        let mut r = empty_ping(host, port);
        r.error = Some("地址不能为空".into());
        return r;
    }
    let target = format!("{}:{}", host, port);
    let stream = match tokio::time::timeout(PING_TIMEOUT, TcpStream::connect(&target)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let mut r = empty_ping(host, port);
            r.error = Some(format!("无法连接：{e}"));
            return r;
        }
        Err(_) => {
            let mut r = empty_ping(host, port);
            r.error = Some("连接超时".into());
            return r;
        }
    };

    // 握手包：包长 + id 0x00 + 协议号 + 主机字符串 + 端口(u16 BE) + nextState=1
    let mut body = Vec::new();
    write_varint(0, &mut body);
    write_varint(PROTOCOL_VERSION, &mut body);
    let hb = host.as_bytes();
    write_varint(hb.len() as i32, &mut body);
    body.extend_from_slice(hb);
    body.extend_from_slice(&port.to_be_bytes());
    write_varint(1, &mut body);
    let mut packet = Vec::new();
    write_varint(body.len() as i32, &mut packet);
    packet.extend_from_slice(&body);
    // 紧跟着发一个空的 Status Request 包
    packet.push(1);
    packet.push(0);

    let (mut rd, mut wr) = stream.into_split();
    if let Err(e) = wr.write_all(&packet).await {
        let mut r = empty_ping(host, port);
        r.error = Some(format!("发送失败：{e}"));
        return r;
    }

    let response = tokio::time::timeout(PING_TIMEOUT, async {
        let _len = read_varint_async(&mut rd).await?;
        let _id = read_varint_async(&mut rd).await?;
        let n = read_varint_async(&mut rd).await?;
        if n <= 0 || n > 1_048_576 {
            return Err("响应长度异常".to_string());
        }
        let mut buf = vec![0u8; n as usize];
        rd.read_exact(&mut buf).await.map_err(|e| e.to_string())?;
        String::from_utf8(buf).map_err(|e| e.to_string())
    })
    .await;

    let json = match response {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let mut r = empty_ping(host, port);
            r.error = Some(e);
            return r;
        }
        Err(_) => {
            let mut r = empty_ping(host, port);
            r.error = Some("读取响应超时".into());
            return r;
        }
    };

    let v: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => {
            let mut r = empty_ping(host, port);
            r.error = Some("服务器返回了无法解析的数据".into());
            return r;
        }
    };

    let mut r = empty_ping(host, port);
    r.ok = true;
    r.latency_ms = Some(start.elapsed().as_millis() as i64);
    r.motd = Some(description_text(v.get("description")));
    if let Some(p) = v.get("players") {
        r.players_online = p.get("online").and_then(json_i64);
        r.players_max = p.get("max").and_then(json_i64);
    }
    r
}

fn json_i64(v: &serde_json::Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_u64().map(|u| u as i64))
}

// ============================================================================
// 公共服务器浏览器（内置示例清单，不编造外部 API）
// ============================================================================

#[tauri::command]
pub fn server_browser_list() -> BrowserList {
    let items = vec![
        BrowserServer {
            name: "Hypixel".into(),
            address: "hypixel.net".into(),
            port: 25565,
            tags: vec!["Minigames".into(), "EN".into()],
        },
        BrowserServer {
            name: "CubeCraft".into(),
            address: "play.cubecraft.net".into(),
            port: 25565,
            tags: vec!["Minigames".into(), "EN".into()],
        },
        BrowserServer {
            name: "2b2t".into(),
            address: "2b2t.org".into(),
            port: 25565,
            tags: vec!["Anarchy".into()],
        },
        BrowserServer {
            name: "Pika Network".into(),
            address: "play.pika-network.net".into(),
            port: 25565,
            tags: vec!["Minigames".into()],
        },
        BrowserServer {
            name: "Purple Prison".into(),
            address: "play.purpleprison.org".into(),
            port: 25565,
            tags: vec!["Prison".into(), "EN".into()],
        },
        BrowserServer {
            name: "GraniteMC".into(),
            address: "play.granitemc.net".into(),
            port: 25565,
            tags: vec!["Survival".into(), "EN".into()],
        },
        BrowserServer {
            name: "Mineplex".into(),
            address: "us.mineplex.com".into(),
            port: 25565,
            tags: vec!["Minigames".into(), "EN".into()],
        },
        BrowserServer {
            name: "Pixelmon Realms".into(),
            address: "play.pixelmonrealms.com".into(),
            port: 25565,
            tags: vec!["Pixelmon".into(), "Modded".into()],
        },
    ];
    BrowserList {
        notice: "示例清单，连通性取决于网络；延迟为实时探测结果。".into(),
        items,
    }
}

fn pct_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 可选补充：mcsrvstat.us 的描述/人数。失败一律返回 None，由前端忽略。
#[tauri::command]
pub async fn server_browser_info(host: String) -> Option<BrowserInfo> {
    let h = host.trim();
    if h.is_empty() {
        return None;
    }
    let url = format!("https://api.mcsrvstat.us/3/{}", pct_encode(h));
    let fut = net::get_json(&url);
    let v = match tokio::time::timeout(Duration::from_secs(5), fut).await {
        Ok(Ok(v)) => v,
        _ => return None,
    };
    if !v.get("online").and_then(|x| x.as_bool()).unwrap_or(false) {
        return None;
    }
    let description = v
        .get("motd")
        .and_then(|m| m.get("clean"))
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    Some(BrowserInfo {
        description,
        players_online: v
            .get("players")
            .and_then(|p| p.get("online"))
            .and_then(json_i64)
            .unwrap_or(0),
        players_max: v
            .get("players")
            .and_then(|p| p.get("max"))
            .and_then(json_i64)
            .unwrap_or(0),
        version: v
            .get("version")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

// ============================================================================
// FRP 客户端
// ============================================================================

fn frp_dir() -> PathBuf {
    store::files_dir("frp")
}

fn frp_bin_path() -> PathBuf {
    let name = if cfg!(target_os = "windows") {
        "frpc.exe"
    } else {
        "frpc"
    };
    frp_dir().join(name)
}

fn frp_log_path() -> PathBuf {
    frp_dir().join("frp.log")
}

fn frp_toml_path() -> PathBuf {
    frp_dir().join("frpc.toml")
}

/// 返回 (发行包名, 是否为 zip)。macOS/Linux 是 tar.gz，Windows 是 zip。
fn frp_archive() -> (String, bool) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let is_zip = cfg!(target_os = "windows");
    let ext = if is_zip { "zip" } else { "tar.gz" };
    (format!("frp_{FRP_VERSION}_{os}_{arch}.{ext}"), is_zip)
}

fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn build_frp_toml(cfg: &FrpConfig) -> String {
    let token = cfg.token.trim();
    let auth = if token.is_empty() {
        String::new()
    } else {
        format!("auth.token = \"{}\"\n", toml_escape(token))
    };
    let kind = if cfg.protocol.trim().eq_ignore_ascii_case("udp") {
        "udp"
    } else {
        "tcp"
    };
    format!(
        "serverAddr = \"{}\"\nserverPort = {}\n{}[[proxies]]\nname = \"glassmc\"\ntype = \"{}\"\nlocalIP = \"127.0.0.1\"\nlocalPort = {}\nremotePort = {}\n",
        toml_escape(cfg.server_addr.trim()),
        cfg.server_port,
        auth,
        kind,
        cfg.local_port,
        cfg.remote_port,
    )
}

/// 极小 tar 读取器：只取普通文件，跳过 PAX/GNU 扩展头与目录，拒绝路径穿越。
fn extract_tar_gz(archive: &Path, dst: &Path) -> Result<usize, String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("打开压缩包失败：{e}"))?;
    let mut gz = flate2::read::GzDecoder::new(file);
    let mut data = Vec::new();
    gz.read_to_end(&mut data)
        .map_err(|e| format!("gzip 解压失败：{e}"))?;
    std::fs::create_dir_all(dst).map_err(|e| format!("创建目录失败：{e}"))?;

    let mut pos = 0usize;
    let mut count = 0usize;
    let mut long_name: Option<String> = None;
    while pos + 512 <= data.len() {
        let header = &data[pos..pos + 512];
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let name = tar_cstr(&header[0..100]);
        let prefix = tar_cstr(&header[345..500]);
        let size = tar_octal(&header[124..136]) as usize;
        let kind = header[156];
        pos += 512;
        if pos + size > data.len() {
            break;
        }
        let content = &data[pos..pos + size];
        let padded = size.div_ceil(512) * 512;

        if kind == b'L' {
            long_name = Some(
                String::from_utf8_lossy(content)
                    .trim_matches('\0')
                    .to_string(),
            );
            pos += padded;
            continue;
        }
        if kind == b'x' || kind == b'g' || kind == b'5' {
            pos += padded;
            continue;
        }
        if kind == b'0' || kind == 0 {
            let rel = long_name.take().unwrap_or_else(|| {
                if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                }
            });
            let rel = rel.trim_start_matches("./").to_string();
            if !rel.is_empty() && !rel.starts_with('/') && !rel.contains("..") && !rel.contains(':')
            {
                let target = dst.join(&rel);
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::write(&target, content).is_ok() {
                    count += 1;
                }
            }
        }
        pos += padded;
    }
    Ok(count)
}

fn tar_cstr(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn tar_octal(bytes: &[u8]) -> u64 {
    let s = String::from_utf8_lossy(bytes);
    let s = s.trim_matches(|c| c == '\0' || c == ' ');
    u64::from_str_radix(s, 8).unwrap_or(0)
}

fn find_file(dir: &Path, target: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_file(&p, target) {
                return Some(found);
            }
        } else if p.file_name().map(|n| n == target).unwrap_or(false) {
            return Some(p);
        }
    }
    None
}

/// 仅当用户显式调用时才下载 FRP，下载完也不会自动启动。
#[tauri::command]
pub async fn frp_download() -> Result<String, String> {
    let (release_name, is_zip) = frp_archive();
    let url =
        format!("https://github.com/fatedier/frp/releases/download/v{FRP_VERSION}/{release_name}");
    let dir = frp_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败：{e}"))?;
    let local_name = if is_zip { "frp.zip" } else { "frp.tar.gz" };
    let archive = dir.join(local_name);

    download::download(&url, &archive, None)
        .await
        .map_err(|e| format!("下载 FRP 失败：{e}"))?;

    let extract = dir.join("unpacked");
    let _ = std::fs::remove_dir_all(&extract);
    if is_zip {
        store::unzip_to(&archive, &extract)?;
    } else {
        extract_tar_gz(&archive, &extract)?;
    }

    let exe_name = if cfg!(target_os = "windows") {
        "frpc.exe"
    } else {
        "frpc"
    };
    let found = find_file(&extract, exe_name)
        .ok_or_else(|| "压缩包里没有找到 frpc 可执行文件".to_string())?;
    let dest = frp_bin_path();
    let _ = std::fs::remove_file(&dest);
    std::fs::copy(&found, &dest).map_err(|e| format!("写入 frpc 失败：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::remove_dir_all(&extract);
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub fn frp_config_get() -> FrpConfig {
    store::load::<FrpConfig>(STORE_FRP)
}

#[tauri::command]
pub fn frp_config_save(cfg: FrpConfig) -> Result<FrpConfig, String> {
    store::save(STORE_FRP, &cfg)?;
    Ok(cfg)
}

#[tauri::command]
pub fn frp_status() -> FrpStatus {
    let mut running = false;
    let mut pid = None;
    let mut died = false;
    {
        let mut guard = FRP_CHILD.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            if matches!(child.try_wait(), Ok(Some(_))) {
                died = true;
            } else {
                running = true;
                pid = Some(child.id());
            }
        }
    }
    if died {
        let _ = FRP_CHILD.lock().unwrap().take();
    }
    FrpStatus {
        running,
        pid,
        binary_installed: frp_bin_path().is_file(),
        config_exists: store::json_path(STORE_FRP).is_file(),
        log_exists: frp_log_path().is_file(),
        binary_path: frp_bin_path().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn frp_start() -> Result<u32, String> {
    {
        let mut guard = FRP_CHILD.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            // 还活着就不重复启动；已退出的顺手清掉
            if !matches!(child.try_wait(), Ok(Some(_))) {
                return Err("FRP 已在运行".into());
            }
            *guard = None;
        }
    }
    let bin = frp_bin_path();
    if !bin.is_file() {
        return Err("尚未下载 frpc，请先点「下载 FRP」".into());
    }
    let cfg = store::load::<FrpConfig>(STORE_FRP);
    if cfg.server_addr.trim().is_empty() {
        return Err("请先填写 FRP 服务器地址并保存配置".into());
    }
    let dir = frp_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败：{e}"))?;
    std::fs::write(frp_toml_path(), build_frp_toml(&cfg))
        .map_err(|e| format!("写入配置失败：{e}"))?;

    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(frp_log_path())
        .map_err(|e| format!("打开日志失败：{e}"))?;
    let err = log
        .try_clone()
        .map_err(|e| format!("复制日志句柄失败：{e}"))?;

    let mut cmd = Command::new(&bin);
    cmd.arg("-c")
        .arg(frp_toml_path())
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err));
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW：避免弹出黑色控制台窗口
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let child = cmd.spawn().map_err(|e| format!("启动 frpc 失败：{e}"))?;
    let pid = child.id();
    *FRP_CHILD.lock().unwrap() = Some(child);
    Ok(pid)
}

#[tauri::command]
pub fn frp_stop() -> Result<bool, String> {
    let mut guard = FRP_CHILD.lock().unwrap();
    if let Some(child) = guard.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
        *guard = None;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 读日志末尾 100 行；文件不存在时返回空串。
#[tauri::command]
pub fn frp_log() -> String {
    match std::fs::read_to_string(frp_log_path()) {
        Ok(s) => s
            .lines()
            .rev()
            .take(100)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n"),
        Err(_) => String::new(),
    }
}

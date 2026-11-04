//! 设置持久化：一个 JSON 文件，字段全部带默认值，坏文件不会导致启动失败。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::paths;

/// 一个 Yggdrasil 兼容的认证服务器（外置登录）。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AuthServer {
    pub id: String,
    /// 展示名称，如 "LittleSkin"
    pub name: String,
    /// API 基址，如 "https://littleskin.cn/api/yggdrasil"
    pub base_url: String,
    /// 内置预设不可删除
    pub is_preset: bool,
}

impl AuthServer {
    pub fn new(name: &str, base_url: &str, preset: bool) -> Self {
        Self {
            id: format!("srv-{}", slug(base_url)),
            name: name.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            is_preset: preset,
        }
    }

    pub fn normalized_base(&self) -> String {
        self.base_url.trim().trim_end_matches('/').to_string()
    }
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// 不同皮肤站的认证路径不同，`yggdrasil.rs` 会自动探测；
/// 这里只保证基址是「API 根」，不带 `/authserver`。
pub fn preset_servers() -> Vec<AuthServer> {
    vec![
        AuthServer::new("LittleSkin", "https://littleskin.cn/api/yggdrasil", true),
        AuthServer::new("MiniSkins", "https://www.mcpeau.com/api/yggdrasil", true),
        AuthServer::new("Ely.by", "https://authserver.ely.by", true),
    ]
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// 窗口玻璃不透明度 0.35 ~ 1.0
    pub glass_opacity: f64,
    /// system / light / dark
    pub theme: String,
    /// 游戏文件下载源
    pub download_base: String,
    /// Java 运行环境下载源
    pub java_mirror: String,
    /// 分配给游戏的堆内存（GB）
    pub memory_gb: f64,
    /// 游戏窗口尺寸，0 = 不干预
    pub window_width: u32,
    pub window_height: u32,
    /// 追加的 JVM 参数（空格分隔）
    pub extra_jvm_args: String,
    /// 上次选择的版本
    pub last_version: String,
    /// 手动指定的 Java 可执行文件（空 = 自动挑选）
    pub java_path: String,
    /// 当前使用的账号 id（对应 accounts.json 的 id）
    pub active_account: String,
    /// 离线账号名（没有任何账号时使用）
    pub offline_name: String,
    /// 关闭窗口时退出应用
    pub quit_on_close: bool,
    /// 外置登录服务器列表
    pub auth_servers: Vec<AuthServer>,
    /// 皮肤库站点（Blessing Skin）
    pub skin_library_base: String,
    /// 微软登录用的 Azure 公共客户端 id
    pub ms_client_id: String,
    /// 陶瓦联机组件版本（空 = 用内置默认）
    pub terracotta_version: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            glass_opacity: 1.0,
            theme: "system".into(),
            // BMCLAPI 是 HMCL 默认使用的国内镜像
            download_base: "https://bmclapi2.bangbang93.com".into(),
            java_mirror: "https://mirrors.tuna.tsinghua.edu.cn/Adoptium".into(),
            memory_gb: 2.0,
            window_width: 0,
            window_height: 0,
            extra_jvm_args: String::new(),
            last_version: String::new(),
            java_path: String::new(),
            active_account: String::new(),
            offline_name: "Steve".into(),
            quit_on_close: true,
            auth_servers: preset_servers(),
            skin_library_base: "https://www.mcpeau.com".into(),
            ms_client_id: String::new(),
            terracotta_version: String::new(),
        }
    }
}

impl Config {
    pub fn download_base(&self) -> String {
        self.download_base.trim_end_matches('/').to_string()
    }
}

pub struct ConfigState(pub Mutex<Config>);

pub fn load() -> Config {
    let p = paths::paths().config_file.clone();
    let mut cfg = match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Config::default(),
    };
    // 老配置文件里没有这些字段时，`#[serde(default)]` 会给出空值而不是预设
    if cfg.auth_servers.is_empty() {
        cfg.auth_servers = preset_servers();
    }
    if cfg.skin_library_base.trim().is_empty() {
        cfg.skin_library_base = "https://www.mcpeau.com".into();
    }
    if cfg.download_base.trim().is_empty() {
        cfg.download_base = "https://bmclapi2.bangbang93.com".into();
    }
    if cfg.java_mirror.trim().is_empty() {
        cfg.java_mirror = "https://mirrors.tuna.tsinghua.edu.cn/Adoptium".into();
    }
    cfg
}

pub fn save(cfg: &Config) -> Result<(), String> {
    let p = paths::paths().config_file.clone();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&p, s).map_err(|e| format!("写入设置失败：{e}"))
}

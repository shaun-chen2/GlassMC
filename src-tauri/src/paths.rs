//! 全局目录约定。
//!
//! 三端都用各平台「应用数据」目录下的 `GlassMC`，与 macOS 原生版保持同一位置，
//! 因此可以直接读到既有版本、账号与设置：
//!   macOS   ~/Library/Application Support/GlassMC
//!   Windows %APPDATA%\GlassMC
//!   Linux   ~/.local/share/GlassMC
//!
//! 环境变量 `GLASSMC_DATA` 可整体覆盖（便携模式 / 测试用）。

use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct Paths {
    pub data_root: PathBuf,
    /// 等价于官方启动器的 .minecraft
    pub game_dir: PathBuf,
    pub java_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub natives_dir: PathBuf,
    pub accounts_file: PathBuf,
    pub config_file: PathBuf,
    pub log_dir: PathBuf,
    /// 皮肤缓存目录
    pub skins_dir: PathBuf,
}

pub fn paths() -> &'static Paths {
    static P: OnceLock<Paths> = OnceLock::new();
    P.get_or_init(|| {
        let data_root = match std::env::var("GLASSMC_DATA") {
            Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
            _ => dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("GlassMC"),
        };
        let game_dir = data_root.join("minecraft");
        Paths {
            java_dir: data_root.join("java"),
            versions_dir: game_dir.join("versions"),
            libraries_dir: game_dir.join("libraries"),
            assets_dir: game_dir.join("assets"),
            natives_dir: game_dir.join("natives"),
            accounts_file: data_root.join("accounts.json"),
            // 刻意不与原生版共用文件名，两边同时装也不会互相踩掉设置
            config_file: data_root.join("tauri-config.json"),
            log_dir: data_root.join("logs"),
            skins_dir: data_root.join("skins"),
            game_dir,
            data_root,
        }
    })
}

/// 确保基础目录存在；失败不致命（后续写入时会再报错）。
pub fn ensure_dirs() {
    let p = paths();
    for dir in [
        &p.data_root,
        &p.game_dir,
        &p.java_dir,
        &p.versions_dir,
        &p.libraries_dir,
        &p.assets_dir,
        &p.natives_dir,
        &p.log_dir,
        &p.skins_dir,
    ] {
        let _ = std::fs::create_dir_all(dir);
    }
}

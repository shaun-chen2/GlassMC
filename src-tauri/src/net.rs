//! 统一的 HTTP 客户端。
//!
//! 两个要点：
//! 1. 带 User-Agent —— BMCLAPI 对空 UA / 默认 UA 会 403；
//! 2. 直连本地地址时不能走代理 —— 系统级代理会把 127.0.0.1 也劫持走，
//!    所以本地请求单独用 `local_client()`（禁用代理）。

use std::sync::OnceLock;
use std::time::Duration;

pub fn client() -> reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(format!(
                "GlassMC/{} (Tauri; +https://github.com/shaun-chen2/GlassMC)",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(Duration::from_secs(40))
            .connect_timeout(Duration::from_secs(15))
            .pool_max_idle_per_host(8)
            .build()
            .expect("构建 HTTP 客户端失败")
    })
    .clone()
}

#[allow(dead_code)] // 联机（陶瓦本地 HTTP 接口）会用它
pub fn local_client() -> reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("GlassMC")
            .timeout(Duration::from_secs(10))
            .no_proxy()
            .build()
            .expect("构建本地 HTTP 客户端失败")
    })
    .clone()
}

pub async fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let resp = client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("解析 JSON 失败：{e}"))
}

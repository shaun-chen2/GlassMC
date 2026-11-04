//! 微软正版登录（OAuth 2.0 设备码流程）。
//!
//! 完整链路：设备码 → 轮询换微软令牌 → Xbox Live → XSTS → Minecraft 服务 → 游戏档案。
//! 刷新令牌单独存 `ms-refresh.json`，不写进账号文件（账号文件是明文，容易被翻）。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::account::{self, Account};
use crate::config::Config;
use crate::paths;

const SCOPE: &str = "XboxLive.signin offline_access";
/// 旧版 Xbox Live 公共客户端。微软已收紧，实际应让用户填自己注册的 Azure 客户端 id。
const DEFAULT_CLIENT_ID: &str = "00000000402b5328";
const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

fn client_id(cfg: &Config) -> String {
    let manual = cfg.ms_client_id.trim();
    if manual.is_empty() {
        DEFAULT_CLIENT_ID.to_string()
    } else {
        manual.to_string()
    }
}

#[allow(dead_code)]
pub fn is_configured(cfg: &Config) -> bool {
    !cfg.ms_client_id.trim().is_empty()
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u32,
    pub interval: u32,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PollResult {
    /// pending / slow_down / done
    pub status: String,
    pub account: Option<Account>,
    pub message: String,
}

/// 申请设备码。
pub async fn start(cfg: &Config) -> Result<DeviceCode, String> {
    let resp = crate::net::client()
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id(cfg)), ("scope", SCOPE.to_string())])
        .send()
        .await
        .map_err(|e| format!("请求微软登录失败：{e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if status != 200 {
        return Err(oauth_error(&text, status));
    }
    let v: Value = serde_json::from_str(&text).map_err(|_| "设备码响应无法解析".to_string())?;
    let pick = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
    Ok(DeviceCode {
        device_code: pick("device_code").ok_or("设备码响应缺少 device_code")?,
        user_code: pick("user_code").ok_or("设备码响应缺少 user_code")?,
        verification_uri: pick("verification_uri")
            .or_else(|| pick("verification_url"))
            .unwrap_or_else(|| "https://www.microsoft.com/link".into()),
        expires_in: v.get("expires_in").and_then(|x| x.as_u64()).unwrap_or(900) as u32,
        interval: v
            .get("interval")
            .and_then(|x| x.as_u64())
            .unwrap_or(5)
            .max(1) as u32,
    })
}

/// 轮询一次。前端按 `interval` 秒调用，直到 status = done。
pub async fn poll(cfg: &Config, device_code: &str) -> Result<PollResult, String> {
    let resp = crate::net::client()
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", &client_id(cfg)),
            ("device_code", device_code),
        ])
        .send()
        .await
        .map_err(|e| format!("轮询微软授权失败：{e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);

    if status == 200 {
        let access = v
            .get("access_token")
            .and_then(|x| x.as_str())
            .ok_or("微软没有返回 access_token")?
            .to_string();
        let refresh_token = v
            .get("refresh_token")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());

        let profile = complete_login(&access).await?;
        let id = account::random_id();
        if let Some(rt) = refresh_token {
            save_refresh(&id, &rt);
        }
        let acc = Account {
            id,
            kind: "microsoft".into(),
            username: profile.name,
            uuid: Some(profile.uuid),
            access_token: Some(profile.access_token),
            source: "glassmc".into(),
            needs_reauth: false,
            skin_url: profile.skin_url,
            skin_variant: profile.skin_variant,
            ..Default::default()
        };
        return Ok(PollResult {
            status: "done".into(),
            message: format!("登录完成：{}", acc.username),
            account: Some(acc),
        });
    }

    let err = v.get("error").and_then(|x| x.as_str()).unwrap_or("");
    match err {
        "authorization_pending" => Ok(PollResult {
            status: "pending".into(),
            account: None,
            message: "等待浏览器中完成授权…".into(),
        }),
        "slow_down" => Ok(PollResult {
            status: "slow_down".into(),
            account: None,
            message: "微软要求降低轮询频率".into(),
        }),
        "expired_token" => Err("登录代码已过期，请重新发起微软登录".into()),
        "authorization_declined" => Err("你已在授权页面拒绝了本次登录".into()),
        _ => Err(oauth_error(&text, status)),
    }
}

/// 用保存的刷新令牌续期。
pub async fn refresh(cfg: &Config, acc: &Account) -> Result<Account, String> {
    let token =
        refresh_token(&acc.id).ok_or_else(|| "登录凭据已失效，请重新添加微软账号".to_string())?;
    let resp = crate::net::client()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id(cfg)),
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", token.clone()),
            ("scope", SCOPE.to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("刷新微软令牌失败：{e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status != 200 {
        if status == 400 || status == 401 {
            return Err("登录已过期，请重新添加微软账号".into());
        }
        return Err(oauth_error(&text, status));
    }
    let Some(ms_token) = v.get("access_token").and_then(|x| x.as_str()) else {
        return Err("微软没有返回新的 access_token".into());
    };
    if let Some(rt) = v.get("refresh_token").and_then(|x| x.as_str()) {
        save_refresh(&acc.id, rt);
    }
    let profile = complete_login(ms_token).await?;
    let mut updated = acc.clone();
    updated.kind = "microsoft".into();
    updated.username = profile.name;
    updated.uuid = Some(profile.uuid);
    updated.access_token = Some(profile.access_token);
    updated.needs_reauth = false;
    if profile.skin_url.is_some() {
        updated.skin_url = profile.skin_url;
        updated.skin_variant = profile.skin_variant;
    }
    Ok(updated)
}

struct Profile {
    name: String,
    uuid: String,
    access_token: String,
    skin_url: Option<String>,
    skin_variant: Option<String>,
}

async fn complete_login(ms_token: &str) -> Result<Profile, String> {
    // 1) Xbox Live
    let xbl = crate::net::client()
        .post(XBL_URL)
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|e| format!("Xbox Live 认证请求失败：{e}"))?;
    let xbl_status = xbl.status().as_u16();
    let xbl_text = xbl.text().await.unwrap_or_default();
    if xbl_status != 200 {
        return Err(format!("Xbox Live 认证失败（HTTP {xbl_status}）"));
    }
    let (xbl_token, _) = parse_xbox_token(&xbl_text)?;

    // 2) XSTS
    let xsts = crate::net::client()
        .post(XSTS_URL)
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|e| format!("XSTS 授权请求失败：{e}"))?;
    let xsts_status = xsts.status().as_u16();
    let xsts_text = xsts.text().await.unwrap_or_default();
    if xsts_status == 401 {
        return Err(xsts_error(&xsts_text));
    }
    if xsts_status != 200 {
        return Err(format!("XSTS 授权失败（HTTP {xsts_status}）"));
    }
    let (xsts_token, uhs) = parse_xbox_token(&xsts_text)?;

    // 3) Minecraft 令牌
    let mc = crate::net::client()
        .post(MC_LOGIN_URL)
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }))
        .send()
        .await
        .map_err(|e| format!("登录 Minecraft 服务失败：{e}"))?;
    let mc_status = mc.status().as_u16();
    let mc_text = mc.text().await.unwrap_or_default();
    if mc_status != 200 {
        return Err(format!("Minecraft 登录失败（HTTP {mc_status}）"));
    }
    let mc_json: Value =
        serde_json::from_str(&mc_text).map_err(|_| "Minecraft 登录响应无法解析")?;
    let mc_token = mc_json
        .get("access_token")
        .and_then(|x| x.as_str())
        .ok_or("Minecraft 登录没有返回令牌")?
        .to_string();

    // 4) 档案
    let resp = crate::net::client()
        .get(PROFILE_URL)
        .bearer_auth(&mc_token)
        .send()
        .await
        .map_err(|e| format!("读取 Minecraft 档案失败：{e}"))?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Err("该微软账号没有购买 Minecraft: Java Edition".into());
    }
    let text = resp.text().await.unwrap_or_default();
    if status != 200 {
        return Err(format!("读取 Minecraft 档案失败（HTTP {status}）"));
    }
    let v: Value = serde_json::from_str(&text).map_err(|_| "档案响应无法解析")?;
    let skin = v
        .get("skins")
        .and_then(|s| s.as_array())
        .and_then(|a| a.first())
        .cloned();
    Ok(Profile {
        name: v
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("Player")
            .to_string(),
        uuid: v
            .get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        access_token: mc_token,
        skin_url: skin
            .as_ref()
            .and_then(|s| s.get("url"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        skin_variant: skin
            .as_ref()
            .and_then(|s| s.get("variant"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
    })
}

fn parse_xbox_token(text: &str) -> Result<(String, String), String> {
    let v: Value = serde_json::from_str(text).map_err(|_| "Xbox 认证响应无法解析".to_string())?;
    let token = v
        .get("Token")
        .and_then(|x| x.as_str())
        .ok_or("Xbox 认证响应缺少 Token")?
        .to_string();
    let uhs = v
        .get("DisplayClaims")
        .and_then(|d| d.get("xui"))
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .and_then(|e| e.get("uhs"))
        .and_then(|x| x.as_str())
        .ok_or("Xbox 认证响应缺少 uhs")?
        .to_string();
    Ok((token, uhs))
}

fn xsts_error(text: &str) -> String {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let code = v.get("XErr").and_then(|x| x.as_i64());
    match code {
        Some(2148916233) => "该微软账号还没有 Xbox 档案，请先在 xbox.com 创建档案后重试".into(),
        Some(2148916235) => "该账号所在地区不支持 Xbox Live，请更换账号或调整网络环境".into(),
        Some(2148916238) => "该账号是未成年账号，需要家长在 Microsoft 家庭组中同意".into(),
        Some(c) => format!("Xbox 授权被拒绝（XErr={c}）"),
        None => "Xbox 授权被拒绝".into(),
    }
}

fn oauth_error(text: &str, status: u16) -> String {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    if let Some(desc) = v
        .get("error_description")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
    {
        // 微软的 description 带换行与追踪 id，截断到第一行更好读
        let first = desc.lines().next().unwrap_or(desc);
        return format!("微软登录失败：{first}");
    }
    if let Some(err) = v.get("error").and_then(|x| x.as_str()) {
        return format!("微软登录失败：{err}");
    }
    format!("微软登录失败（HTTP {status}）")
}

// ---------------------------------------------------------------- 刷新令牌存储

fn refresh_file() -> std::path::PathBuf {
    paths::paths().data_root.join("ms-refresh.json")
}

fn load_refresh() -> HashMap<String, String> {
    std::fs::read_to_string(refresh_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_refresh(id: &str, token: &str) {
    let mut map = load_refresh();
    map.insert(id.to_string(), token.to_string());
    if let Ok(s) = serde_json::to_string_pretty(&map) {
        let _ = std::fs::create_dir_all(paths::paths().data_root.clone());
        let _ = std::fs::write(refresh_file(), s);
    }
}

pub fn refresh_token(id: &str) -> Option<String> {
    load_refresh().get(id).cloned()
}

pub fn forget(id: &str) {
    let mut map = load_refresh();
    if map.remove(id).is_some() {
        if let Ok(s) = serde_json::to_string_pretty(&map) {
            let _ = std::fs::write(refresh_file(), s);
        }
    }
}

/// 兼容旧数据：从来没有存过刷新令牌的账号提示重新登录。
#[allow(dead_code)]
#[derive(Deserialize)]
struct LegacyRefresh {
    token: String,
}

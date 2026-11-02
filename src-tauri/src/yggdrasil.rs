//! Yggdrasil / Blessing Skin 外置登录。
//!
//! 不同站点的认证路径不同，必须逐个探测（404 视为路径不存在，继续下一个）：
//! - 标准 Mojang：`{root}/authenticate`
//! - Blessing Skin（LittleSkin / MiniSkins）：`{root}/authserver/authenticate`
//! - Ely.by：`{root}/auth/authenticate`
//!
//! 另外：`authenticate` 的响应通常**不带** properties，皮肤必须再查一次
//! `{base}/sessionserver/session/minecraft/profile/{uuid}` 才拿得到。

use base64::Engine;
use serde_json::{json, Value};

use crate::account::{self, Account};
use crate::config::{AuthServer, Config};
use crate::net;

pub fn auth_candidates(server: &AuthServer) -> Vec<String> {
    let base = server.normalized_base();
    let mut out = vec![base.clone()];
    if !base.ends_with("/authserver") && !base.ends_with("/auth") {
        out.push(format!("{base}/authserver"));
        out.push(format!("{base}/auth"));
    }
    out
}

/// sessionserver 挂在 yggdrasil 基址下，用户填到 `/authserver` 时要回退父路径。
pub fn session_bases(server: &AuthServer) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for base in auth_candidates(server) {
        let b = if let Some(s) = base.strip_suffix("/authserver") {
            s.to_string()
        } else if let Some(s) = base.strip_suffix("/auth") {
            s.to_string()
        } else {
            base
        };
        if !out.contains(&b) {
            out.push(b);
        }
    }
    out
}

pub fn find_server(cfg: &Config, id: &str) -> Option<AuthServer> {
    cfg.auth_servers.iter().find(|s| s.id == id).cloned()
}

/// 用账号密码登录。
pub async fn authenticate(
    server: &AuthServer,
    username: &str,
    password: &str,
) -> Result<Account, String> {
    let client_token = account::random_id();
    let mut last = String::new();
    for base in auth_candidates(server) {
        match authenticate_at(&base, server, username, password, &client_token).await {
            Ok(acc) => return Ok(acc),
            Err((code, msg)) => {
                if code == 404 {
                    last = msg;
                    continue;
                }
                return Err(msg);
            }
        }
    }
    Err(if last.is_empty() {
        "没有找到可用的认证接口，请检查服务器地址".to_string()
    } else {
        format!("{}（认证路径均不可用）", last)
    })
}

async fn authenticate_at(
    base: &str,
    server: &AuthServer,
    username: &str,
    password: &str,
    client_token: &str,
) -> Result<Account, (u16, String)> {
    let body = json!({
        "agent": { "name": "Minecraft", "version": 1 },
        "username": username,
        "password": password,
        "clientToken": client_token,
        "requestUser": true
    });
    let resp = net::client()
        .post(format!("{base}/authenticate"))
        .json(&body)
        .send()
        .await
        .map_err(|e| (0, format!("请求认证服务器失败：{e}")))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err((status, ygg_error(&text, status)));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|_| (status, "认证服务器返回的内容无法解析".to_string()))?;

    let profile = v
        .get("selectedProfile")
        .filter(|p| !p.is_null())
        .or_else(|| {
            v.get("availableProfiles")
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
        })
        .cloned();
    let mut skin = extract_skin(profile.as_ref().and_then(|p| p.get("properties")));
    let uuid = profile
        .as_ref()
        .and_then(|p| p.get("id"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    if skin.0.is_none() {
        if let Some(id) = uuid.clone() {
            let fetched = profile_textures(base, &id).await;
            if fetched.0.is_some() {
                skin = fetched;
            }
        }
    }

    Ok(Account {
        id: account::random_id(),
        kind: "yggdrasil".into(),
        username: profile
            .as_ref()
            .and_then(|p| p.get("name"))
            .and_then(|x| x.as_str())
            .unwrap_or(username)
            .to_string(),
        uuid,
        access_token: v
            .get("accessToken")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        client_token: Some(
            v.get("clientToken")
                .and_then(|x| x.as_str())
                .unwrap_or(client_token)
                .to_string(),
        ),
        auth_server_id: Some(server.id.clone()),
        source: "glassmc".into(),
        needs_reauth: false,
        skin_url: skin.0,
        skin_variant: skin.1,
        skin_local_path: None,
    })
}

/// 用已有令牌续期。
pub async fn refresh(server: &AuthServer, acc: &Account) -> Result<Account, String> {
    let token = acc
        .access_token
        .clone()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "该账号没有可刷新的令牌".to_string())?;
    let mut last = String::new();
    for base in auth_candidates(server) {
        match refresh_at(&base, acc, &token).await {
            Ok(a) => return Ok(a),
            Err((code, msg)) => {
                if code == 404 {
                    last = msg;
                    continue;
                }
                return Err(msg);
            }
        }
    }
    Err(if last.is_empty() {
        "没有找到可用的刷新接口".to_string()
    } else {
        last
    })
}

async fn refresh_at(base: &str, acc: &Account, token: &str) -> Result<Account, (u16, String)> {
    let body = json!({
        "accessToken": token,
        "clientToken": acc.client_token.clone().unwrap_or_else(account::random_id),
        "requestUser": true
    });
    let resp = net::client()
        .post(format!("{base}/refresh"))
        .json(&body)
        .send()
        .await
        .map_err(|e| (0, format!("请求认证服务器失败：{e}")))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err((status, ygg_error(&text, status)));
    }
    let v: Value = serde_json::from_str(&text).map_err(|_| (status, "刷新响应无法解析".into()))?;
    let profile = v
        .get("selectedProfile")
        .filter(|p| !p.is_null())
        .cloned();

    let mut updated = acc.clone();
    if let Some(t) = v.get("accessToken").and_then(|x| x.as_str()) {
        updated.access_token = Some(t.to_string());
    }
    if let Some(t) = v.get("clientToken").and_then(|x| x.as_str()) {
        updated.client_token = Some(t.to_string());
    }
    updated.needs_reauth = false;
    if let Some(p) = profile {
        if let Some(n) = p.get("name").and_then(|x| x.as_str()) {
            updated.username = n.to_string();
        }
        if let Some(id) = p.get("id").and_then(|x| x.as_str()) {
            updated.uuid = Some(id.to_string());
        }
        let skin = extract_skin(p.get("properties"));
        if skin.0.is_some() {
            updated.skin_url = skin.0;
            updated.skin_variant = skin.1;
        }
    }
    if updated.skin_url.is_none() {
        if let Some(id) = updated.uuid.clone() {
            let skin = profile_textures(base, &id).await;
            if skin.0.is_some() {
                updated.skin_url = skin.0;
                updated.skin_variant = skin.1;
            }
        }
    }
    Ok(updated)
}

/// 主动向 sessionserver 同步站点当前皮肤（站点换皮肤后调它）。
pub async fn sync_skin(server: &AuthServer, acc: &Account) -> Option<Account> {
    let uuid = acc.uuid.clone().filter(|u| !u.is_empty())?;
    for base in session_bases(server) {
        let (url, variant) = profile_textures(&base, &uuid).await;
        if let Some(u) = url {
            let mut updated = acc.clone();
            updated.skin_url = Some(u);
            if variant.is_some() {
                updated.skin_variant = variant;
            }
            return Some(updated);
        }
    }
    None
}

/// 查询 profile 贴图，返回（地址、模型）。
async fn profile_textures(base: &str, uuid: &str) -> (Option<String>, Option<String>) {
    let id = uuid.replace('-', "");
    let url = format!("{base}/sessionserver/session/minecraft/profile/{id}?unsigned=false");
    let resp = match net::client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return (None, None),
    };
    if !resp.status().is_success() {
        return (None, None);
    }
    let Ok(v) = resp.json::<Value>().await else {
        return (None, None);
    };
    extract_skin(v.get("properties"))
}

/// 从 properties 里解出 `textures` → SKIN 地址与模型。
fn extract_skin(properties: Option<&Value>) -> (Option<String>, Option<String>) {
    let Some(props) = properties.and_then(|p| p.as_array()) else {
        return (None, None);
    };
    for prop in props {
        if prop.get("name").and_then(|n| n.as_str()) != Some("textures") {
            continue;
        }
        let Some(value) = prop.get("value").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(value) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_slice::<Value>(&raw) else {
            continue;
        };
        let Some(skin) = parsed.get("textures").and_then(|t| t.get("SKIN")) else {
            continue;
        };
        let Some(url) = skin.get("url").and_then(|u| u.as_str()) else {
            continue;
        };
        let variant = skin
            .get("metadata")
            .and_then(|m| m.get("model"))
            .and_then(|m| m.as_str())
            .map(|m| if m == "slim" { "alex".to_string() } else { "steve".to_string() });
        return (Some(normalize_skin_url(url)), variant);
    }
    (None, None)
}

/// 少数站点返回 `http://` 的贴图地址，统一升级成 https。
pub fn normalize_skin_url(raw: &str) -> String {
    let s = raw.trim();
    if let Some(rest) = s.strip_prefix("http://") {
        format!("https://{rest}")
    } else {
        s.to_string()
    }
}

fn ygg_error(body: &str, status: u16) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(msg) = v
            .get("errorMessage")
            .and_then(|m| m.as_str())
            .or_else(|| v.get("error").and_then(|m| m.as_str()))
        {
            // 常见错误换成更好懂的说法
            let lower = msg.to_lowercase();
            if lower.contains("invalid credentials") || lower.contains("incorrect password") {
                return "账号或密码不正确".into();
            }
            return format!("认证失败：{msg}");
        }
    }
    format!("认证失败（HTTP {status}）")
}

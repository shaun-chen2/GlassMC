//! 账号：直接读原生版写下的 `accounts.json`（同一份文件，只读），
//! 没有任何账号时退化为离线账号。

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Account {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub username: String,
    pub uuid: Option<String>,
    pub access_token: Option<String>,
    pub client_token: Option<String>,
    pub auth_server_id: Option<String>,
    pub source: String,
    pub needs_reauth: bool,
    pub skin_url: Option<String>,
    pub skin_variant: Option<String>,
    pub skin_local_path: Option<String>,
}

impl Account {
    pub fn offline(name: &str) -> Self {
        Self {
            id: "offline".into(),
            kind: "offline".into(),
            username: name.to_string(),
            source: "glassmc".into(),
            ..Default::default()
        }
    }

    pub fn is_microsoft(&self) -> bool {
        matches!(self.kind.as_str(), "msa" | "microsoft")
    }

    /// 启动时传给游戏的 UUID（去掉横线）。
    pub fn auth_uuid(&self) -> String {
        if let Some(u) = self.uuid.as_deref() {
            if !u.is_empty() {
                return u.replace('-', "");
            }
        }
        match self.kind.as_str() {
            "offline" => offline_uuid(&self.username).replace('-', ""),
            _ => "00000000000000000000000000000000".to_string(),
        }
    }

    pub fn access_token(&self) -> String {
        match self.access_token.as_deref() {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => {
                if self.kind == "offline" {
                    self.username.clone()
                } else {
                    "0".into()
                }
            }
        }
    }
}

pub fn load_all() -> Vec<Account> {
    let p = paths::paths().accounts_file.clone();
    let Ok(s) = std::fs::read_to_string(&p) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<Account>>(&s).unwrap_or_default()
}

pub fn save_all(list: &[Account]) -> Result<(), String> {
    let p = paths::paths().accounts_file.clone();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    let s = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    std::fs::write(&p, s).map_err(|e| format!("写入账号失败：{e}"))
}

/// 插入或更新一个账号（按 id 匹配）。
pub fn upsert(acc: Account) -> Result<Vec<Account>, String> {
    let mut list = load_all();
    if let Some(slot) = list.iter_mut().find(|a| a.id == acc.id) {
        *slot = acc;
    } else {
        list.push(acc);
    }
    save_all(&list)?;
    Ok(list)
}

pub fn remove(id: &str) -> Result<Vec<Account>, String> {
    let mut list = load_all();
    list.retain(|a| a.id != id);
    save_all(&list)?;
    Ok(list)
}

/// 生成一个 UUID 样式的 id（不引入 uuid 依赖）。
pub fn random_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let a = (t >> 32) as u32;
    let b = (t >> 16) as u16;
    let c = ((t & 0xFFFF) as u16 & 0x0FFF) | 0x4000;
    let d = ((n as u16) & 0x3FFF) | 0x8000;
    let e = t ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!(
        "{a:08X}-{b:04X}-{c:04X}-{d:04X}-{:012X}",
        e % 0xFFFF_FFFF_FFFF
    )
}

pub fn find(id: &str) -> Option<Account> {
    load_all().into_iter().find(|a| a.id == id)
}

/// NameUUIDHash：`MD5("OfflinePlayer:名字")`，版本位 3、RFC4122 变体位。
pub fn offline_uuid(name: &str) -> String {
    let mut b = md5::compute(format!("OfflinePlayer:{name}").as_bytes()).0;
    b[6] = (b[6] & 0x0F) | 0x30;
    b[8] = (b[8] & 0x3F) | 0x80;
    let hx: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hx[0..8],
        &hx[8..12],
        &hx[12..16],
        &hx[16..20],
        &hx[20..32]
    )
}

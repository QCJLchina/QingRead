use crate::storage::store::{atomic_write_json, Store};
use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};

/// 系统凭据管理器里的服务名。
///
/// 兼容说明：产品已改名「轻阅 / QingRead」，但这个名字必须保持不变 ——
/// 它是旧版存放 WebDAV 密码的位置，改名会让升级后的用户重新输入密码。
pub const SYNC_SERVICE: &str = "EpubReader WebDAV";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub server_url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default = "default_remote_dir")]
    pub remote_dir: String,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            username: String::new(),
            remote_dir: default_remote_dir(),
        }
    }
}

impl SyncConfig {
    pub fn is_configured(&self) -> bool {
        !self.server_url.trim().is_empty() && !self.username.trim().is_empty()
    }

    pub fn normalized(&self) -> Self {
        let remote_dir = self
            .remote_dir
            .trim()
            .trim_matches(|c| c == '/' || c == '\\')
            .to_string();
        Self {
            server_url: self.server_url.trim().trim_end_matches('/').to_string(),
            username: self.username.trim().to_string(),
            remote_dir: if remote_dir.is_empty() {
                default_remote_dir()
            } else {
                remote_dir
            },
        }
    }
}

/// WebDAV 上的默认远端目录。
///
/// 兼容说明：保持旧名字，改掉会让已同步过的用户在新版本里看不到自己的书。
fn default_remote_dir() -> String {
    "EpubReader".to_string()
}

pub fn load_sync_config(store: &Store) -> SyncConfig {
    std::fs::read_to_string(&store.paths.sync_config_file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_sync_config(store: &Store, config: &SyncConfig) -> anyhow::Result<()> {
    if let Some(parent) = store.paths.sync_config_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write_json(&store.paths.sync_config_file, config)?;
    Ok(())
}

pub fn set_password(store: &Store, username: &str, password: &str) -> anyhow::Result<()> {
    let username = username.trim();
    if username.is_empty() {
        return Err(anyhow!("WebDAV 用户名不能为空"));
    }
    let entry = keyring::Entry::new(SYNC_SERVICE, username).context("无法打开系统凭据管理器")?;
    entry
        .set_password(password)
        .context("无法写入系统凭据管理器")?;
    let _ = store;
    Ok(())
}

pub fn get_password(_store: &Store, username: &str) -> anyhow::Result<Option<String>> {
    let username = username.trim();
    if username.is_empty() {
        return Ok(None);
    }
    let entry = keyring::Entry::new(SYNC_SERVICE, username).context("无法打开系统凭据管理器")?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow!("无法读取系统凭据: {e}")),
    }
}

pub fn delete_password(_store: &Store, username: &str) -> anyhow::Result<()> {
    let username = username.trim();
    if username.is_empty() {
        return Ok(());
    }
    let entry = keyring::Entry::new(SYNC_SERVICE, username).context("无法打开系统凭据管理器")?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow!("无法删除系统凭据: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_config_trims_credentials_and_remote_directory() {
        let config = SyncConfig {
            server_url: " https://dav.example.com/ ".to_string(),
            username: " user ".to_string(),
            remote_dir: " /Books/ ".to_string(),
        };

        let normalized = config.normalized();
        assert_eq!(normalized.server_url, "https://dav.example.com");
        assert_eq!(normalized.username, "user");
        assert_eq!(normalized.remote_dir, "Books");
        assert!(normalized.is_configured());
    }

    #[test]
    fn empty_remote_directory_uses_default() {
        let config = SyncConfig {
            server_url: "https://dav.example.com".to_string(),
            username: "user".to_string(),
            remote_dir: " / ".to_string(),
        };

        assert_eq!(config.normalized().remote_dir, "EpubReader");
    }
}

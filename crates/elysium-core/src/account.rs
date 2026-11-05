//! Player accounts: offline names and Microsoft accounts. Tokens for Microsoft accounts
//! never live here; see [`crate::auth`] and [`crate::secrets`].

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::fsutil;
use crate::paths::Paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    Offline,
    Microsoft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skin {
    pub url: String,
    /// `classic` or `slim`.
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub uuid: Uuid,
    pub username: String,
    pub kind: AccountKind,
    /// Whether the account owns Minecraft: Java Edition (Microsoft accounts only).
    #[serde(default)]
    pub owns_game: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<Skin>,
}

impl Account {
    pub fn offline(username: &str) -> Result<Self> {
        let username = username.trim();
        validate_username(username)?;
        Ok(Self {
            uuid: offline_uuid(username),
            username: username.to_owned(),
            kind: AccountKind::Offline,
            owns_game: false,
            skin: None,
        })
    }
}

/// Everything the game needs to know about the player for one launch.
#[derive(Clone, PartialEq, Eq)]
pub struct LaunchSession {
    pub username: String,
    pub uuid: Uuid,
    pub access_token: String,
    /// `msa` for Microsoft accounts, `legacy` for offline ones.
    pub user_type: &'static str,
    pub xuid: String,
    pub online: bool,
}

impl std::fmt::Debug for LaunchSession {
    // Never print the token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchSession")
            .field("username", &self.username)
            .field("uuid", &self.uuid)
            .field("user_type", &self.user_type)
            .field("online", &self.online)
            .finish_non_exhaustive()
    }
}

impl LaunchSession {
    pub fn offline(account: &Account) -> Self {
        Self {
            username: account.username.clone(),
            uuid: account.uuid,
            access_token: "0".to_owned(),
            user_type: "legacy",
            xuid: "0".to_owned(),
            online: false,
        }
    }
}

/// Minecraft usernames are 3–16 characters of `[A-Za-z0-9_]`.
pub fn validate_username(name: &str) -> Result<()> {
    let ok_len = (3..=16).contains(&name.len());
    let ok_chars = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok_len && ok_chars {
        Ok(())
    } else {
        Err(Error::Invalid(
            "usernames must be 3–16 characters: letters, numbers and underscores".into(),
        ))
    }
}

/// The UUID a vanilla server assigns an offline player: Java's
/// `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)` (MD5, version 3, no namespace).
pub fn offline_uuid(username: &str) -> Uuid {
    let digest = Md5::digest(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest);
    uuid::Builder::from_md5_bytes(bytes).into_uuid()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountStore {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub active: Option<Uuid>,
}

impl AccountStore {
    pub fn load(paths: &Paths) -> Result<Self> {
        Ok(fsutil::read_json(&paths.accounts_file())?.unwrap_or_default())
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        fsutil::write_json_atomic(&paths.accounts_file(), self)
    }

    pub fn active(&self) -> Option<&Account> {
        let active = self.active?;
        self.accounts.iter().find(|a| a.uuid == active)
    }

    /// Adds (or replaces) an account and makes it active.
    pub fn add(&mut self, account: Account) {
        self.accounts.retain(|a| a.uuid != account.uuid);
        self.active = Some(account.uuid);
        self.accounts.push(account);
    }

    pub fn remove(&mut self, uuid: Uuid) {
        self.accounts.retain(|a| a.uuid != uuid);
        if self.active == Some(uuid) {
            self.active = self.accounts.first().map(|a| a.uuid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_java() {
        // UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(UTF_8))
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }

    #[test]
    fn username_validation() {
        assert!(validate_username("Steve_123").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("this_name_is_too_long").is_err());
        assert!(validate_username("bad name").is_err());
    }

    #[test]
    fn store_add_remove() {
        let mut store = AccountStore::default();
        let a = Account::offline("Alex").unwrap();
        let s = Account::offline("Steve").unwrap();
        store.add(a.clone());
        store.add(s.clone());
        assert_eq!(store.active(), Some(&s));
        store.remove(s.uuid);
        assert_eq!(store.active(), Some(&a));
        store.add(a.clone());
        assert_eq!(store.accounts.len(), 1);
    }
}

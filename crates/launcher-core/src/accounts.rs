use std::path::PathBuf;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::write_atomic,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Microsoft,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub provider: Provider,
    pub username: String,
    pub uuid: String,
}

/// What the game needs to start. Never log `access_token`.
#[derive(Clone)]
pub struct Session {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
    pub xuid: Option<String>,
}

// Manual `Debug` so formatting a `Session` (or anything holding one) can never print the token.
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("username", &self.username)
            .field("uuid", &self.uuid)
            .field("access_token", &"<redacted>")
            .field("user_type", &self.user_type)
            .field("xuid", &self.xuid)
            .finish()
    }
}

impl Account {
    pub fn offline_session(&self) -> Session {
        Session {
            username: self.username.clone(),
            uuid: self.uuid.replace('-', ""),
            access_token: "0".to_string(),
            user_type: "legacy".to_string(),
            xuid: None,
        }
    }
}

/// Same as Java's `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`, which is what servers expect for offline players.
pub fn offline_uuid(username: &str) -> String {
    let mut h = Md5::digest(format!("OfflinePlayer:{username}").as_bytes());
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    let x = hex::encode(h);
    format!(
        "{}-{}-{}-{}-{}",
        &x[0..8],
        &x[8..12],
        &x[12..16],
        &x[16..20],
        &x[20..32]
    )
}

pub fn validate_offline_name(name: &str) -> Result<()> {
    let ok = (3..=16).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(LauncherError::auth(
            ErrorCode::OfflineNameInvalid,
            "Offline names must be 3–16 letters, numbers or underscores",
        ))
    }
}

pub fn offline_account(name: &str) -> Result<Account> {
    validate_offline_name(name)?;
    Ok(Account {
        provider: Provider::Offline,
        username: name.to_string(),
        uuid: offline_uuid(name),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    Dev,
    /// An optimized build made with `PERLAND_BUILD=beta`: offline accounts stay open (spec §3.1).
    Beta,
    Release,
}

impl BuildKind {
    pub fn current() -> Self {
        Self::from_parts(cfg!(debug_assertions), option_env!("PERLAND_BUILD"))
    }

    /// Debug assertions make a dev build; otherwise `PERLAND_BUILD=beta` at compile time makes
    /// a beta. rustc tracks `option_env!`, so changing the variable rebuilds this crate.
    pub fn from_parts(debug_assertions: bool, flag: Option<&str>) -> Self {
        if debug_assertions {
            Self::Dev
        } else if flag == Some("beta") {
            Self::Beta
        } else {
            Self::Release
        }
    }
}

/// Dev and beta builds allow offline accounts; release builds unlock them only after a licensed
/// Microsoft sign-in on this install.
pub fn offline_allowed(build: BuildKind, licensed_signin_seen: bool) -> bool {
    matches!(build, BuildKind::Dev | BuildKind::Beta) || licensed_signin_seen
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsFile {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub licensed_signin_seen: bool,
}

pub struct AccountStore {
    path: PathBuf,
}

impl AccountStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub async fn load(&self) -> Result<AccountsFile> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                LauncherError::disk(ErrorCode::AccountsDamaged, "The accounts file is damaged")
                    .with_detail(e)
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AccountsFile::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn save(&self, file: &AccountsFile) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(file).map_err(|e| {
            LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save accounts")
                .with_param("item", "accounts")
                .with_detail(e)
        })?;
        write_atomic(&self.path, &bytes).await
    }

    pub async fn add_offline(&self, name: &str, build: BuildKind) -> Result<Account> {
        let mut file = self.load().await?;
        if !offline_allowed(build, file.licensed_signin_seen) {
            return Err(LauncherError::auth(ErrorCode::OfflineLocked, "Offline accounts unlock after you sign in once with a Microsoft account that owns Minecraft")
                .with_fix("Add a Microsoft account first"));
        }
        let account = offline_account(name)?;
        file.accounts.retain(|a| a.uuid != account.uuid);
        file.accounts.push(account.clone());
        file.active = Some(account.uuid.clone());
        self.save(&file).await?;
        Ok(account)
    }

    pub async fn active(&self) -> Result<Option<Account>> {
        let file = self.load().await?;
        Ok(file
            .active
            .and_then(|uuid| file.accounts.into_iter().find(|a| a.uuid == uuid)))
    }

    pub async fn list(&self) -> Result<AccountsFile> {
        self.load().await
    }

    pub async fn set_active(&self, uuid: &str) -> Result<AccountsFile> {
        let mut file = self.load().await?;
        if !file.accounts.iter().any(|a| a.uuid == uuid) {
            return Err(LauncherError::auth(
                ErrorCode::NoAccount,
                "That account isn't on this computer anymore",
            ));
        }
        file.active = Some(uuid.to_string());
        self.save(&file).await?;
        Ok(file)
    }

    /// Forget an account. If it was active, the first remaining account becomes active.
    pub async fn remove(&self, uuid: &str) -> Result<AccountsFile> {
        let mut file = self.load().await?;
        file.accounts.retain(|a| a.uuid != uuid);
        if file.active.as_deref() == Some(uuid) {
            file.active = file.accounts.first().map(|a| a.uuid.clone());
        }
        self.save(&file).await?;
        Ok(file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCategory;

    #[test]
    fn offline_uuid_matches_java() {
        assert_eq!(
            offline_uuid("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        assert_eq!(
            offline_uuid("Steve"),
            "5627dd98-e6be-3c21-b8a8-e92344183641"
        );
    }

    #[test]
    fn validates_offline_names() {
        assert!(validate_offline_name("Steve").is_ok());
        assert!(validate_offline_name("a_b_c").is_ok());
        assert!(validate_offline_name("ab").is_err());
        assert!(validate_offline_name("has space").is_err());
        assert!(validate_offline_name("seventeen_chars_x").is_err());
        assert_eq!(
            validate_offline_name("ñandú").unwrap_err().category,
            ErrorCategory::Auth
        );
    }

    #[test]
    fn offline_session_shape() {
        let s = offline_account("Steve").unwrap().offline_session();
        assert_eq!(s.uuid, "5627dd98e6be3c21b8a8e92344183641");
        assert_eq!(s.access_token, "0");
        assert_eq!(s.user_type, "legacy");
    }

    #[test]
    fn session_debug_redacts_access_token() {
        let session = Session {
            username: "Steve".to_string(),
            uuid: "5627dd98e6be3c21b8a8e92344183641".to_string(),
            access_token: "secret-token-123".to_string(),
            user_type: "msa".to_string(),
            xuid: None,
        };
        let shown = format!("{session:?}");
        assert!(shown.contains("<redacted>"));
        assert!(shown.contains("Steve"));
        assert!(!shown.contains("secret-token-123"));
    }

    #[test]
    fn build_kind_comes_from_debug_assertions_then_the_beta_flag() {
        assert_eq!(BuildKind::from_parts(true, None), BuildKind::Dev);
        assert_eq!(BuildKind::from_parts(true, Some("beta")), BuildKind::Dev);
        assert_eq!(BuildKind::from_parts(false, Some("beta")), BuildKind::Beta);
        assert_eq!(BuildKind::from_parts(false, None), BuildKind::Release);
        assert_eq!(BuildKind::from_parts(false, Some("")), BuildKind::Release);
        assert_eq!(
            BuildKind::from_parts(false, Some("Beta")),
            BuildKind::Release
        );
    }

    #[test]
    fn offline_gate() {
        assert!(offline_allowed(BuildKind::Dev, false));
        assert!(offline_allowed(BuildKind::Beta, false));
        assert!(!offline_allowed(BuildKind::Release, false));
        assert!(offline_allowed(BuildKind::Release, true));
    }

    #[tokio::test]
    async fn beta_builds_add_offline_accounts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path().join("accounts.json"));
        let added = store.add_offline("Steve", BuildKind::Beta).await.unwrap();
        assert_eq!(added.username, "Steve");
    }

    #[tokio::test]
    async fn store_gates_and_activates() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path().join("accounts.json"));
        let err = store
            .add_offline("Steve", BuildKind::Release)
            .await
            .unwrap_err();
        assert_eq!(err.category, ErrorCategory::Auth);
        assert!(err.fix.is_some());
        let a = store.add_offline("Steve", BuildKind::Dev).await.unwrap();
        store.add_offline("Steve", BuildKind::Dev).await.unwrap();
        assert_eq!(
            store.load().await.unwrap().accounts.len(),
            1,
            "re-adding does not duplicate"
        );
        assert_eq!(store.active().await.unwrap(), Some(a));
    }

    #[tokio::test]
    async fn switch_and_remove_accounts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path().join("accounts.json"));
        let steve = store.add_offline("Steve", BuildKind::Dev).await.unwrap();
        let alex = store.add_offline("Alex", BuildKind::Dev).await.unwrap();
        assert_eq!(
            store.list().await.unwrap().active.as_deref(),
            Some(alex.uuid.as_str())
        );

        let file = store.set_active(&steve.uuid).await.unwrap();
        assert_eq!(file.active.as_deref(), Some(steve.uuid.as_str()));
        assert_eq!(
            store.set_active("not-a-uuid").await.unwrap_err().code,
            crate::ErrorCode::NoAccount
        );

        // Removing the active account makes the first remaining one active.
        let file = store.remove(&steve.uuid).await.unwrap();
        assert_eq!(file.accounts, vec![alex.clone()]);
        assert_eq!(file.active.as_deref(), Some(alex.uuid.as_str()));

        let file = store.remove(&alex.uuid).await.unwrap();
        assert!(file.accounts.is_empty());
        assert_eq!(file.active, None);
    }
}

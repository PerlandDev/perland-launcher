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
    /// The Azure app the token was issued to (`clientid`); `None` for an offline session.
    pub client_id: Option<String>,
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
            .field("client_id", &self.client_id)
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
            client_id: None,
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
    /// An optimized build made with `PERLAND_BUILD=beta`. Offline accounts are dev-only.
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

/// Offline accounts exist only in dev builds. Mojang's review requires every shipped build
/// to need a Microsoft account that owns Minecraft.
pub fn offline_allowed(build: BuildKind) -> bool {
    build == BuildKind::Dev
}

/// The launcher's own record of an unfinished Microsoft sign-in (a browser window it opened).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsFile {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub active: Option<String>,
    /// No longer read; kept so older files still parse and round-trip.
    #[serde(default)]
    pub licensed_signin_seen: bool,
    /// Launcher state, not a setting: the UI writes `Settings` whole and would overwrite it.
    #[serde(default)]
    pub offline_notice_dismissed: bool,
}

/// What the player may see: outside dev builds, offline accounts are gone, and `active` is
/// cleared when it named one. The file on disk is not touched.
pub fn visible(mut file: AccountsFile, build: BuildKind) -> AccountsFile {
    if offline_allowed(build) {
        return file;
    }
    file.accounts.retain(|a| a.provider != Provider::Offline);
    if let Some(active) = &file.active {
        if !file.accounts.iter().any(|a| &a.uuid == active) {
            file.active = None;
        }
    }
    file
}

/// True when `visible` would hide at least one account.
pub fn hides_offline(file: &AccountsFile, build: BuildKind) -> bool {
    !offline_allowed(build)
        && file
            .accounts
            .iter()
            .any(|a| a.provider == Provider::Offline)
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
        if !offline_allowed(build) {
            return Err(LauncherError::auth(
                ErrorCode::OfflineLocked,
                "Offline accounts are only available in development builds.",
            )
            .with_fix("Sign in with a Microsoft account that owns Minecraft"));
        }
        let account = offline_account(name)?;
        file.accounts.retain(|a| a.uuid != account.uuid);
        file.accounts.push(account.clone());
        file.active = Some(account.uuid.clone());
        self.save(&file).await?;
        Ok(account)
    }

    /// Add a Microsoft account, or refresh the one with the same uuid, and make it active.
    pub async fn add_microsoft(&self, account: Account) -> Result<AccountsFile> {
        let mut file = self.load().await?;
        file.active = Some(account.uuid.clone());
        match file.accounts.iter_mut().find(|a| a.uuid == account.uuid) {
            Some(existing) => *existing = account,
            None => file.accounts.push(account),
        }
        self.save(&file).await?;
        Ok(file)
    }

    /// Change the player name of the account `uuid`, leaving which account is active alone.
    /// `false` when no such account is stored (it was removed meanwhile): nothing is written.
    pub async fn rename(&self, uuid: &str, username: &str) -> Result<bool> {
        let mut file = self.load().await?;
        let Some(account) = file.accounts.iter_mut().find(|a| a.uuid == uuid) else {
            return Ok(false);
        };
        account.username = username.to_string();
        self.save(&file).await?;
        Ok(true)
    }

    pub async fn dismiss_offline_notice(&self) -> Result<()> {
        let mut file = self.load().await?;
        file.offline_notice_dismissed = true;
        self.save(&file).await
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
        assert_eq!(s.client_id, None);
    }

    #[test]
    fn session_debug_redacts_access_token() {
        let session = Session {
            username: "Steve".to_string(),
            uuid: "5627dd98e6be3c21b8a8e92344183641".to_string(),
            access_token: "secret-token-123".to_string(),
            user_type: "msa".to_string(),
            xuid: None,
            client_id: Some("client-1".to_string()),
        };
        let shown = format!("{session:?}");
        assert!(shown.contains("client-1"));
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

    fn microsoft(name: &str, uuid: &str) -> Account {
        Account {
            provider: Provider::Microsoft,
            username: name.to_string(),
            uuid: uuid.to_string(),
        }
    }

    #[test]
    fn offline_gate() {
        assert!(offline_allowed(BuildKind::Dev));
        assert!(!offline_allowed(BuildKind::Beta));
        assert!(!offline_allowed(BuildKind::Release));
    }

    #[test]
    fn visible_drops_offline_and_clears_active() {
        let steve = offline_account("Steve").unwrap();
        let alex = microsoft("Alex", "11111111-1111-1111-1111-111111111111");
        let file = AccountsFile {
            accounts: vec![steve.clone(), alex.clone()],
            active: Some(steve.uuid.clone()),
            ..AccountsFile::default()
        };
        for build in [BuildKind::Beta, BuildKind::Release] {
            let shown = visible(file.clone(), build);
            assert_eq!(shown.accounts, vec![alex.clone()]);
            assert_eq!(shown.active, None);
        }
        assert_eq!(visible(file.clone(), BuildKind::Dev), file);

        // An active Microsoft account stays active.
        let file = AccountsFile {
            active: Some(alex.uuid.clone()),
            ..file
        };
        assert_eq!(
            visible(file, BuildKind::Release).active.as_deref(),
            Some(alex.uuid.as_str())
        );
    }

    #[test]
    fn hides_offline_only_outside_dev() {
        let with_offline = AccountsFile {
            accounts: vec![offline_account("Steve").unwrap()],
            ..AccountsFile::default()
        };
        let without = AccountsFile {
            accounts: vec![microsoft("Alex", "11111111-1111-1111-1111-111111111111")],
            ..AccountsFile::default()
        };
        assert!(!hides_offline(&with_offline, BuildKind::Dev));
        assert!(hides_offline(&with_offline, BuildKind::Beta));
        assert!(hides_offline(&with_offline, BuildKind::Release));
        assert!(!hides_offline(&without, BuildKind::Release));
    }

    #[test]
    fn old_file_parses() {
        let file: AccountsFile =
            serde_json::from_str(r#"{"accounts":[],"active":null,"licensedSigninSeen":true}"#)
                .unwrap();
        assert!(!file.offline_notice_dismissed);
    }

    #[tokio::test]
    async fn add_microsoft_upserts_and_activates() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path().join("accounts.json"));
        let uuid = "11111111-1111-1111-1111-111111111111";
        store.add_microsoft(microsoft("Alex", uuid)).await.unwrap();
        let other = microsoft("Sam", "22222222-2222-2222-2222-222222222222");
        store.add_microsoft(other.clone()).await.unwrap();
        let file = store
            .add_microsoft(microsoft("AlexRenamed", uuid))
            .await
            .unwrap();
        assert_eq!(file.accounts.len(), 2, "same uuid does not duplicate");
        assert_eq!(file.active.as_deref(), Some(uuid));
        assert_eq!(
            file.accounts
                .iter()
                .find(|a| a.uuid == uuid)
                .unwrap()
                .username,
            "AlexRenamed"
        );
        assert_eq!(store.load().await.unwrap(), file);
    }

    #[tokio::test]
    async fn rename_keeps_the_active_account() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path().join("accounts.json"));
        let alex = "11111111-1111-1111-1111-111111111111";
        let sam = "22222222-2222-2222-2222-222222222222";
        store.add_microsoft(microsoft("Alex", alex)).await.unwrap();
        store.add_microsoft(microsoft("Sam", sam)).await.unwrap();
        assert!(store.rename(alex, "Alexis").await.unwrap());
        let file = store.load().await.unwrap();
        assert_eq!(
            file.active.as_deref(),
            Some(sam),
            "the active account stays"
        );
        let names: Vec<_> = file.accounts.iter().map(|a| a.username.as_str()).collect();
        assert_eq!(names, ["Alexis", "Sam"]);
        assert!(
            !store
                .rename("33333333-3333-3333-3333-333333333333", "X")
                .await
                .unwrap(),
            "an account that is gone is not added back"
        );
        assert_eq!(store.load().await.unwrap(), file);
    }

    #[tokio::test]
    async fn notice_state_persists() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path().join("accounts.json"));
        store.dismiss_offline_notice().await.unwrap();
        assert!(store.load().await.unwrap().offline_notice_dismissed);
    }

    /// A file written while the launcher remembered an approval wait still parses.
    #[test]
    fn a_file_with_the_old_approval_wait_still_parses() {
        let file: AccountsFile = serde_json::from_str(
            r#"{"accounts":[],"active":null,"msaWaiting":{"clientId":"abc","at":42}}"#,
        )
        .unwrap();
        assert_eq!(file, AccountsFile::default());
    }

    #[tokio::test]
    async fn add_offline_refused_in_beta_even_after_licensed_signin() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path().join("accounts.json"));
        store
            .save(&AccountsFile {
                licensed_signin_seen: true,
                ..AccountsFile::default()
            })
            .await
            .unwrap();
        for build in [BuildKind::Beta, BuildKind::Release] {
            let err = store.add_offline("Steve", build).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::OfflineLocked);
        }
        assert!(store.load().await.unwrap().accounts.is_empty());
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

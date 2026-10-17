//! The signed-in Perland player, kept between launches.
//!
//! The token (`plt_...`) is a credential and lives in the OS credential store only
//! ([`SecretStore`]). A small marker file, `perland-account.json`, holds what is safe to show
//! offline (the Engine's origin and the profile) and says a session exists, so a player who
//! never signed in never touches the credential store. Every method here is synchronous and
//! may block on the credential store (D-Bus on Linux): call them from `spawn_blocking`.

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use reqwest::Url;
use serde::{Deserialize, Serialize};

use super::api::Player;
use crate::{
    error::{ErrorCategory, ErrorCode, LauncherError, Result},
    fsutil,
};

/// The credential store entry: service and user of the one entry that holds the token.
pub const KEYRING_SERVICE: &str = "online.perland.launcher";
pub const KEYRING_USER: &str = "perland-account";

/// What the launcher shows about the signed-in player, with no secret in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The display name, else the email, else `""`.
    pub name: String,
    pub email: Option<String>,
    /// Whether a Discord account is linked.
    pub discord: bool,
    /// The linked Minecraft name, if an account is linked.
    pub minecraft: Option<String>,
}

impl Profile {
    pub fn from_player(player: &Player) -> Self {
        let name = if !player.display_name.is_empty() {
            player.display_name.clone()
        } else {
            player.email.clone().unwrap_or_default()
        };
        Self {
            name,
            email: player.email.clone(),
            discord: player.discord.is_some(),
            minecraft: player.minecraft.as_ref().map(|m| m.name.clone()),
        }
    }
}

/// A signed-in player: the Engine the token belongs to, the token and the profile.
#[derive(Clone)]
pub struct Session {
    pub engine: Url,
    pub token: String,
    pub profile: Profile,
}

// Manual Debug: the token is a credential.
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("engine", &self.engine.host_str().unwrap_or(""))
            .field("token", &"<redacted>")
            .field("profile", &self.profile)
            .finish()
    }
}

/// Why the credential store could not answer. Never carries the secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecretError {
    /// There is no credential store to talk to (a Linux desktop without Secret Service, a
    /// locked or missing keyring, a platform without a backend).
    NoBackend,
    /// Anything else, as a short phrase.
    Other(String),
}

/// One secret string in the credential store.
pub trait SecretStore: Send + Sync {
    /// `Ok(None)` when nothing is stored.
    fn load(&self) -> std::result::Result<Option<String>, SecretError>;
    fn save(&self, secret: &str) -> std::result::Result<(), SecretError>;
    /// Deleting what is not there is fine.
    fn delete(&self) -> std::result::Result<(), SecretError>;
}

/// A secret kept in memory only: what tests use, and the fallback where no credential store
/// is built in.
#[derive(Default)]
pub struct MemoryStore {
    value: Mutex<Option<String>>,
}

impl MemoryStore {
    fn value(&self) -> MutexGuard<'_, Option<String>> {
        self.value.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl SecretStore for MemoryStore {
    fn load(&self) -> std::result::Result<Option<String>, SecretError> {
        Ok(self.value().clone())
    }
    fn save(&self, secret: &str) -> std::result::Result<(), SecretError> {
        *self.value() = Some(secret.to_string());
        Ok(())
    }
    fn delete(&self) -> std::result::Result<(), SecretError> {
        *self.value() = None;
        Ok(())
    }
}

/// The OS credential store: Windows Credential Manager, or Secret Service on Linux. On any
/// other platform it is built without a backend and every call answers `NoBackend`.
pub struct KeyringStore {
    _private: (),
}

impl KeyringStore {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(windows, target_os = "linux"))]
mod native {
    use super::{KeyringStore, SecretError, SecretStore, KEYRING_SERVICE, KEYRING_USER};

    /// A short phrase for a keyring failure. Debug and Display of some variants carry the
    /// bytes of the secret or of other attributes, so only the kind is named.
    pub(super) fn map_error(e: keyring::Error) -> Result<(), SecretError> {
        use keyring::Error as E;
        Err(match e {
            E::NoStorageAccess(_) | E::PlatformFailure(_) => SecretError::NoBackend,
            E::BadEncoding(_) => SecretError::Other("the stored secret is not text".into()),
            E::TooLong(..) => SecretError::Other("the secret is too long".into()),
            E::Invalid(..) => SecretError::Other("the credential entry is invalid".into()),
            E::Ambiguous(_) => SecretError::Other("several credential entries match".into()),
            E::NoEntry => SecretError::Other("no credential entry".into()),
            _ => SecretError::Other("the credential store failed".into()),
        })
    }

    fn entry() -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| map_error(e).unwrap_err())
    }

    impl SecretStore for KeyringStore {
        fn load(&self) -> Result<Option<String>, SecretError> {
            match entry()?.get_password() {
                Ok(secret) => Ok(Some(secret)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => map_error(e).map(|()| None),
            }
        }
        fn save(&self, secret: &str) -> Result<(), SecretError> {
            entry()?.set_password(secret).or_else(map_error)
        }
        fn delete(&self) -> Result<(), SecretError> {
            match entry()?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => map_error(e),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn keyring_errors_map_without_the_secret() {
            let secret = "plt_secretsecretsecret";
            let fail = |e| map_error(e).unwrap_err();
            let boxed = || Box::<dyn std::error::Error + Send + Sync>::from("denied");
            assert_eq!(
                fail(keyring::Error::NoStorageAccess(boxed())),
                SecretError::NoBackend
            );
            assert_eq!(
                fail(keyring::Error::PlatformFailure(boxed())),
                SecretError::NoBackend
            );
            let SecretError::Other(text) = fail(keyring::Error::BadEncoding(secret.into())) else {
                panic!("BadEncoding must be Other");
            };
            assert!(!text.contains("plt_"));
            let SecretError::Other(text) = fail(keyring::Error::Invalid("a".into(), secret.into()))
            else {
                panic!("Invalid must be Other");
            };
            assert!(!text.contains("plt_"));
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
impl SecretStore for KeyringStore {
    fn load(&self) -> std::result::Result<Option<String>, SecretError> {
        Err(SecretError::NoBackend)
    }
    fn save(&self, _secret: &str) -> std::result::Result<(), SecretError> {
        Err(SecretError::NoBackend)
    }
    fn delete(&self) -> std::result::Result<(), SecretError> {
        Err(SecretError::NoBackend)
    }
}

/// The marker file. No token.
#[derive(Serialize, Deserialize)]
struct Marker {
    engine: String,
    name: String,
    email: Option<String>,
    discord: bool,
    /// Absent in a marker written before the Minecraft link existed.
    #[serde(default)]
    minecraft: Option<String>,
}

impl Marker {
    fn of(session: &Session) -> Self {
        Self {
            engine: session.engine.to_string(),
            name: session.profile.name.clone(),
            email: session.profile.email.clone(),
            discord: session.profile.discord,
            minecraft: session.profile.minecraft.clone(),
        }
    }
}

/// Where the signed-in player is kept: the marker file plus the credential store, with an
/// in-memory copy for a session the credential store could not take.
pub struct SessionStore {
    marker: PathBuf,
    secret: Arc<dyn SecretStore>,
    /// Only a session the credential store could not hold (`NoBackend`).
    memory: Mutex<Option<Session>>,
}

impl SessionStore {
    /// `marker` is `<root>/perland-account.json`.
    pub fn new(marker: PathBuf, secret: Arc<dyn SecretStore>) -> Self {
        Self {
            marker,
            secret,
            memory: Mutex::new(None),
        }
    }

    fn memory(&self) -> MutexGuard<'_, Option<Session>> {
        self.memory.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The saved session. The credential store is read only when the marker exists. A marker
    /// the store has no secret for is deleted. Any failure to read is no session, never an error.
    pub fn load(&self) -> Option<Session> {
        let bytes = match std::fs::read(&self.marker) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return self.memory().clone(),
            Err(e) => {
                tracing::warn!("couldn't read the Perland account marker: {}", e.kind());
                return None;
            }
        };
        let marker = match serde_json::from_slice::<Marker>(&bytes) {
            Ok(marker) => marker,
            Err(_) => {
                tracing::warn!("the Perland account marker is damaged");
                return None;
            }
        };
        let Ok(engine) = Url::parse(&marker.engine) else {
            tracing::warn!("the Perland account marker names no valid Engine");
            return None;
        };
        match self.secret.load() {
            Ok(Some(token)) => Some(Session {
                engine,
                token,
                profile: Profile {
                    name: marker.name,
                    email: marker.email,
                    discord: marker.discord,
                    minecraft: marker.minecraft,
                },
            }),
            Ok(None) => {
                self.remove_marker();
                None
            }
            Err(e) => {
                // The marker stays: the store may answer next time.
                tracing::warn!(
                    "couldn't read the Perland sign-in from the credential store: {}",
                    describe(&e)
                );
                None
            }
        }
    }

    /// Save the session: the token to the credential store first, then the marker. A store
    /// with no backend leaves the session in memory only, with no marker.
    pub fn save(&self, session: &Session) -> Result<()> {
        match self.secret.save(&session.token) {
            Ok(()) => {}
            Err(SecretError::NoBackend) => {
                tracing::info!(
                    "no credential store: the Perland sign-in lasts until the launcher closes"
                );
                self.remove_marker();
                *self.memory() = Some(session.clone());
                return Ok(());
            }
            Err(SecretError::Other(why)) => {
                return Err(LauncherError::new(
                    ErrorCategory::Auth,
                    ErrorCode::Internal,
                    "Couldn't save your Perland sign-in",
                )
                .with_detail(why));
            }
        }
        let bytes = serde_json::to_vec(&Marker::of(session)).expect("the marker serializes");
        if let Err(e) = fsutil::write_atomic_blocking(&self.marker, &bytes) {
            // Don't leave a token no marker points to.
            let _ = self.secret.delete();
            return Err(e);
        }
        *self.memory() = None;
        Ok(())
    }

    /// Delete the marker and the stored token. Failures are logged, not returned: signing out
    /// must always end with the player signed out here.
    pub fn forget(&self) {
        *self.memory() = None;
        // A launcher that never signed in has no marker and never touches the credential store.
        if !self.marker.exists() {
            return;
        }
        if let Err(e) = self.secret.delete() {
            tracing::warn!(
                "couldn't delete the Perland sign-in from the credential store: {}",
                describe(&e)
            );
        }
        self.remove_marker();
    }

    fn remove_marker(&self) {
        remove_file(&self.marker);
    }
}

fn remove_file(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("couldn't delete the Perland account marker: {}", e.kind()),
    }
}

/// A secret-free phrase for a log line.
fn describe(e: &SecretError) -> &str {
    match e {
        SecretError::NoBackend => "no credential store",
        SecretError::Other(why) => why,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

    use super::*;
    use crate::perland::api::{DiscordAccount, MinecraftAccount};

    const TOKEN: &str = "plt_0123456789abcdef0123456789abcdef";

    fn session() -> Session {
        Session {
            engine: Url::parse("https://engine.example.com").unwrap(),
            token: TOKEN.into(),
            profile: Profile {
                name: "Eve".into(),
                email: Some("eve@example.com".into()),
                discord: true,
                minecraft: None,
            },
        }
    }

    /// A secret store that counts calls and can answer a fixed error.
    #[derive(Default)]
    struct Spy {
        calls: AtomicUsize,
        value: Mutex<Option<String>>,
        fail: Mutex<Option<SecretError>>,
    }

    impl Spy {
        fn failing(error: SecretError) -> Self {
            let spy = Spy::default();
            *spy.fail.lock().unwrap() = Some(error);
            spy
        }
        fn answer(&self) -> std::result::Result<(), SecretError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.fail.lock().unwrap().clone() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        }
    }

    impl SecretStore for Spy {
        fn load(&self) -> std::result::Result<Option<String>, SecretError> {
            self.answer()?;
            Ok(self.value.lock().unwrap().clone())
        }
        fn save(&self, secret: &str) -> std::result::Result<(), SecretError> {
            self.answer()?;
            *self.value.lock().unwrap() = Some(secret.to_string());
            Ok(())
        }
        fn delete(&self) -> std::result::Result<(), SecretError> {
            self.answer()?;
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }

    fn marker_in(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("perland-account.json")
    }

    #[test]
    fn no_marker_never_reads_the_secret() {
        let dir = tempfile::tempdir().unwrap();
        let spy = Arc::new(Spy::default());
        let store = SessionStore::new(marker_in(&dir), spy.clone());
        assert!(store.load().is_none());
        assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let secret = Arc::new(MemoryStore::default());
        let store = SessionStore::new(marker_in(&dir), secret.clone());
        store.save(&session()).unwrap();
        // A fresh instance over the same files sees it: nothing relied on the in-memory copy.
        let again = SessionStore::new(marker_in(&dir), secret).load().unwrap();
        assert_eq!(again.engine, session().engine);
        assert_eq!(again.token, TOKEN);
        assert_eq!(again.profile, session().profile);
    }

    #[test]
    fn marker_without_secret_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let writer = SessionStore::new(marker_in(&dir), Arc::new(MemoryStore::default()));
        writer.save(&session()).unwrap();
        assert!(marker_in(&dir).exists());
        let reader = SessionStore::new(marker_in(&dir), Arc::new(MemoryStore::default()));
        assert!(reader.load().is_none());
        assert!(!marker_in(&dir).exists());
    }

    #[test]
    fn no_backend_keeps_memory_only() {
        let dir = tempfile::tempdir().unwrap();
        let spy = Arc::new(Spy::failing(SecretError::NoBackend));
        let store = SessionStore::new(marker_in(&dir), spy);
        store.save(&session()).unwrap();
        assert!(!marker_in(&dir).exists());
        let loaded = store.load().unwrap();
        assert_eq!(loaded.token, TOKEN);
        // Forgetting drops the memory copy too, even though the store has no backend.
        store.forget();
        assert!(store.load().is_none());
    }

    #[test]
    fn no_backend_clears_a_stale_marker() {
        let dir = tempfile::tempdir().unwrap();
        let first = SessionStore::new(marker_in(&dir), Arc::new(MemoryStore::default()));
        first.save(&session()).unwrap();
        let second = SessionStore::new(
            marker_in(&dir),
            Arc::new(Spy::failing(SecretError::NoBackend)),
        );
        second.save(&session()).unwrap();
        assert!(!marker_in(&dir).exists());
    }

    #[test]
    fn unreadable_secret_store_is_no_session_and_keeps_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        let writer = SessionStore::new(marker_in(&dir), Arc::new(MemoryStore::default()));
        writer.save(&session()).unwrap();
        for error in [SecretError::NoBackend, SecretError::Other("boom".into())] {
            let reader = SessionStore::new(marker_in(&dir), Arc::new(Spy::failing(error)));
            assert!(reader.load().is_none());
            assert!(marker_in(&dir).exists());
        }
    }

    #[test]
    fn other_secret_errors_fail_the_save_without_a_marker() {
        let dir = tempfile::tempdir().unwrap();
        let spy = Arc::new(Spy::failing(SecretError::Other("locked".into())));
        let store = SessionStore::new(marker_in(&dir), spy);
        let err = store.save(&session()).unwrap_err();
        assert!(!format!("{err:?}").contains(TOKEN));
        assert!(!marker_in(&dir).exists());
        assert!(store.load().is_none());
    }

    #[test]
    fn damaged_marker_is_no_session() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(marker_in(&dir), b"{not json").unwrap();
        let spy = Arc::new(Spy::default());
        let store = SessionStore::new(marker_in(&dir), spy.clone());
        assert!(store.load().is_none());
        assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn forget_without_a_marker_never_touches_the_secret() {
        let dir = tempfile::tempdir().unwrap();
        let spy = Arc::new(Spy::default());
        SessionStore::new(marker_in(&dir), spy.clone()).forget();
        assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn forget_deletes_both() {
        let dir = tempfile::tempdir().unwrap();
        let secret = Arc::new(MemoryStore::default());
        let store = SessionStore::new(marker_in(&dir), secret.clone());
        store.save(&session()).unwrap();
        store.forget();
        assert!(!marker_in(&dir).exists());
        assert_eq!(secret.load().unwrap(), None);
        assert!(store.load().is_none());
        // Forgetting again is fine.
        store.forget();
    }

    #[test]
    fn marker_never_holds_the_token() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(marker_in(&dir), Arc::new(MemoryStore::default()));
        store.save(&session()).unwrap();
        let text = std::fs::read_to_string(marker_in(&dir)).unwrap();
        assert!(!text.contains("plt_"));
        assert!(!text.contains(TOKEN));
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["engine"], "https://engine.example.com/");
        assert_eq!(json["name"], "Eve");
        assert_eq!(json["email"], "eve@example.com");
        assert_eq!(json["discord"], true);
        assert_eq!(json["minecraft"], serde_json::Value::Null);
        assert_eq!(json.as_object().unwrap().len(), 5);
    }

    #[test]
    fn session_debug_hides_token() {
        let text = format!("{:?}", session());
        assert!(!text.contains("plt_"));
        assert!(text.contains("<redacted>"));
    }

    #[test]
    fn profile_from_player_falls_back_to_email_then_empty() {
        let mut player = Player {
            display_name: "Eve".into(),
            email: Some("eve@example.com".into()),
            discord: Some(DiscordAccount::default()),
            ..Player::default()
        };
        let p = Profile::from_player(&player);
        assert_eq!((p.name.as_str(), p.discord), ("Eve", true));
        player.display_name = String::new();
        player.discord = None;
        let p = Profile::from_player(&player);
        assert_eq!((p.name.as_str(), p.discord), ("eve@example.com", false));
        player.email = None;
        assert_eq!(Profile::from_player(&player).name, "");
    }

    #[test]
    fn profile_carries_minecraft_name() {
        let player = Player {
            minecraft: Some(MinecraftAccount {
                uuid: "u".into(),
                name: "Moi".into(),
                linked_at: "t".into(),
            }),
            ..Player::default()
        };
        assert_eq!(
            Profile::from_player(&player).minecraft.as_deref(),
            Some("Moi")
        );
        assert_eq!(Profile::from_player(&Player::default()).minecraft, None);
    }

    #[test]
    fn marker_without_minecraft_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        // The marker #48 wrote, before the Minecraft link existed.
        std::fs::write(
            marker_in(&dir),
            br#"{"engine":"https://api.example.com/","name":"Steve","email":null,"discord":true}"#,
        )
        .unwrap();
        let secret = Arc::new(MemoryStore::default());
        secret.save(TOKEN).unwrap();
        let loaded = SessionStore::new(marker_in(&dir), secret).load().unwrap();
        assert_eq!(loaded.profile.name, "Steve");
        assert_eq!(loaded.profile.minecraft, None);
    }

    #[test]
    fn marker_round_trips_minecraft() {
        let dir = tempfile::tempdir().unwrap();
        let secret = Arc::new(MemoryStore::default());
        let mut linked = session();
        linked.profile.minecraft = Some("Moi".into());
        SessionStore::new(marker_in(&dir), secret.clone())
            .save(&linked)
            .unwrap();
        let again = SessionStore::new(marker_in(&dir), secret).load().unwrap();
        assert_eq!(again.profile.minecraft.as_deref(), Some("Moi"));
    }

    #[test]
    fn memory_store_round_trips() {
        let store = MemoryStore::default();
        assert_eq!(store.load().unwrap(), None);
        store.save("x").unwrap();
        assert_eq!(store.load().unwrap().as_deref(), Some("x"));
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }
}

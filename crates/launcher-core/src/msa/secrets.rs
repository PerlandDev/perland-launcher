//! Microsoft refresh tokens, kept in the OS credential store under a key per account.
//!
//! Windows Credential Manager caps a credential at 2560 bytes, stored as UTF-16, so a token is
//! split into chunks of at most [`CHUNK_CHARS`] characters, one credential each:
//! `msa:<uuid>:0`, `msa:<uuid>:1`, ... Nothing here prints a token; errors never carry one.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use crate::perland::session::SecretError;

/// The most characters in one stored chunk. At two UTF-16 bytes each (up to four for a
/// character outside the BMP) this stays under the 2560-byte cap of a Windows credential.
pub const CHUNK_CHARS: usize = 1000;

/// Secrets addressed by key.
pub trait KeyedSecrets: Send + Sync {
    /// `Ok(None)` when nothing is stored under `key`.
    fn load(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn save(&self, key: &str, value: &str) -> Result<(), SecretError>;
    /// Deleting what is not there is fine.
    fn delete(&self, key: &str) -> Result<(), SecretError>;
}

/// Secrets kept in memory only: what tests use.
#[derive(Default)]
pub struct MemorySecrets {
    values: Mutex<HashMap<String, String>>,
}

impl MemorySecrets {
    fn values(&self) -> MutexGuard<'_, HashMap<String, String>> {
        self.values.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl KeyedSecrets for MemorySecrets {
    fn load(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self.values().get(key).cloned())
    }
    fn save(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.values().insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.values().remove(key);
        Ok(())
    }
}

/// The OS credential store, one entry per key on the service `KEYRING_SERVICE`. On a platform
/// without a backend every call answers `NoBackend`.
pub struct KeyringSecrets {
    _private: (),
}

impl KeyringSecrets {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for KeyringSecrets {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(windows, target_os = "linux"))]
impl KeyedSecrets for KeyringSecrets {
    fn load(&self, key: &str) -> Result<Option<String>, SecretError> {
        match entry(key)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => map_error(e).map(|()| None),
        }
    }
    fn save(&self, key: &str, value: &str) -> Result<(), SecretError> {
        entry(key)?.set_password(value).or_else(map_error)
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => map_error(e),
        }
    }
}

#[cfg(any(windows, target_os = "linux"))]
use crate::perland::session::{native::map_error, KEYRING_SERVICE};

#[cfg(any(windows, target_os = "linux"))]
fn entry(key: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(KEYRING_SERVICE, key).map_err(|e| map_error(e).unwrap_err())
}

#[cfg(not(any(windows, target_os = "linux")))]
impl KeyedSecrets for KeyringSecrets {
    fn load(&self, _key: &str) -> Result<Option<String>, SecretError> {
        Err(SecretError::NoBackend)
    }
    fn save(&self, _key: &str, _value: &str) -> Result<(), SecretError> {
        Err(SecretError::NoBackend)
    }
    fn delete(&self, _key: &str) -> Result<(), SecretError> {
        Err(SecretError::NoBackend)
    }
}

fn chunk_key(uuid: &str, index: usize) -> String {
    format!("msa:{uuid}:{index}")
}

/// Split on `char` boundaries into pieces of at most [`CHUNK_CHARS`] characters.
fn chunks(token: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut count = 0;
    for (i, _) in token.char_indices() {
        if count == CHUNK_CHARS {
            pieces.push(&token[start..i]);
            start = i;
            count = 0;
        }
        count += 1;
    }
    if start < token.len() {
        pieces.push(&token[start..]);
    }
    pieces
}

/// Store the refresh token of account `uuid`, replacing any earlier one. An empty token
/// stores nothing and removes what was there.
pub fn save_refresh(store: &dyn KeyedSecrets, uuid: &str, token: &str) -> Result<(), SecretError> {
    let pieces = chunks(token);
    for (i, piece) in pieces.iter().enumerate() {
        store.save(&chunk_key(uuid, i), piece)?;
    }
    // Remove chunks left by a longer earlier token, until a load finds none.
    let mut i = pieces.len();
    while store.load(&chunk_key(uuid, i))?.is_some() {
        store.delete(&chunk_key(uuid, i))?;
        i += 1;
    }
    Ok(())
}

/// The refresh token of account `uuid`: the chunks from `:0` until one is missing. `None`
/// when `:0` is missing.
pub fn load_refresh(store: &dyn KeyedSecrets, uuid: &str) -> Result<Option<String>, SecretError> {
    let mut token = String::new();
    let mut i = 0;
    while let Some(piece) = store.load(&chunk_key(uuid, i))? {
        token.push_str(&piece);
        i += 1;
    }
    Ok((i > 0).then_some(token))
}

/// Remove every chunk of account `uuid`'s refresh token. Nothing stored is fine.
pub fn delete_refresh(store: &dyn KeyedSecrets, uuid: &str) -> Result<(), SecretError> {
    let mut i = 0;
    while store.load(&chunk_key(uuid, i))?.is_some() {
        store.delete(&chunk_key(uuid, i))?;
        i += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "11111111-2222-3333-4444-555555555555";

    fn token(len: usize) -> String {
        (0..len).map(|i| (b'a' + (i % 26) as u8) as char).collect()
    }

    #[test]
    fn round_trips_a_long_token() {
        let store = MemorySecrets::default();
        let long = token(2500);
        save_refresh(&store, UUID, &long).unwrap();
        assert!(store.load(&chunk_key(UUID, 0)).unwrap().is_some());
        assert!(store.load(&chunk_key(UUID, 2)).unwrap().is_some());
        assert_eq!(store.load(&chunk_key(UUID, 3)).unwrap(), None);
        assert_eq!(
            store.load(&chunk_key(UUID, 0)).unwrap().unwrap().len(),
            CHUNK_CHARS
        );
        assert_eq!(load_refresh(&store, UUID).unwrap(), Some(long));
    }

    #[test]
    fn shorter_token_removes_stale_chunks() {
        let store = MemorySecrets::default();
        save_refresh(&store, UUID, &token(2500)).unwrap();
        save_refresh(&store, UUID, "short").unwrap();
        assert_eq!(store.load(&chunk_key(UUID, 1)).unwrap(), None);
        assert_eq!(store.load(&chunk_key(UUID, 2)).unwrap(), None);
        assert_eq!(
            load_refresh(&store, UUID).unwrap().as_deref(),
            Some("short")
        );
    }

    #[test]
    fn missing_is_none() {
        let store = MemorySecrets::default();
        assert_eq!(load_refresh(&store, UUID).unwrap(), None);
    }

    #[test]
    fn delete_removes_every_chunk() {
        let store = MemorySecrets::default();
        save_refresh(&store, UUID, &token(2500)).unwrap();
        delete_refresh(&store, UUID).unwrap();
        for i in 0..3 {
            assert_eq!(store.load(&chunk_key(UUID, i)).unwrap(), None);
        }
        assert_eq!(load_refresh(&store, UUID).unwrap(), None);
        // Deleting again is fine.
        delete_refresh(&store, UUID).unwrap();
    }

    #[test]
    fn keys_are_per_account() {
        let store = MemorySecrets::default();
        save_refresh(&store, "a", &token(1500)).unwrap();
        save_refresh(&store, "b", "other").unwrap();
        delete_refresh(&store, "b").unwrap();
        assert_eq!(load_refresh(&store, "a").unwrap(), Some(token(1500)));
        assert_eq!(load_refresh(&store, "b").unwrap(), None);
    }

    #[test]
    fn empty_token_stores_nothing() {
        let store = MemorySecrets::default();
        save_refresh(&store, UUID, &token(2500)).unwrap();
        save_refresh(&store, UUID, "").unwrap();
        assert_eq!(store.load(&chunk_key(UUID, 0)).unwrap(), None);
        assert_eq!(load_refresh(&store, UUID).unwrap(), None);
    }

    #[test]
    fn splits_on_char_boundaries() {
        let store = MemorySecrets::default();
        let wide: String = "é".repeat(CHUNK_CHARS + 5);
        save_refresh(&store, UUID, &wide).unwrap();
        let first = store.load(&chunk_key(UUID, 0)).unwrap().unwrap();
        assert_eq!(first.chars().count(), CHUNK_CHARS);
        assert_eq!(load_refresh(&store, UUID).unwrap(), Some(wide));
    }

    #[test]
    fn exact_multiple_has_no_empty_chunk() {
        let store = MemorySecrets::default();
        save_refresh(&store, UUID, &token(CHUNK_CHARS * 2)).unwrap();
        assert_eq!(store.load(&chunk_key(UUID, 2)).unwrap(), None);
        assert_eq!(
            load_refresh(&store, UUID).unwrap().unwrap().len(),
            CHUNK_CHARS * 2
        );
    }
}

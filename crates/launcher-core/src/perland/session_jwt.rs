//! A cache of Perland session JWTs, so the launcher doesn't mint a new one per request.
//! The base JWT (`player-session`) and one scoped JWT per server (`select-tenant`) are each
//! reused while more than [`REFRESH_MARGIN_SECS`] from their `exp`. Each key has its own async
//! lock, so concurrent callers share one refresh. `now` comes from the caller (seconds since
//! the Unix epoch) so tests control time. A JWT is a credential: nothing here prints one.

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::Value;

use super::auth::SessionJwt;
use crate::{
    error::{ErrorCode, LauncherError, Result},
    sync::BoxFuture,
};

/// A JWT closer than this to its `exp` is replaced, so a request never carries one that
/// expires in flight.
pub const REFRESH_MARGIN_SECS: u64 = 300;

/// The base JWT's key; a tenant id is never empty.
const BASE_KEY: &str = "";

/// Where fresh JWTs come from. The app implements it over `AuthClient` and the device token.
pub trait JwtSource: Send + Sync {
    /// A JWT for the signed-in player, not yet scoped to a server.
    fn player_session(&self) -> BoxFuture<'_, Result<SessionJwt>>;
    /// A JWT scoped to one server.
    fn select_tenant<'a>(
        &'a self,
        base: &'a SessionJwt,
        tenant: &'a str,
    ) -> BoxFuture<'a, Result<SessionJwt>>;
}

/// The `exp` claim (seconds since the Unix epoch) of a JWT. The signature is not checked: this
/// only decides when to ask for a new one.
pub fn jwt_exp(jwt: &str) -> Option<u64> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("exp")?.as_u64()
}

fn is_fresh(jwt: &SessionJwt, now: u64) -> bool {
    jwt_exp(jwt.as_str()).is_some_and(|exp| exp > now.saturating_add(REFRESH_MARGIN_SECS))
}

type Slot = Arc<tokio::sync::Mutex<Option<SessionJwt>>>;

#[derive(Default)]
pub struct JwtCache {
    slots: Mutex<HashMap<String, Slot>>,
}

impl fmt::Debug for JwtCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JwtCache(<redacted>)")
    }
}

impl JwtCache {
    /// The base (`player-session`) JWT, for calls that aren't about one server.
    pub async fn base(&self, source: &dyn JwtSource, now: u64) -> Result<SessionJwt> {
        let slot = self.slot(BASE_KEY);
        let mut held = slot.lock().await;
        Self::cached_or(&mut held, now, source.player_session()).await
    }

    /// A JWT scoped to `tenant`, minted from the base JWT when the cached one is stale.
    pub async fn jwt_for(
        &self,
        source: &dyn JwtSource,
        tenant: &str,
        now: u64,
    ) -> Result<SessionJwt> {
        // An empty tenant is the base JWT's key: its slot would be locked here and then again
        // by `base`, which waits forever.
        if tenant.is_empty() {
            return Err(LauncherError::network(
                ErrorCode::ServerNotFound,
                "That server wasn't found.",
            ));
        }
        let slot = self.slot(tenant);
        let mut held = slot.lock().await;
        if let Some(jwt) = held.as_ref().filter(|j| is_fresh(j, now)) {
            return Ok(jwt.clone());
        }
        let base = self.base(source, now).await?;
        Self::cached_or(&mut held, now, source.select_tenant(&base, tenant)).await
    }

    /// Drops one server's JWT, e.g. after the Engine answered 401 to it.
    pub fn forget(&self, tenant: &str) {
        self.slots.lock().unwrap().remove(tenant);
    }

    /// Drops the base JWT, e.g. after the Engine refused a JWT minted from it. Scoped JWTs
    /// stay until they are refused themselves.
    pub fn forget_base(&self) {
        self.forget(BASE_KEY);
    }

    /// Drops every JWT: sign-out, or a new session.
    pub fn clear(&self) {
        self.slots.lock().unwrap().clear();
    }

    fn slot(&self, key: &str) -> Slot {
        self.slots
            .lock()
            .unwrap()
            .entry(key.to_string())
            .or_default()
            .clone()
    }

    /// The held JWT if fresh, else the one `fetch` brings. A JWT with no readable `exp` is
    /// handed back once and not kept; a failed fetch keeps nothing.
    async fn cached_or(
        held: &mut Option<SessionJwt>,
        now: u64,
        fetch: BoxFuture<'_, Result<SessionJwt>>,
    ) -> Result<SessionJwt> {
        if let Some(jwt) = held.as_ref().filter(|j| is_fresh(j, now)) {
            return Ok(jwt.clone());
        }
        let jwt = fetch.await?;
        *held = jwt_exp(jwt.as_str()).map(|_| jwt.clone());
        Ok(jwt)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use super::*;

    const NOW: u64 = 1_700_000_000;

    fn jwt_with_exp(exp: u64) -> String {
        let body = URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp}}}"));
        format!("h.{body}.s")
    }

    struct FakeSource {
        player_sessions: AtomicUsize,
        selects: AtomicUsize,
        /// The `exp` of every JWT it mints.
        exp: u64,
        fail_first_select: bool,
        delay: bool,
    }

    impl FakeSource {
        fn new(exp: u64) -> Self {
            Self {
                player_sessions: AtomicUsize::new(0),
                selects: AtomicUsize::new(0),
                exp,
                fail_first_select: false,
                delay: false,
            }
        }
        fn counts(&self) -> (usize, usize) {
            (
                self.player_sessions.load(Ordering::SeqCst),
                self.selects.load(Ordering::SeqCst),
            )
        }
    }

    impl JwtSource for FakeSource {
        fn player_session(&self) -> BoxFuture<'_, Result<SessionJwt>> {
            Box::pin(async move {
                self.player_sessions.fetch_add(1, Ordering::SeqCst);
                Ok(SessionJwt::new(jwt_with_exp(self.exp)))
            })
        }
        fn select_tenant<'a>(
            &'a self,
            _base: &'a SessionJwt,
            _tenant: &'a str,
        ) -> BoxFuture<'a, Result<SessionJwt>> {
            Box::pin(async move {
                let n = self.selects.fetch_add(1, Ordering::SeqCst);
                if self.delay {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                if self.fail_first_select && n == 0 {
                    return Err(LauncherError::network(ErrorCode::Internal, "boom"));
                }
                Ok(SessionJwt::new(jwt_with_exp(self.exp)))
            })
        }
    }

    #[tokio::test]
    async fn a_blank_tenant_is_refused_without_a_mint() {
        let cache = JwtCache::default();
        let source = FakeSource::new(NOW + 3600);
        let e = tokio::time::timeout(Duration::from_secs(5), cache.jwt_for(&source, "", NOW))
            .await
            .expect("no deadlock on the base slot")
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::ServerNotFound);
        assert_eq!(source.counts(), (0, 0));
    }

    #[test]
    fn exp_reads_the_payload() {
        assert_eq!(jwt_exp(&jwt_with_exp(1_700_000_000)), Some(1_700_000_000));
        let padded = format!("h.{}==.s", URL_SAFE_NO_PAD.encode("{\"exp\":5}"));
        assert_eq!(jwt_exp(&padded), Some(5));
        assert_eq!(jwt_exp("garbage"), None);
        assert_eq!(jwt_exp("a.!!!.c"), None);
        assert_eq!(
            jwt_exp(&format!("h.{}.s", URL_SAFE_NO_PAD.encode("{}"))),
            None
        );
    }

    #[tokio::test]
    async fn a_fresh_token_is_reused() {
        let (cache, source) = (JwtCache::default(), FakeSource::new(NOW + 3600));
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        cache.jwt_for(&source, "t1", NOW + 10).await.unwrap();
        assert_eq!(source.counts(), (1, 1));
    }

    #[tokio::test]
    async fn a_token_within_five_minutes_of_exp_is_refreshed() {
        let (cache, source) = (JwtCache::default(), FakeSource::new(NOW + 299));
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        assert_eq!(source.counts().1, 2);

        let (cache, source) = (JwtCache::default(), FakeSource::new(NOW + 301));
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        assert_eq!(source.counts().1, 1);
    }

    #[tokio::test]
    async fn a_token_without_exp_is_used_once_and_not_cached() {
        struct Opaque(AtomicUsize);
        impl JwtSource for Opaque {
            fn player_session(&self) -> BoxFuture<'_, Result<SessionJwt>> {
                Box::pin(async { Ok(SessionJwt::new("opaque")) })
            }
            fn select_tenant<'a>(
                &'a self,
                _: &'a SessionJwt,
                _: &'a str,
            ) -> BoxFuture<'a, Result<SessionJwt>> {
                Box::pin(async {
                    self.0.fetch_add(1, Ordering::SeqCst);
                    Ok(SessionJwt::new("opaque"))
                })
            }
        }
        let (cache, source) = (JwtCache::default(), Opaque(AtomicUsize::new(0)));
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        assert_eq!(source.0.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn tenants_are_cached_apart() {
        let (cache, source) = (JwtCache::default(), FakeSource::new(NOW + 3600));
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        cache.jwt_for(&source, "t2", NOW).await.unwrap();
        assert_eq!(source.counts(), (1, 2));
    }

    #[tokio::test]
    async fn the_base_jwt_is_cached() {
        let (cache, source) = (JwtCache::default(), FakeSource::new(NOW + 3600));
        cache.base(&source, NOW).await.unwrap();
        cache.base(&source, NOW).await.unwrap();
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        assert_eq!(source.counts(), (1, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_callers_share_one_refresh() {
        let cache = JwtCache::default();
        let mut source = FakeSource::new(NOW + 3600);
        source.delay = true;
        let calls = (0..8).map(|_| cache.jwt_for(&source, "t1", NOW));
        for jwt in futures_util::future::join_all(calls).await {
            jwt.unwrap();
        }
        assert_eq!(source.counts(), (1, 1));
    }

    #[tokio::test]
    async fn forget_drops_one_tenant() {
        let (cache, source) = (JwtCache::default(), FakeSource::new(NOW + 3600));
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        cache.jwt_for(&source, "t2", NOW).await.unwrap();
        cache.forget("t1");
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        cache.jwt_for(&source, "t2", NOW).await.unwrap();
        assert_eq!(source.counts().1, 3);
    }

    #[tokio::test]
    async fn forget_base_keeps_the_teams() {
        let (cache, source) = (JwtCache::default(), FakeSource::new(NOW + 3600));
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        cache.forget_base();
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        assert_eq!(source.counts(), (1, 1), "t1's JWT is still cached");
        cache.jwt_for(&source, "t2", NOW).await.unwrap();
        assert_eq!(source.counts(), (2, 2), "a new team mints a new base");
    }

    #[tokio::test]
    async fn clear_drops_everything() {
        let (cache, source) = (JwtCache::default(), FakeSource::new(NOW + 3600));
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        cache.clear();
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        assert_eq!(source.counts(), (2, 2));
    }

    #[tokio::test]
    async fn a_failed_refresh_caches_nothing() {
        let cache = JwtCache::default();
        let mut source = FakeSource::new(NOW + 3600);
        source.fail_first_select = true;
        assert!(cache.jwt_for(&source, "t1", NOW).await.is_err());
        cache.jwt_for(&source, "t1", NOW).await.unwrap();
        assert_eq!(source.counts().1, 2);
    }
}

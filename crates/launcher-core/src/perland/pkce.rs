//! PKCE (RFC 7636) for the Discord sign-in: a verifier the launcher keeps, the challenge the
//! Engine sees first, and a `state` that ties the browser's return to this attempt. The
//! verifier is a secret until the code is exchanged, so nothing here prints it.

use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::Url;
use sha2::{Digest, Sha256};

/// One sign-in attempt's secrets.
#[derive(Clone)]
pub struct Pkce {
    /// 43 base64url characters (32 random bytes).
    pub verifier: String,
    /// base64url (no padding) of the verifier's SHA-256: 43 characters.
    pub challenge: String,
    /// 22 base64url characters (16 random bytes).
    pub state: String,
}

impl fmt::Debug for Pkce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pkce { .. }")
    }
}

fn random_b64(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).expect("the operating system has no random source");
    URL_SAFE_NO_PAD.encode(bytes)
}

impl Pkce {
    pub fn new() -> Self {
        Self::from_verifier(&random_b64(32), &random_b64(16))
    }

    /// For tests: fixed verifier and state.
    pub fn from_verifier(verifier: &str, state: &str) -> Self {
        Self {
            verifier: verifier.to_string(),
            challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            state: state.to_string(),
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// `{engine}/api/v1/player/discord/authorize?redirect_uri=…&code_challenge=…&code_challenge_method=S256&state=…`.
/// Built here, never taken from the UI: the browser only ever opens the Engine.
pub fn authorize_url(engine: &Url, redirect: &str, pkce: &Pkce) -> Url {
    let mut url = engine.clone();
    if let Ok(mut segments) = url.path_segments_mut() {
        segments
            .pop_if_empty()
            .extend(["api", "v1", "player", "discord", "authorize"]);
    }
    url.set_query(None);
    url.query_pairs_mut()
        .append_pair("redirect_uri", redirect)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &pkce.state);
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_matches_rfc7636() {
        let p = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk", "s");
        assert_eq!(p.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn new_is_43_url_safe_chars() {
        let p = Pkce::new();
        for v in [&p.verifier, &p.challenge] {
            assert_eq!(v.len(), 43);
            assert!(v
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        }
        assert!(!p.state.is_empty() && p.state.len() <= 256);
        assert!(p.state.bytes().all(|b| b.is_ascii_graphic()));
        let q = Pkce::new();
        assert_ne!(p.verifier, q.verifier);
        assert_ne!(p.state, q.state);
    }

    #[test]
    fn authorize_url_encodes() {
        let p = Pkce::from_verifier("v", "st ate");
        let engine = Url::parse("https://engine.example/").unwrap();
        let url = authorize_url(&engine, "http://127.0.0.1:5000/callback", &p);
        assert_eq!(url.path(), "/api/v1/player/discord/authorize");
        let q = url.query().unwrap();
        assert!(q.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5000%2Fcallback"));
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(pairs.len(), 4);
        assert_eq!(
            pairs[0],
            (
                "redirect_uri".into(),
                "http://127.0.0.1:5000/callback".into()
            )
        );
        assert_eq!(pairs[1], ("code_challenge".into(), p.challenge.clone()));
        assert_eq!(pairs[2], ("code_challenge_method".into(), "S256".into()));
        assert_eq!(pairs[3], ("state".into(), "st ate".into()));
    }

    #[test]
    fn authorize_url_keeps_an_engine_path_prefix() {
        let p = Pkce::from_verifier("v", "s");
        let engine = Url::parse("https://example.org/engine").unwrap();
        let url = authorize_url(&engine, "http://127.0.0.1:5000/callback", &p);
        assert_eq!(url.path(), "/engine/api/v1/player/discord/authorize");
    }

    #[test]
    fn debug_hides_the_secrets() {
        let p = Pkce::from_verifier("secretverifier", "secretstate");
        let shown = format!("{p:?}");
        assert!(!shown.contains("secretverifier") && !shown.contains("secretstate"));
        assert!(!shown.contains(&p.challenge));
    }
}

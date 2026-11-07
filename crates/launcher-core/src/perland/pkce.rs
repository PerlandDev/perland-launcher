//! PKCE (RFC 7636) for the OAuth sign-in: a verifier the launcher keeps, the challenge the
//! Engine sees first, and a `state` that ties the browser's return to this attempt. The
//! verifier is a secret until the code is exchanged, so nothing here prints it.

use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
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
    fn debug_hides_the_secrets() {
        let p = Pkce::from_verifier("secretverifier", "secretstate");
        let shown = format!("{p:?}");
        assert!(!shown.contains("secretverifier") && !shown.contains("secretstate"));
        assert!(!shown.contains(&p.challenge));
    }
}

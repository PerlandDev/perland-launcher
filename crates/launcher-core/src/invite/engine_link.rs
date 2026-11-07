//! Links that point at an invite on the Engine itself rather than at the invite page:
//! `<engine>/api/v1/invites/<token>[/pack[/zip]]`. The invite page's **Copy modpack link**
//! copies `…/pack`, so players paste these where an invite link belongs. The token is a
//! credential: `Debug` never prints it.

use std::fmt;

use super::{check_engine_url, link::TOKEN_LEN, Url};
use crate::error::{ErrorCode, LauncherError};

/// The Engine route prefix before the token (Engine `invite/src/lib/api.ts` `inviteUrl`).
const ROUTE: &str = "/api/v1/invites/";

/// What may follow the token: the view, the `.mrpack`, or the zip of it.
const SUFFIXES: [&str; 5] = ["", "/", "/pack", "/pack/", "/pack/zip"];

/// Longest input looked at, in UTF-8 bytes, as for invite links.
const MAX_INPUT: usize = 512;

/// A modpack link the launcher can't turn into an invite: its Engine isn't the Perland site's.
pub fn pack_link_refused() -> LauncherError {
    LauncherError::pack(
        ErrorCode::InvitePackLink,
        "That's a server's modpack link, not its invite link",
    )
    .with_fix(
        "On the server's invite page, choose Open in Perland Launcher, or copy the page's address",
    )
}

/// An Engine invite route: the Engine (with any path it sits under, no trailing slash) and
/// the invite token.
#[derive(Clone, PartialEq, Eq)]
pub struct EngineInviteLink {
    engine: Url,
    token: String,
}

impl EngineInviteLink {
    /// `https://<engine>[/<path>]/api/v1/invites/<token>[/pack[/zip]]`, or `http://` to this
    /// computer only. A query, a fragment and user info are refused. `None` for anything else.
    pub fn parse(input: &str) -> Option<EngineInviteLink> {
        let raw = input.trim();
        // Checked on the raw text, as for invite links: `Url` hides an empty `?` or `#`.
        if raw.is_empty()
            || raw.len() > MAX_INPUT
            || raw
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '?' | '#' | '@' | '\\'))
        {
            return None;
        }
        let url = check_engine_url(raw)?;
        let path = url.path();
        let at = path.rfind(ROUTE)?;
        let rest = &path[at + ROUTE.len()..];
        let token = rest.get(..TOKEN_LEN)?;
        let valid = token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !valid || !SUFFIXES.contains(&&rest[TOKEN_LEN..]) {
            return None;
        }
        let mut engine = url.clone();
        engine.set_path(&path[..at]);
        Some(EngineInviteLink {
            engine,
            token: token.to_string(),
        })
    }

    /// The Engine, as a site's `config.json` would name it.
    pub fn engine(&self) -> &Url {
        &self.engine
    }

    /// Whether `engine` (an `engineUrl` from `config.json`) is this link's Engine: the same
    /// origin and path, ignoring a trailing slash.
    pub fn is_on(&self, engine: &Url) -> bool {
        let base = |u: &Url| {
            format!(
                "{}{}",
                u.origin().ascii_serialization(),
                u.path().trim_end_matches('/')
            )
        };
        base(&self.engine) == base(engine)
    }

    /// The credential. Only for building the invite link; never log it.
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for EngineInviteLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "EngineInviteLink({}{ROUTE}<invite>)",
            self.engine.as_str().trim_end_matches('/')
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> String {
        "A".repeat(TOKEN_LEN)
    }

    fn engine_of(input: &str) -> Option<String> {
        EngineInviteLink::parse(input).map(|l| l.engine().to_string())
    }

    #[test]
    fn the_copied_modpack_link_names_its_engine_and_token() {
        let t = token();
        let link = EngineInviteLink::parse(&format!(
            " https://api.example.com/api/v1/invites/{t}/pack "
        ))
        .unwrap();
        assert_eq!(link.engine().as_str(), "https://api.example.com/");
        assert_eq!(link.token(), t);
    }

    #[test]
    fn the_view_and_the_zip_are_engine_links_too() {
        let t = token();
        for suffix in ["", "/", "/pack/", "/pack/zip"] {
            assert_eq!(
                engine_of(&format!(
                    "https://API.example.com/api/v1/invites/{t}{suffix}"
                )),
                Some("https://api.example.com/".into()),
                "{suffix:?}"
            );
        }
    }

    #[test]
    fn an_engine_under_a_path_keeps_it() {
        let t = token();
        assert_eq!(
            engine_of(&format!(
                "https://example.com/engine/api/v1/invites/{t}/pack"
            )),
            Some("https://example.com/engine".into())
        );
        assert_eq!(
            engine_of(&format!("http://127.0.0.1:8080/api/v1/invites/{t}/pack")),
            Some("http://127.0.0.1:8080/".into())
        );
    }

    #[test]
    fn anything_else_is_not_an_engine_link() {
        let t = token();
        let short = &t[..42];
        for input in [
            format!("https://play.example.com/{t}"),
            format!("http://api.example.com/api/v1/invites/{t}/pack"),
            format!("https://api.example.com/api/v1/invites/{short}/pack"),
            format!("https://api.example.com/api/v1/invites/{t}A/pack"),
            format!("https://api.example.com/api/v1/invites/{t}/icon"),
            format!("https://api.example.com/api/v1/invites/{t}/mods"),
            format!("https://api.example.com/api/v1/invites/{t}/pack/zip/x"),
            format!("https://api.example.com/api/v1/invites/{t}/pack?x=1"),
            format!("https://api.example.com/api/v1/invites/{t}/pack#x"),
            format!("https://me@api.example.com/api/v1/invites/{t}/pack"),
            format!(
                "https://api.example.com/api/v1/invites/{}%41/pack",
                &t[..40]
            ),
            format!("https://api.example.com/api/v1/invites/{t} /pack"),
            "https://perlanddev.github.io/asalith-modpack/pack.toml".into(),
            "https://api.example.com/api/v1/invites/".into(),
            String::new(),
        ] {
            assert_eq!(EngineInviteLink::parse(&input), None, "{input:?}");
        }
    }

    #[test]
    fn the_engine_matches_with_or_without_a_trailing_slash() {
        let t = token();
        let link = EngineInviteLink::parse(&format!(
            "https://example.com/engine/api/v1/invites/{t}/pack"
        ))
        .unwrap();
        assert!(link.is_on(&Url::parse("https://example.com/engine/").unwrap()));
        assert!(link.is_on(&Url::parse("https://EXAMPLE.com:443/engine").unwrap()));
        assert!(!link.is_on(&Url::parse("https://example.com/").unwrap()));
        assert!(!link.is_on(&Url::parse("https://other.example.com/engine").unwrap()));
    }

    #[test]
    fn debug_never_prints_the_token() {
        let t = token();
        let link =
            EngineInviteLink::parse(&format!("https://api.example.com/api/v1/invites/{t}/pack"))
                .unwrap();
        let text = format!("{link:?}");
        assert!(!text.contains(&t), "{text}");
        assert!(text.contains("api.example.com"), "{text}");
    }
}

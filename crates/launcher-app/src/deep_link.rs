//! `perland://pack?url=<https pack.toml>` (core spec §5) and
//! `perland://join?invite=<https invite link>` (join-by-link spec). The link is remote input:
//! only an https packwiz link or an https invite link gets through, and the player still clicks
//! Add pack or Join before anything downloads.
use std::fmt;

use launcher_core::{
    invite::{shown_invite, InviteLink},
    sources::PackInput,
    ErrorCode, LauncherError, Result,
};
use serde::Serialize;

fn invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackLinkInvalid,
        "That doesn't look like a pack link",
    )
    .with_fix("Pack links start with https:// and usually end in pack.toml")
}

/// A link the OS handed the launcher, after checking.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeepLink {
    /// `perland://pack?url=`: an https packwiz link for New instance.
    Pack { url: String },
    /// `perland://join?invite=`: a canonical invite link for Join server.
    Join { invite: String },
}

// Manual Debug: an invite holds its token.
impl fmt::Debug for DeepLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeepLink::Pack { url } => f.debug_struct("Pack").field("url", url).finish(),
            DeepLink::Join { invite } => f
                .debug_struct("Join")
                .field("invite", &shown_invite(invite))
                .finish(),
        }
    }
}

fn join_invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::InviteLinkInvalid,
        "That isn't a Perland invite link",
    )
}

/// The one value of `key`, if it appears exactly once.
fn only_param(link: &url::Url, key: &str) -> Option<String> {
    let mut values = link
        .query_pairs()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned());
    match (values.next(), values.next()) {
        (Some(value), None) => Some(value),
        _ => None,
    }
}

/// `perland://join?invite=…` → the canonical invite. Only an https invite gets through.
fn parse_join(link: &url::Url) -> Result<String> {
    if link.query_pairs().any(|(k, _)| k != "invite") {
        return Err(join_invalid());
    }
    let invite = only_param(link, "invite").ok_or_else(join_invalid)?;
    if !invite.trim_start().starts_with("https://") {
        return Err(join_invalid());
    }
    InviteLink::parse(&invite)
        .map(|l| l.canonical())
        .map_err(|_| join_invalid())
}

/// A `perland://pack?url=…` or `perland://join?invite=…` link.
pub fn parse(raw: &str) -> Result<DeepLink> {
    let link = url::Url::parse(raw.trim()).map_err(|_| invalid())?;
    if link.scheme() != "perland" {
        return Err(invalid());
    }
    let bare = matches!(link.path(), "" | "/");
    match link.host_str() {
        Some("pack") if bare => parse_pack(&link).map(|url| DeepLink::Pack { url }),
        Some("join") if bare => parse_join(&link).map(|invite| DeepLink::Join { invite }),
        Some("join") => Err(join_invalid()),
        _ => Err(invalid()),
    }
}

/// 3b's pack rule, unchanged: one `url`, https, and it must parse as packwiz.
fn parse_pack(link: &url::Url) -> Result<String> {
    let pack = only_param(link, "url").ok_or_else(invalid)?;
    // Stricter than a pasted link: a web page never gets loopback http.
    if !pack.starts_with("https://") {
        return Err(invalid());
    }
    match PackInput::parse(&pack).map_err(|_| invalid())? {
        PackInput::Packwiz { url } => Ok(url),
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_links_accept_only_https_pack_links() {
        let asalith = "https://perlanddev.github.io/asalith-modpack/pack.toml";
        for good in [
            "perland://pack?url=https%3A%2F%2Fperlanddev.github.io%2Fasalith-modpack%2Fpack.toml",
            "perland://pack/?url=https://perlanddev.github.io/asalith-modpack/pack.toml",
            " perland://pack?url=https://perlanddev.github.io/asalith-modpack/pack.toml ",
        ] {
            assert_eq!(
                parse(good).unwrap(),
                DeepLink::Pack {
                    url: asalith.into()
                },
                "{good}"
            );
        }
        for bad in [
            "perland://pack?url=http://x.example/pack.toml",
            "perland://pack?url=http://localhost:8080/pack.toml",
            "perland://pack?url=C:%5CUsers%5Cme%5Cevil.mrpack",
            "perland://pack?url=modrinth:sky",
            "perland://pack?url=https://a.example/pack.toml&url=https://b.example/pack.toml",
            "perland://pack",
            "perland://pack/extra?url=https://a.example/pack.toml",
            "https://perlanddev.github.io/asalith-modpack/pack.toml",
            "not a link",
        ] {
            assert_eq!(
                parse(bad).unwrap_err().code,
                ErrorCode::PackLinkInvalid,
                "{bad}"
            );
        }
    }

    #[test]
    fn join_links_carry_a_canonical_invite() {
        let token = "A".repeat(43);
        let canonical = format!("https://play.example.com/{token}");
        for good in [
            format!("perland://join?invite=https%3A%2F%2FPLAY.example.com%2F{token}%2F"),
            format!("perland://join/?invite=https://play.example.com/{token}"),
            format!(" perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token} "),
            // The Engine's Go `url.QueryEscape` form (a `+` never appears in a valid invite)
            // and the invite page's `encodeURIComponent` form decode to the same invite.
            format!("perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}"),
            format!("perland://join?invite=https%3a%2f%2fplay.example.com%2f{token}"),
        ] {
            assert_eq!(
                parse(&good).unwrap(),
                DeepLink::Join {
                    invite: canonical.clone()
                },
                "{good}"
            );
        }
        assert_eq!(
            parse(&format!(
                "perland://join?invite=https%3A%2F%2Fplay.example.com%3A8443%2F{token}"
            ))
            .unwrap(),
            DeepLink::Join {
                invite: format!("https://play.example.com:8443/{token}")
            }
        );
        for bad in [
            format!("perland://join?invite=http%3A%2F%2Fplay.example.com%2F{token}"),
            format!("perland://join?invite=http%3A%2F%2F127.0.0.1%3A5173%2F{token}"),
            format!("perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}&invite=https%3A%2F%2Fplay.example.com%2F{token}"),
            format!("perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}&url=x"),
            format!("perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}%3Fref%3Dx"),
            format!("perland://join/extra?invite=https%3A%2F%2Fplay.example.com%2F{token}"),
            "perland://join?invite=https%3A%2F%2Fplay.example.com%2Fshort".to_string(),
            "perland://join?url=https://a.example/pack.toml".to_string(),
            "perland://join".to_string(),
        ] {
            assert_eq!(
                parse(&bad).unwrap_err().code,
                ErrorCode::InviteLinkInvalid,
                "{bad}"
            );
        }
        assert_eq!(
            parse("perland://joinx?invite=x").unwrap_err().code,
            ErrorCode::PackLinkInvalid
        );
    }

    #[test]
    fn debug_hides_the_invite_token_and_json_is_tagged() {
        let token = "A".repeat(43);
        let link = DeepLink::Join {
            invite: format!("https://play.example.com/{token}"),
        };
        let shown = format!("{link:?}");
        assert!(!shown.contains(&token), "{shown}");
        assert!(shown.contains("<invite>"), "{shown}");
        let json = serde_json::to_value(&link).unwrap();
        assert_eq!(json["kind"], "join");
        assert_eq!(
            serde_json::to_value(DeepLink::Pack {
                url: "https://x.example/pack.toml".into()
            })
            .unwrap()["kind"],
            "pack"
        );
    }
}

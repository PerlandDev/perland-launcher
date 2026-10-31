//! Invite links: `https://<invite host>/<43-char token>`.

use std::fmt;

use serde::Serialize;

use crate::error::{ErrorCode, LauncherError, Result};

/// Length of an invite token: 32 random bytes in unpadded base64url.
pub const TOKEN_LEN: usize = 43;

/// Longest input `parse` looks at, in UTF-8 bytes; anything longer can't be an invite link.
const MAX_INPUT: usize = 512;

/// Longest host, after IDNA: a DNS name is at most 253 characters (RFC 1035). It also keeps every
/// canonical link under `MAX_INPUT`, so it parses again.
const MAX_HOST: usize = 253;

/// A parsed invite link. The token is a credential: `Debug` and `Display` print
/// `https://<host>/<invite>` instead.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct InviteLink {
    scheme: &'static str,
    /// Lowercase (as `Url` normalizes it); an IPv6 literal keeps its brackets.
    host: String,
    /// `None` for the scheme's default port.
    port: Option<u16>,
    token: String,
}

pub(crate) fn link_invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::InviteLinkInvalid,
        "That isn't a Perland invite link",
    )
    .with_fix("Invite links look like https://play.example.com/ followed by a long code")
}

fn valid_token(s: &str) -> bool {
    s.len() == TOKEN_LEN
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl InviteLink {
    /// `https://<host>[:port]/<token>[/]`, or `http://` to this computer only. Extra path
    /// segments, a query, a fragment and user info are refused.
    pub fn parse(input: &str) -> Result<InviteLink> {
        let raw = input.trim();
        // Checked on the raw text: `Url` hides an empty `?` or `#` and turns `\` into `/`.
        if raw.is_empty()
            || raw.len() > MAX_INPUT
            || raw
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '?' | '#' | '@' | '\\'))
        {
            return Err(link_invalid());
        }
        let url = reqwest::Url::parse(raw).map_err(|_| link_invalid())?;
        let scheme = match url.scheme() {
            "https" => "https",
            "http" if crate::http::is_loopback(&url) => "http",
            _ => return Err(link_invalid()),
        };
        let host = url
            .host_str()
            .filter(|h| !h.is_empty() && h.len() <= MAX_HOST)
            .ok_or_else(link_invalid)?
            .to_ascii_lowercase();
        let token = url
            .path()
            .strip_prefix('/')
            .map(|p| p.strip_suffix('/').unwrap_or(p))
            .filter(|t| valid_token(t))
            .ok_or_else(link_invalid)?
            .to_string();
        Ok(InviteLink {
            scheme,
            host,
            port: url.port(),
            token,
        })
    }

    fn port_suffix(&self) -> String {
        self.port.map(|p| format!(":{p}")).unwrap_or_default()
    }

    /// `https://play.example.com[:port]`: where the invite site's `config.json` lives.
    pub fn origin(&self) -> String {
        format!("{}://{}{}", self.scheme, self.host, self.port_suffix())
    }

    /// The host alone, for messages ("From play.example.com").
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The identity instances are compared by: lowercase host, no trailing slash.
    pub fn canonical(&self) -> String {
        format!("{}/{}", self.origin(), self.token)
    }

    /// The credential. Only for request URLs and the redactor; never log it.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// `https://play.example.com/<invite>`: safe for logs and errors.
    pub fn shown(&self) -> String {
        format!("{}/<invite>", self.origin())
    }
}

impl fmt::Debug for InviteLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InviteLink({})", self.shown())
    }
}

impl fmt::Display for InviteLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.shown())
    }
}

/// A stored invite URL without its token, for `Debug` of types that keep it as a string.
pub fn shown_invite(invite: &str) -> String {
    InviteLink::parse(invite)
        .map(|l| l.shown())
        .unwrap_or_else(|_| "<invite>".into())
}

/// One row of the shared link table. `canonical` is `None` for input that must be refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InviteLinkCase {
    pub input: String,
    pub canonical: Option<String>,
}

/// The invite-link rule as data. The Rust tests and the UI's `isInviteLink` both run this
/// table; the contract test writes it to `ui/src/ipc/contract/invite_links.json`. Fake tokens
/// only.
pub fn invite_link_cases() -> Vec<InviteLinkCase> {
    let a = "A".repeat(TOKEN_LEN);
    let a42 = &a[..42];
    let mixed = "b4sE64url_-b4sE64url_-b4sE64url_-b4sE64url_";
    let ok = |input: String, canonical: String| InviteLinkCase {
        input,
        canonical: Some(canonical),
    };
    let bad = |input: String| InviteLinkCase {
        input,
        canonical: None,
    };
    // Under MAX_INPUT in UTF-16 units but over it in UTF-8 bytes: 7 labels of 40 `é` plus
    // `.com` is 290 chars of host (342 in all), 567 + 3 bytes of host (622 in all). The
    // length cap counts bytes, so a mirror that counts chars would disagree here.
    let long_idn_host = format!("{}com", format!("{}.", "é".repeat(40)).repeat(7));
    vec![
        ok(
            format!("https://play.example.com/{a}"),
            format!("https://play.example.com/{a}"),
        ),
        ok(
            format!("https://play.example.com/{a}/"),
            format!("https://play.example.com/{a}"),
        ),
        ok(
            format!("  https://PLAY.Example.COM/{mixed}  "),
            format!("https://play.example.com/{mixed}"),
        ),
        ok(
            format!("https://play.example.com:8443/{a}"),
            format!("https://play.example.com:8443/{a}"),
        ),
        ok(
            format!("https://play.example.com:443/{a}"),
            format!("https://play.example.com/{a}"),
        ),
        ok(
            format!("http://127.0.0.1:5173/{a}"),
            format!("http://127.0.0.1:5173/{a}"),
        ),
        ok(
            format!("http://localhost:5173/{a}"),
            format!("http://localhost:5173/{a}"),
        ),
        ok(
            format!("http://[::1]:5173/{a}"),
            format!("http://[::1]:5173/{a}"),
        ),
        bad(format!("http://play.example.com/{a}")),
        bad(format!("http://10.0.0.1/{a}")),
        bad(format!("ftp://play.example.com/{a}")),
        bad(format!("https://play.example.com/{a42}")),
        bad(format!("https://play.example.com/{a}A")),
        bad(format!("https://play.example.com/{a42}=")),
        bad(format!("https://play.example.com/{}%41", &a[..40])),
        bad(format!("https://play.example.com/{a}?ref=discord")),
        bad(format!("https://play.example.com/{a}?")),
        bad(format!("https://play.example.com/{a}#top")),
        bad(format!("https://play.example.com/{a}#")),
        bad(format!("https://player@play.example.com/{a}")),
        bad(format!("https://player:pw@play.example.com/{a}")),
        bad(format!("https://play.example.com/join/{a}")),
        bad(format!("https://play.example.com/{a}/pack")),
        bad(format!("https://play.example.com/{a}//")),
        bad(format!("https://play.example.com\\{a}")),
        bad(format!(
            "https://play.example.com/{} {}",
            &a[..21],
            &a[..21]
        )),
        bad(format!("play.example.com/{a}")),
        bad(format!(
            "perland://join?invite=https%3A%2F%2Fplay.example.com%2F{a}"
        )),
        bad("https://perlanddev.github.io/asalith-modpack/pack.toml".into()),
        bad("https://play.example.com/".into()),
        bad(String::new()),
        bad(format!("https://{long_idn_host}/{a}")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    #[test]
    fn the_shared_table_holds() {
        let cases = invite_link_cases();
        assert!(cases.iter().any(|c| c.canonical.is_some()));
        assert!(cases.iter().any(|c| c.canonical.is_none()));
        for case in cases {
            let got = InviteLink::parse(&case.input).ok().map(|l| l.canonical());
            assert_eq!(got, case.canonical, "{:?}", case.input);
        }
    }

    #[test]
    fn the_table_pins_the_cap_by_bytes_not_chars() {
        let pinned = invite_link_cases()
            .into_iter()
            .filter(|c| c.canonical.is_none())
            .filter(|c| c.input.encode_utf16().count() < MAX_INPUT && c.input.len() > MAX_INPUT)
            .count();
        assert_eq!(pinned, 1);
    }

    #[test]
    fn refusals_are_invite_link_invalid() {
        assert_eq!(
            InviteLink::parse("nope").unwrap_err().code,
            ErrorCode::InviteLinkInvalid
        );
    }

    /// Fuzzing found a link of 512 bytes whose host IDNA turns into 466 characters: its
    /// canonical form was too long to parse again. No real host is longer than 253.
    #[test]
    fn a_host_longer_than_a_dns_name_is_refused() {
        let token = "A".repeat(TOKEN_LEN);
        let fits = format!("{}.example", "a".repeat(245));
        assert_eq!(fits.len(), 253);
        assert!(InviteLink::parse(&format!("https://{fits}/{token}")).is_ok());
        let long = format!("{}.example", "a".repeat(246));
        assert!(InviteLink::parse(&format!("https://{long}/{token}")).is_err());
        // A short raw host IDNA makes too long.
        let unicode = format!("{}.example", "ñ".repeat(60));
        assert!(unicode.len() < 253);
        if let Ok(link) = InviteLink::parse(&format!("https://{unicode}/{token}")) {
            assert!(link.host().len() <= 253);
        }
    }

    #[test]
    fn every_canonical_form_parses_again() {
        let token = "A".repeat(TOKEN_LEN);
        let fuzzed = format!(
            "https:3zVVz1111VVVV5x505{}ˀxxxxxx5555{}hR*{}..00006171t{}(463971{}ˀx{}w{}(00003085{}..0000617140863971xxxx5555555555VVVVVVVV-VV:/0VViiVVVVVViiV-kVVVVVVVVVV_VVVVVVV_VVVVVVVV",
            "x".repeat(7) + "5555" + &"1".repeat(7) + "7579240863971xxxx" + &"5".repeat(14) + "x50",
            "1".repeat(18),
            "1".repeat(24),
            "1".repeat(5) + "0000031857775792",
            "xxxx".to_string() + &"5".repeat(14) + "x505xxxxxxx5555" + &"1".repeat(7) + "7579240863971xxxx" + &"5".repeat(14) + "x50",
            "78737961038281111",
            "w".repeat(118) + &"1".repeat(7),
            "5589368980519139",
        );
        for input in [
            fuzzed,
            format!("https://{}.example/{token}", "ñ".repeat(60)),
        ] {
            if let Ok(link) = InviteLink::parse(&input) {
                let again = InviteLink::parse(&link.canonical());
                assert_eq!(again.ok(), Some(link), "{input:?}");
            }
        }
    }

    #[test]
    fn debug_and_display_never_print_the_token() {
        let token = "A".repeat(TOKEN_LEN);
        let link = InviteLink::parse(&format!("https://play.example.com/{token}")).unwrap();
        for text in [
            format!("{link:?}"),
            link.to_string(),
            link.shown(),
            shown_invite(&link.canonical()),
        ] {
            assert!(!text.contains(&token), "{text}");
            assert!(text.contains("play.example.com"), "{text}");
        }
        assert_eq!(link.token(), token);
        assert_eq!(link.origin(), "https://play.example.com");
        assert_eq!(link.host(), "play.example.com");
        assert_eq!(shown_invite("junk"), "<invite>");
    }

    #[test]
    fn spellings_of_one_link_share_a_canonical_form() {
        let token = "A".repeat(TOKEN_LEN);
        let a = InviteLink::parse(&format!("https://PLAY.example.com/{token}/")).unwrap();
        let b = InviteLink::parse(&format!(" https://play.example.com:443/{token} ")).unwrap();
        assert_eq!(a.canonical(), b.canonical());
        assert_eq!(a, b);
    }
}

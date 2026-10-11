//! `perland://pack?url=<https pack.toml>` (core spec §5). The link is remote input: only an
//! https packwiz link gets through, and the player still clicks Add pack before anything
//! downloads.
use launcher_core::{sources::PackInput, ErrorCode, LauncherError, Result};

fn invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackLinkInvalid,
        "That doesn't look like a pack link",
    )
    .with_fix("Pack links start with https:// and usually end in pack.toml")
}

/// The https `pack.toml` URL inside a `perland://pack?url=…` link.
pub fn parse(raw: &str) -> Result<String> {
    let link = url::Url::parse(raw.trim()).map_err(|_| invalid())?;
    if link.scheme() != "perland"
        || link.host_str() != Some("pack")
        || !matches!(link.path(), "" | "/")
    {
        return Err(invalid());
    }
    let mut urls = link
        .query_pairs()
        .filter(|(key, _)| key == "url")
        .map(|(_, value)| value.into_owned());
    let (Some(pack), None) = (urls.next(), urls.next()) else {
        return Err(invalid());
    };
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
            assert_eq!(parse(good).unwrap(), asalith, "{good}");
        }
        for bad in [
            "perland://pack?url=http://x.example/pack.toml",
            "perland://pack?url=http://localhost:8080/pack.toml",
            "perland://pack?url=C:%5CUsers%5Cme%5Cevil.mrpack",
            "perland://pack?url=modrinth:sky",
            "perland://pack?url=https://a.example/pack.toml&url=https://b.example/pack.toml",
            "perland://pack",
            "perland://join?url=https://a.example/pack.toml",
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
}

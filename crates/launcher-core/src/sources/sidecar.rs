//! `launcher.toml`, the optional file next to `pack.toml` that gives a packwiz pack art, a
//! description and a default update policy (spec §3.5, addendum §3.1).
use serde::Deserialize;

use super::{PackAbout, PackArt};
use crate::instances::UpdatePolicy;

#[derive(Deserialize, Default)]
#[serde(default)]
struct Raw {
    art: RawArt,
    about: RawAbout,
    sync: RawSync,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawArt {
    icon: Option<String>,
    banner: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAbout {
    description: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSync {
    policy: Option<String>,
}

/// What `launcher.toml` says. It never fails: an unreadable file is the same as no file, and
/// the launcher falls back to its generated art.
pub fn parse_sidecar(text: &str, pack_url: &str) -> PackAbout {
    let Ok(raw) = toml::from_str::<Raw>(text) else {
        return PackAbout::default();
    };
    let base = reqwest::Url::parse(pack_url).ok();
    let art = |value: Option<String>| value.and_then(|v| resolve_art(base.as_ref()?, &v));
    let description = raw
        .about
        .description
        .map(|d| d.trim().chars().take(500).collect::<String>())
        .filter(|d| !d.is_empty());
    let policy = match raw.sync.policy.as_deref() {
        Some("auto") => Some(UpdatePolicy::Auto),
        Some("ask") => Some(UpdatePolicy::Ask),
        _ => None,
    };
    PackAbout {
        description,
        art: PackArt {
            icon: art(raw.art.icon),
            banner: art(raw.art.banner),
        },
        policy,
    }
}

/// An art reference as an absolute URL: relative paths resolve against `pack.toml`. Only https,
/// or plain http on the pack's own (local) host.
fn resolve_art(base: &reqwest::Url, value: &str) -> Option<String> {
    let url = match reqwest::Url::parse(value) {
        Ok(abs) => abs,
        Err(_) => base.join(value).ok()?,
    };
    let same_host = url.host_str() == base.host_str();
    match url.scheme() {
        "https" => Some(url.to_string()),
        "http" if base.scheme() == "http" && same_host => Some(url.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::UpdatePolicy;

    const PACK: &str = "https://perlanddev.github.io/asalith-modpack/pack.toml";

    #[test]
    fn sidecar_art_resolves_against_the_pack() {
        let about = parse_sidecar(
            "[art]\nicon = \"assets/favicon.svg\"\nbanner = \"https://cdn.example.com/b.png\"\n\
             [about]\ndescription = \"Official Asalith server modpack\"\n[sync]\npolicy = \"auto\"\n\
             [future]\nunknown = 1\n",
            PACK,
        );
        assert_eq!(
            about.art.icon.as_deref(),
            Some("https://perlanddev.github.io/asalith-modpack/assets/favicon.svg")
        );
        assert_eq!(
            about.art.banner.as_deref(),
            Some("https://cdn.example.com/b.png")
        );
        assert_eq!(
            about.description.as_deref(),
            Some("Official Asalith server modpack")
        );
        assert_eq!(about.policy, Some(UpdatePolicy::Auto));
    }

    #[test]
    fn sidecar_refuses_other_schemes_and_ignores_garbage() {
        let about = parse_sidecar(
            "[art]\nicon = \"javascript:alert(1)\"\nbanner = \"file:///C:/x.png\"\n[sync]\npolicy = \"pinned\"\n",
            PACK,
        );
        assert_eq!(about, PackAbout::default());
        let http = parse_sidecar("[art]\nicon = \"http://evil.example/x.png\"\n", PACK);
        assert_eq!(
            http.art.icon, None,
            "an https pack never pulls art over plain http"
        );
        assert_eq!(parse_sidecar("not toml [", PACK), PackAbout::default());
    }
}

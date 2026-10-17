//! The Perland panel, where a player manages the servers they own (free players in the panel,
//! Part 5): the launcher only opens its pages in the browser.
use crate::invite::{check_engine_url, Url};

/// A panel the launcher will open: https (or http on this computer), with a host and no user
/// info, query or fragment. A path is kept, so a panel under one works.
pub fn panel_url(raw: &str) -> Option<Url> {
    check_engine_url(raw)
}

/// `<panel>/stacks/<stack>`: the panel's page for one server. `stack` is one path segment,
/// escaped, so a name can never reach another page.
pub fn manage_url(panel: &Url, stack: &str) -> Url {
    let mut url = panel.clone();
    url.path_segments_mut()
        .expect("a checked panel URL has a host, so it can have a path")
        .pop_if_empty()
        .push("stacks")
        .push(stack);
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_url_takes_https_and_loopback_only() {
        assert!(panel_url("https://panel.example.com").is_some());
        assert!(panel_url(" https://panel.example.com/ ").is_some());
        assert!(panel_url("http://127.0.0.1:5173").is_some());
        for bad in [
            "",
            "panel.example.com",
            "http://panel.example.com",
            "https://user@panel.example.com",
            "https://panel.example.com/?a=1",
            "https://panel.example.com/#x",
            "file:///etc/passwd",
        ] {
            assert!(panel_url(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn manage_url_is_the_stack_page() {
        let panel = panel_url("https://panel.example.com").unwrap();
        assert_eq!(
            manage_url(&panel, "survival").as_str(),
            "https://panel.example.com/stacks/survival"
        );
        let slash = panel_url("https://panel.example.com/").unwrap();
        assert_eq!(
            manage_url(&slash, "survival").as_str(),
            "https://panel.example.com/stacks/survival"
        );
    }

    #[test]
    fn manage_url_keeps_the_panel_path() {
        let panel = panel_url("https://example.com/panel/").unwrap();
        assert_eq!(
            manage_url(&panel, "survival").as_str(),
            "https://example.com/panel/stacks/survival"
        );
    }

    #[test]
    fn manage_url_escapes_the_name_as_one_segment() {
        let panel = panel_url("https://panel.example.com").unwrap();
        assert_eq!(
            manage_url(&panel, "../admin?x#y").as_str(),
            "https://panel.example.com/stacks/..%2Fadmin%3Fx%23y"
        );
        assert_eq!(
            manage_url(&panel, "my world").as_str(),
            "https://panel.example.com/stacks/my%20world"
        );
    }
}

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

/// The panel page that upgrades a team's plan.
pub fn upgrade_path() -> &'static str {
    "/settings/upgrade"
}

/// `/stacks/<stack>`: the panel path of one server, with `stack` escaped as one segment (as
/// [`manage_url`] does).
pub fn server_path(stack: &str) -> String {
    let mut url = Url::parse("http://x/").expect("a fixed URL");
    url.path_segments_mut()
        .expect("a URL with a host has a path")
        .pop_if_empty()
        .push("stacks")
        .push(stack);
    url.path().to_string()
}

/// The panel's page at `next` (a path such as [`server_path`] or [`upgrade_path`] gives),
/// under the panel's own path.
pub fn panel_page(panel: &Url, next: &str) -> Url {
    let mut url = panel.clone();
    let base = panel.path().trim_end_matches('/');
    url.set_path(&format!("{base}/{}", next.trim_start_matches('/')));
    url
}

/// `<panel>/login/handoff?code=&next=`: signs the player in on the panel with a one-time
/// `code`, then shows `next`. The URL holds a credential: never log it.
pub fn handoff_url(panel: &Url, code: &str, next: &str) -> Url {
    let mut url = panel_page(panel, "/login/handoff");
    url.query_pairs_mut()
        .append_pair("code", code)
        .append_pair("next", next);
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
    fn handoff_url_escapes_code_and_next() {
        let panel = panel_url("https://example.com/panel/").unwrap();
        let url = handoff_url(&panel, "a b&c=d#e", &server_path("my world"));
        assert_eq!(url.path(), "/panel/login/handoff");
        assert_eq!(url.fragment(), None);
        let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        assert_eq!(
            pairs,
            [
                ("code".to_string(), "a b&c=d#e".to_string()),
                ("next".to_string(), "/stacks/my%20world".to_string()),
            ]
        );
        let root = panel_url("https://panel.example.com").unwrap();
        assert_eq!(
            handoff_url(&root, "abc", upgrade_path()).as_str(),
            "https://panel.example.com/login/handoff?code=abc&next=%2Fsettings%2Fupgrade"
        );
    }

    #[test]
    fn server_path_escapes_one_segment() {
        assert_eq!(server_path("survival"), "/stacks/survival");
        assert_eq!(server_path("my world"), "/stacks/my%20world");
        assert_eq!(server_path("../admin?x#y"), "/stacks/..%2Fadmin%3Fx%23y");
        assert_eq!(upgrade_path(), "/settings/upgrade");
    }

    #[test]
    fn panel_page_is_the_path_under_the_panel() {
        let panel = panel_url("https://example.com/panel/").unwrap();
        assert_eq!(
            panel_page(&panel, &server_path("my world")).as_str(),
            "https://example.com/panel/stacks/my%20world"
        );
        let root = panel_url("https://panel.example.com").unwrap();
        assert_eq!(
            panel_page(&root, upgrade_path()).as_str(),
            "https://panel.example.com/settings/upgrade"
        );
        assert_eq!(
            panel_page(&root, &server_path("../admin?x#y")).as_str(),
            manage_url(&root, "../admin?x#y").as_str()
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

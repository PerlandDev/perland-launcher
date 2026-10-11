//! The one HTTP client setup every launcher request goes through.
use std::time::Duration;

use crate::error::{ErrorCode, LauncherError, Result};

/// The descriptive `User-Agent` sent with every request.
pub fn user_agent(launcher_version: &str) -> String {
    format!("PerlandLauncher/{launcher_version} (+https://github.com/PerlandDev/perland-launcher)")
}

/// A client with the launcher's user agent and timeouts, so a stalled connection fails instead of
/// hanging forever. The read timeout applies per read, so large downloads still work.
pub fn client(launcher_version: &str) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(user_agent(launcher_version))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let from = attempt.previous().last().cloned();
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if from.is_some_and(|from| !allow_redirect(&from, attempt.url())) {
                attempt.error("refused a redirect from https to plain http")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| {
            LauncherError::network(
                ErrorCode::HttpClientFailed,
                "Couldn't start the HTTP client",
            )
            .with_detail(e)
        })
}

/// Redirects followed per request, like reqwest's default.
const MAX_REDIRECTS: usize = 10;

/// Is `url` on this computer: `localhost`, 127.0.0.0/8 or `[::1]`?
pub fn is_loopback(url: &reqwest::Url) -> bool {
    let host = url.host_str().unwrap_or("");
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// May a request for `from` follow a redirect to `to`? Only to http(s), and never from https
/// down to plain http, except to this computer (tests and local pack development): a pack,
/// its index and the Modrinth API answers are where every download's hash comes from.
pub fn allow_redirect(from: &reqwest::Url, to: &reqwest::Url) -> bool {
    match (from.scheme(), to.scheme()) {
        (_, "https") => true,
        ("https", "http") => is_loopback(to),
        (_, "http") => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_names_the_launcher() {
        assert_eq!(
            user_agent("0.1.0"),
            "PerlandLauncher/0.1.0 (+https://github.com/PerlandDev/perland-launcher)"
        );
    }

    #[test]
    fn client_builds() {
        assert!(client("0.1.0").is_ok());
    }

    fn url(s: &str) -> reqwest::Url {
        reqwest::Url::parse(s).unwrap()
    }

    #[test]
    fn redirects_never_leave_https_for_the_network() {
        for (from, to, allowed) in [
            ("https://a.example/p", "https://b.example/q", true),
            ("http://127.0.0.1:1/p", "http://127.0.0.1:2/q", true),
            ("http://a.example/p", "https://b.example/q", true),
            ("https://a.example/p", "http://b.example/q", false),
            ("https://a.example/p", "http://localhost:8080/q", true),
            ("https://a.example/p", "http://127.0.0.2/q", true),
            ("https://a.example/p", "http://[::1]/q", true),
            ("https://a.example/p", "http://10.0.0.1/q", false),
            (
                "https://a.example/p",
                "http://localhost.evil.example/q",
                false,
            ),
            ("https://a.example/p", "ftp://a.example/q", false),
        ] {
            assert_eq!(
                allow_redirect(&url(from), &url(to)),
                allowed,
                "{from} -> {to}"
            );
        }
    }

    #[tokio::test]
    async fn loopback_redirects_are_still_followed() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/old"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/new"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/new"))
            .respond_with(ResponseTemplate::new(200).set_body_string("moved"))
            .mount(&server)
            .await;
        let body = client("test")
            .unwrap()
            .get(format!("{}/old", server.uri()))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(body, "moved");

        // Still at most ten hops.
        Mock::given(method("GET"))
            .and(path("/loop"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/loop"))
            .mount(&server)
            .await;
        let looped = client("test")
            .unwrap()
            .get(format!("{}/loop", server.uri()))
            .send()
            .await;
        assert!(looped.unwrap_err().is_redirect());
    }
}

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
    build(
        launcher_version,
        reqwest::redirect::Policy::custom(|attempt| {
            // `previous` holds the original URL and every redirect followed so far.
            if attempt.previous().len() > MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if !allow_redirect_chain(attempt.previous(), attempt.url()) {
                attempt.error("refused a redirect from https to plain http")
            } else {
                attempt.follow()
            }
        }),
    )
}

/// Like `client`, but never follows a redirect: a 3xx comes back as an ordinary response. For
/// fetches that must stay on one host, such as images from the CDN.
pub fn client_without_redirects(launcher_version: &str) -> Result<reqwest::Client> {
    build(launcher_version, reqwest::redirect::Policy::none())
}

fn build(launcher_version: &str, redirect: reqwest::redirect::Policy) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(user_agent(launcher_version))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .redirect(redirect)
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

/// May a request that has visited `previous` (oldest first) follow a redirect to `to`? Each hop
/// must pass `allow_redirect`, and once any URL in the chain was https, plain http is allowed
/// only to this computer: a detour through a loopback http hop doesn't reopen the network.
pub fn allow_redirect_chain(previous: &[reqwest::Url], to: &reqwest::Url) -> bool {
    if previous
        .last()
        .is_some_and(|last| !allow_redirect(last, to))
    {
        return false;
    }
    let was_secure = previous.iter().any(|u| u.scheme() == "https");
    !(was_secure && to.scheme() == "http" && !is_loopback(to))
}

/// At most `cap` bytes of the body; `None` when it is longer.
pub async fn read_capped(
    mut response: reqwest::Response,
    cap: usize,
) -> std::result::Result<Option<Vec<u8>>, reqwest::Error> {
    if response.content_length().is_some_and(|n| n > cap as u64) {
        return Ok(None);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        body.extend_from_slice(&chunk);
        if body.len() > cap {
            return Ok(None);
        }
    }
    Ok(Some(body))
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

    #[tokio::test]
    async fn the_no_redirect_client_hands_back_the_302() {
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
        let response = client_without_redirects("test")
            .unwrap()
            .get(format!("{}/old", server.uri()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 302);
        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1);
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

    #[test]
    fn a_chain_that_was_on_https_never_drops_to_network_http() {
        // https → loopback http is fine, but the next hop to the network must not be plain http.
        let chain = [url("https://a.example/p"), url("http://localhost:1/q")];
        assert!(!allow_redirect_chain(&chain, &url("http://b.example/r")));
        assert!(allow_redirect_chain(&chain, &url("http://127.0.0.1:2/r")));
        assert!(allow_redirect_chain(&chain, &url("https://b.example/r")));
        let plain = [url("http://a.example/p")];
        assert!(allow_redirect_chain(&plain, &url("http://b.example/q")));
        assert!(!allow_redirect_chain(
            &[url("https://a.example/p")],
            &url("ftp://a.example/q")
        ));
    }

    #[tokio::test]
    async fn ten_redirects_are_followed_and_the_eleventh_is_refused() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        for n in 0..11 {
            Mock::given(method("GET"))
                .and(path(format!("/hop/{n}")))
                .respond_with(
                    ResponseTemplate::new(302).insert_header("location", format!("/hop/{}", n + 1)),
                )
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/hop/11"))
            .respond_with(ResponseTemplate::new(200).set_body_string("end"))
            .mount(&server)
            .await;
        let client = client("test").unwrap();
        // /hop/1 → … → /hop/11 is ten redirects.
        let body = client
            .get(format!("{}/hop/1", server.uri()))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(body, "end");
        // /hop/0 → … → /hop/11 is eleven.
        let err = client
            .get(format!("{}/hop/0", server.uri()))
            .send()
            .await
            .unwrap_err();
        assert!(err.is_redirect());
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

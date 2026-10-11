//! Pack art for the app. It's downloaded into the instance because the app's CSP allows no
//! remote images. Only real images under a size cap are kept, and anything else leaves the
//! generated art in place.
use std::path::Path;

use super::{check_pack_url, PackArt};
use crate::fsutil::write_atomic;

pub const MAX_ART_BYTES: usize = 5 << 20;
const EXTENSIONS: [&str; 4] = ["png", "jpg", "webp", "svg"];

/// The image type, by signature. SVG has none, so it must start like XML or `<svg`.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    let start = head.trim_start_matches('\u{feff}').trim_start();
    ((start.starts_with("<svg") || start.starts_with("<?xml")) && head.contains("<svg"))
        .then_some("svg")
}

pub async fn fetch(http: &reqwest::Client, art: &PackArt, dir: &Path) {
    fetch_limited(http, art, dir, MAX_ART_BYTES).await
}

/// Save `art` as `icon.<ext>` and `banner.<ext>` in `dir`. Never fails.
pub async fn fetch_limited(http: &reqwest::Client, art: &PackArt, dir: &Path, limit: usize) {
    for (stem, url) in [("icon", &art.icon), ("banner", &art.banner)] {
        let Some(url) = url else { continue };
        let Some((bytes, ext)) = download(http, url, limit).await else {
            continue;
        };
        if write_atomic(&dir.join(format!("{stem}.{ext}")), &bytes)
            .await
            .is_err()
        {
            continue;
        }
        for other in EXTENSIONS.iter().filter(|e| **e != ext) {
            let _ = tokio::fs::remove_file(dir.join(format!("{stem}.{other}"))).await;
        }
    }
}

async fn download(
    http: &reqwest::Client,
    url: &str,
    limit: usize,
) -> Option<(Vec<u8>, &'static str)> {
    check_pack_url(url).ok()?;
    let mut response = http.get(url).send().await.ok()?;
    if !response.status().is_success()
        || response.content_length().is_some_and(|n| n > limit as u64)
    {
        return None;
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        body.extend_from_slice(&chunk);
        if body.len() > limit {
            return None;
        }
    }
    let ext = sniff(&body)?;
    Some((body, ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn sniffs_only_images() {
        assert_eq!(sniff(PNG), Some("png"));
        assert_eq!(sniff(b"\xff\xd8\xff\xe0rest"), Some("jpg"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(
            sniff(b"<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            Some("svg")
        );
        assert_eq!(sniff(b"<!doctype html><html>"), None);
        assert_eq!(sniff(b""), None);
    }

    async fn serve(server: &MockServer, at: &str, body: &[u8]) {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn fetches_images_and_ignores_everything_else() {
        let server = MockServer::start().await;
        serve(&server, "/icon.png", PNG).await;
        serve(&server, "/banner", &b"<html>login page</html>"[..]).await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("icon.svg"), "<svg/>").unwrap();
        let art = PackArt {
            icon: Some(format!("{}/icon.png", server.uri())),
            banner: Some(format!("{}/banner", server.uri())),
        };
        fetch(&reqwest::Client::new(), &art, dir.path()).await;
        assert_eq!(std::fs::read(dir.path().join("icon.png")).unwrap(), PNG);
        assert!(
            !dir.path().join("icon.svg").exists(),
            "the old icon is replaced"
        );
        assert!(std::fs::read_dir(dir.path()).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("banner")
        }));

        let big = MockServer::start().await;
        serve(&big, "/icon.png", &[PNG, &[0u8; 64]].concat()).await;
        let small = tempfile::tempdir().unwrap();
        let art = PackArt {
            icon: Some(format!("{}/icon.png", big.uri())),
            banner: None,
        };
        fetch_limited(&reqwest::Client::new(), &art, small.path(), 32).await;
        assert!(!small.path().join("icon.png").exists(), "over the limit");
    }
}

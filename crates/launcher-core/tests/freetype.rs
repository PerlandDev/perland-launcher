//! Swapping in the patched FreeType native before a launch: it is downloaded and checked first,
//! and a launch is never held up when it can't be had. Served by wiremock.
use std::time::Duration;

use launcher_core::{
    download::{hash::sha1_hex, Downloader},
    install::{
        freetype::{is_vulnerable, secure, PatchedNative, PatchedNatives, MOJANG},
        Installer,
    },
    meta::{
        maven::MavenCoord,
        mojang::{VersionJson, VersionManifest, VERSION_MANIFEST_URL},
        rules::{library_applies, RuleContext},
        MetaClient,
    },
    paths::LauncherDirs,
    platform::{Arch, Os, Platform},
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const LINUX_X64: Platform = Platform {
    os: Os::Linux,
    arch: Arch::X64,
};
const LINUX_JAR: &str =
    "/org/lwjgl/lwjgl-freetype/9.9.9-test/lwjgl-freetype-9.9.9-test-natives-linux.jar";
const WINDOWS_JAR: &str =
    "/org/lwjgl/lwjgl-freetype/9.9.9-test/lwjgl-freetype-9.9.9-test-natives-windows.jar";
const LINUX_BYTES: &[u8] = b"the patched linux native";
const WINDOWS_BYTES: &[u8] = b"the patched windows native";

struct Env {
    _tmp: tempfile::TempDir,
    dirs: LauncherDirs,
    downloader: Downloader,
    linux_sha1: String,
    windows_sha1: String,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    Env {
        dirs: LauncherDirs::new(tmp.path()),
        downloader: Downloader::new(http).with_backoff(Duration::from_millis(1)),
        linux_sha1: sha1_hex(LINUX_BYTES),
        windows_sha1: sha1_hex(WINDOWS_BYTES),
        _tmp: tmp,
    }
}

impl Env {
    fn natives(&self) -> [PatchedNative<'_>; 2] {
        [
            PatchedNative {
                classifier: "natives-linux",
                sha1: &self.linux_sha1,
                size: LINUX_BYTES.len() as u64,
            },
            PatchedNative {
                classifier: "natives-windows",
                sha1: &self.windows_sha1,
                size: WINDOWS_BYTES.len() as u64,
            },
        ]
    }

    fn linux_jar(&self) -> std::path::PathBuf {
        self.dirs.libraries().join(&LINUX_JAR[1..])
    }
}

fn set<'a>(uri: &'a str, natives: &'a [PatchedNative<'a>]) -> PatchedNatives<'a> {
    PatchedNatives {
        version: "9.9.9-test",
        libraries_url: uri,
        natives,
    }
}

fn vulnerable() -> VersionJson {
    serde_json::from_str(include_str!("fixtures/freetype-1.21.11.json")).unwrap()
}

fn names(json: &VersionJson) -> Vec<&str> {
    json.libraries.iter().map(|l| l.name.as_str()).collect()
}

async fn serve(server: &MockServer, at: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

#[tokio::test]
async fn the_patched_native_is_downloaded_and_used() {
    let server = MockServer::start().await;
    serve(&server, LINUX_JAR, LINUX_BYTES).await;
    serve(&server, WINDOWS_JAR, WINDOWS_BYTES).await;
    let e = env();
    let uri = server.uri();
    let natives = e.natives();
    let json = secure(
        vulnerable(),
        &set(&uri, &natives),
        &e.downloader,
        &e.dirs,
        LINUX_X64,
    )
    .await;
    assert!(names(&json).contains(&"org.lwjgl:lwjgl-freetype:9.9.9-test:natives-linux"));
    assert!(!names(&json).contains(&"org.lwjgl:lwjgl-freetype:3.3.3:natives-linux"));
    assert_eq!(std::fs::read(e.linux_jar()).unwrap(), LINUX_BYTES);
}

#[tokio::test]
async fn only_this_computers_native_is_downloaded() {
    let server = MockServer::start().await;
    serve(&server, LINUX_JAR, LINUX_BYTES).await;
    serve(&server, WINDOWS_JAR, WINDOWS_BYTES).await;
    let e = env();
    let uri = server.uri();
    let natives = e.natives();
    secure(
        vulnerable(),
        &set(&uri, &natives),
        &e.downloader,
        &e.dirs,
        LINUX_X64,
    )
    .await;
    let asked: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_string())
        .collect();
    assert_eq!(asked, vec![LINUX_JAR]);
}

#[tokio::test]
async fn a_native_that_cant_be_downloaded_leaves_the_version_as_it_was() {
    let server = MockServer::start().await; // answers 404 to everything
    let e = env();
    let uri = server.uri();
    let natives = e.natives();
    let json = secure(
        vulnerable(),
        &set(&uri, &natives),
        &e.downloader,
        &e.dirs,
        LINUX_X64,
    )
    .await;
    assert_eq!(json, vulnerable(), "the game still starts, as before");
}

/// A launch is not held up by retries: one try, and the next launch tries again.
#[tokio::test]
async fn a_host_in_trouble_is_asked_only_once() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let e = env();
    let uri = server.uri();
    let natives = e.natives();
    let json = secure(
        vulnerable(),
        &set(&uri, &natives),
        &e.downloader,
        &e.dirs,
        LINUX_X64,
    )
    .await;
    assert_eq!(json, vulnerable());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_native_with_the_wrong_checksum_is_never_used() {
    let server = MockServer::start().await;
    serve(&server, LINUX_JAR, b"not the file Mojang published").await;
    let e = env();
    let uri = server.uri();
    let natives = e.natives();
    let json = secure(
        vulnerable(),
        &set(&uri, &natives),
        &e.downloader,
        &e.dirs,
        LINUX_X64,
    )
    .await;
    assert_eq!(json, vulnerable());
    assert!(!e.linux_jar().exists(), "the bad file is not kept");
}

#[tokio::test]
async fn a_native_already_on_disk_needs_no_network() {
    let server = MockServer::start().await; // answers 404 to everything
    let e = env();
    std::fs::create_dir_all(e.linux_jar().parent().unwrap()).unwrap();
    std::fs::write(e.linux_jar(), LINUX_BYTES).unwrap();
    let uri = server.uri();
    let natives = e.natives();
    let json = secure(
        vulnerable(),
        &set(&uri, &natives),
        &e.downloader,
        &e.dirs,
        LINUX_X64,
    )
    .await;
    assert!(names(&json).contains(&"org.lwjgl:lwjgl-freetype:9.9.9-test:natives-linux"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_version_mojang_already_patched_downloads_nothing() {
    let server = MockServer::start().await;
    let e = env();
    let uri = server.uri();
    let natives = e.natives();
    let patched: VersionJson =
        serde_json::from_str(include_str!("fixtures/freetype-26.3.json")).unwrap();
    let json = secure(
        patched.clone(),
        &set(&uri, &natives),
        &e.downloader,
        &e.dirs,
        LINUX_X64,
    )
    .await;
    assert_eq!(json, patched);
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A Mojang manifest that lists `id` at `/v/<name>.json`, with that file's checksum.
async fn serve_manifest(server: &MockServer, id: &str, name: &str, version_json: &str) {
    let manifest = serde_json::json!({
        "latest": {"release": id, "snapshot": id},
        "versions": [{
            "id": id,
            "type": "release",
            "url": format!("{}/v/{name}.json", server.uri()),
            "sha1": sha1_hex(version_json.as_bytes()),
        }],
    });
    Mock::given(method("GET"))
        .and(path("/manifest.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(manifest))
        .mount(server)
        .await;
    serve(server, &format!("/v/{name}.json"), version_json.as_bytes()).await;
}

fn installer(e: &Env) -> Installer {
    Installer {
        dirs: e.dirs.clone(),
        meta: MetaClient::new(reqwest::Client::new(), e.dirs.meta()),
        downloader: e.downloader.clone(),
        platform: LINUX_X64,
    }
}

/// What every launch goes through: the version Play resolves is already the secured one.
#[tokio::test]
async fn resolving_a_vulnerable_version_swaps_in_the_patched_native() {
    let server = MockServer::start().await;
    serve_manifest(
        &server,
        "1.21.11",
        "old",
        include_str!("fixtures/freetype-1.21.11.json"),
    )
    .await;
    serve(&server, LINUX_JAR, LINUX_BYTES).await;
    let e = env();
    let uri = server.uri();
    let natives = e.natives();
    let resolved = installer(&e)
        .resolve_at(
            &format!("{uri}/manifest.json"),
            &set(&uri, &natives),
            "1.21.11",
            None,
        )
        .await
        .unwrap();
    assert!(names(&resolved.json).contains(&"org.lwjgl:lwjgl-freetype:9.9.9-test:natives-linux"));
    assert!(!names(&resolved.json).contains(&"org.lwjgl:lwjgl-freetype:3.3.3:natives-linux"));
}

/// Mojang's own fix (26.1 and later): the version file is republished under a new address. A
/// copy of the old one saved by an earlier launch must not win.
#[tokio::test]
async fn a_version_mojang_republishes_replaces_the_one_saved_earlier() {
    let old = include_str!("fixtures/freetype-26.3.json").replace("-mojang-2.15.0", "");
    let e = env();
    let natives = e.natives();
    {
        let before = MockServer::start().await;
        serve_manifest(&before, "26.3", "old", &old).await;
        let uri = before.uri();
        // No patched native is served: the earlier launch ran with the old library.
        let resolved = installer(&e)
            .resolve_at(
                &format!("{uri}/manifest.json"),
                &set(&uri, &natives),
                "26.3",
                None,
            )
            .await
            .unwrap();
        assert!(names(&resolved.json).contains(&"org.lwjgl:lwjgl-freetype:3.4.3:natives-linux"));
    }
    let after = MockServer::start().await;
    serve_manifest(
        &after,
        "26.3",
        "republished",
        include_str!("fixtures/freetype-26.3.json"),
    )
    .await;
    let uri = after.uri();
    let resolved = installer(&e)
        .resolve_at(
            &format!("{uri}/manifest.json"),
            &set(&uri, &natives),
            "26.3",
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        names(&resolved.json)
            .into_iter()
            .filter(|n| n.ends_with("natives-linux") || !n.contains("natives"))
            .collect::<Vec<_>>(),
        vec![
            "org.lwjgl:lwjgl-freetype:3.4.3-mojang-2.15.0",
            "org.lwjgl:lwjgl-freetype:3.4.3-mojang-2.15.0:natives-linux",
        ]
    );
}

/// The `lwjgl-freetype` native versions `json` puts on this computer's classpath.
fn freetype_natives(json: &VersionJson, platform: Platform) -> Vec<String> {
    let ctx = RuleContext::new(platform);
    json.libraries
        .iter()
        .filter(|l| library_applies(l, &ctx))
        .filter_map(|l| MavenCoord::parse(&l.name))
        .filter(|c| c.artifact == "lwjgl-freetype" && c.classifier.is_some())
        .map(|c| c.version)
        .collect()
}

/// The real thing, against Mojang's servers: every release since FreeType came in (1.20.5),
/// resolved the way Play resolves it, for every computer the launcher runs on. None may start
/// with a vulnerable FreeType, and each patched native on disk must be the file Mojang published.
///
/// `cargo test -p launcher-core --test freetype -- --ignored --nocapture`
#[tokio::test]
#[ignore = "talks to Mojang's servers; run by hand and nightly"]
async fn live_no_release_starts_with_a_vulnerable_freetype() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = LauncherDirs::new(tmp.path());
    // The launcher's own client settings; a proxy's certificate is trusted when one is configured.
    let mut http = reqwest::Client::builder().timeout(Duration::from_secs(60));
    if let Some(pem) = std::env::var_os("SSL_CERT_FILE").and_then(|p| std::fs::read(p).ok()) {
        for cert in reqwest::Certificate::from_pem_bundle(&pem).unwrap() {
            http = http.add_root_certificate(cert);
        }
    }
    let http = http.build().unwrap();
    let meta = MetaClient::new(http.clone(), dirs.meta());
    let manifest: VersionManifest = meta.get_json(VERSION_MANIFEST_URL).await.unwrap();
    // Newest first. 1.20.4 is the last release without FreeType: the control.
    let last = manifest
        .versions
        .iter()
        .position(|v| v.id == "1.20.4")
        .expect("1.20.4 is in the manifest");
    let mut ids: Vec<&str> = manifest.versions[..=last]
        .iter()
        .filter(|v| v.kind == "release")
        .map(|v| v.id.as_str())
        .collect();
    ids.insert(0, &manifest.latest.snapshot);
    let platforms = [
        (Os::Windows, Arch::X64),
        (Os::Windows, Arch::X86),
        (Os::Windows, Arch::Arm64),
        (Os::Linux, Arch::X64),
        (Os::MacOs, Arch::X64),
        (Os::MacOs, Arch::Arm64),
    ];
    println!(
        "{:<16} {:<24} the launcher starts the game with",
        "Minecraft", "Mojang lists"
    );
    let mut swapped = 0;
    for id in ids {
        let entry = manifest.find(id).unwrap();
        let listed: VersionJson = meta
            .get_verified_json(&entry.url, &entry.sha1)
            .await
            .unwrap();
        let mut listed_versions = Vec::new();
        let mut used_versions = Vec::new();
        for (os, arch) in platforms {
            let platform = Platform { os, arch };
            let installer = Installer {
                dirs: dirs.clone(),
                meta: meta.clone(),
                downloader: Downloader::new(http.clone()),
                platform,
            };
            let resolved = installer.resolve(id, None).await.unwrap();
            listed_versions.extend(freetype_natives(&listed, platform));
            let used = freetype_natives(&resolved.json, platform);
            for version in &used {
                assert!(
                    !is_vulnerable(version),
                    "{id} on {os:?} {arch:?} would start with lwjgl-freetype {version}"
                );
            }
            assert!(used.len() <= 1, "{id} on {os:?} {arch:?}: {used:?}");
            used_versions.extend(used);
        }
        listed_versions.dedup();
        used_versions.dedup();
        swapped += usize::from(listed_versions != used_versions);
        let show = |v: &[String]| match v {
            [] => "no FreeType".to_string(),
            v => v.join(", "),
        };
        println!(
            "{id:<16} {:<24} {}",
            show(&listed_versions),
            show(&used_versions)
        );
    }
    println!("\n{swapped} of these the launcher had to patch itself.");
    println!("Patched natives on disk, checked against the pinned checksums:");
    for native in MOJANG.natives {
        let coord = format!(
            "org.lwjgl:lwjgl-freetype:{}:{}",
            MOJANG.version, native.classifier
        );
        let jar = dirs
            .libraries()
            .join(MavenCoord::parse(&coord).unwrap().path());
        let Ok(bytes) = std::fs::read(&jar) else {
            continue; // Mojang has since patched every version that needed this one.
        };
        assert_eq!(sha1_hex(&bytes), native.sha1, "{coord}");
        assert_eq!(bytes.len() as u64, native.size, "{coord}");
        println!(
            "  {} sha1 {} ok",
            jar.file_name().unwrap().to_string_lossy(),
            native.sha1
        );
    }
}

mod common;

use common::{sources, write_zip};
use launcher_core::{
    download::{hash::sha1_hex, Hashes},
    instances::SourceRef,
    meta::loaders::LoaderKind,
    sources::{
        mrpack::{self, Limits},
        FileMode, FileSource, Side,
    },
    ErrorCode,
};
use serde_json::json;
use wiremock::MockServer;

fn index(files: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "2.1.0",
        "name": "Sky Pack",
        "summary": "Islands",
        "files": files,
        "dependencies": { "minecraft": "1.20.1", "fabric-loader": "0.16.14" }
    }))
    .unwrap()
}

fn file(path: &str, env: Option<serde_json::Value>) -> serde_json::Value {
    let mut f = json!({
        "path": path,
        "hashes": { "sha1": sha1_hex(path.as_bytes()), "sha512": null },
        "downloads": [format!("https://cdn.example.com/{path}")],
        "fileSize": 10
    });
    if let Some(env) = env {
        f["env"] = env;
    }
    f
}

#[tokio::test]
async fn imports_files_env_and_overrides() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let pack = tmp.path().join("Sky Pack-2.1.0.mrpack");
    let idx = index(json!([
        file("mods/a.jar", None),
        file(
            "mods/server.jar",
            Some(json!({ "client": "unsupported", "server": "required" }))
        ),
        file(
            "mods/opt.jar",
            Some(json!({ "client": "optional", "server": "unsupported" }))
        ),
        file("config/x.toml", None),
    ]));
    write_zip(
        &pack,
        &[
            ("modrinth.index.json", &idx[..]),
            ("overrides/config/x.toml", &b"shared"[..]),
            ("overrides/mods/extra.jar", &b"jar"[..]),
            ("client-overrides/config/x.toml", &b"client"[..]),
            ("server-overrides/config/s.toml", &b"ignored"[..]),
        ],
    );
    let src = sources(&server, tmp.path());
    let (source, desired) = mrpack::import_file(&src, &pack).await.unwrap();
    let sha1 = sha1_hex(&std::fs::read(&pack).unwrap());
    assert_eq!(
        source,
        SourceRef::Mrpack {
            name: "Sky Pack-2.1.0.mrpack".into(),
            sha1: sha1.clone()
        }
    );
    assert_eq!(desired.stamp, sha1);
    assert_eq!(desired.archive, Some(Hashes::sha1(&sha1)));
    assert_eq!(
        (desired.name.as_str(), desired.pack_version.as_str()),
        ("Sky Pack", "2.1.0")
    );
    assert_eq!(desired.about.description.as_deref(), Some("Islands"));
    assert_eq!(desired.loader.clone().unwrap().kind, LoaderKind::Fabric);

    let by_path = |p: &str| desired.files.iter().find(|f| f.path == p).cloned();
    assert_eq!(by_path("mods/server.jar").unwrap().side, Side::Server);
    let opt = by_path("mods/opt.jar").unwrap();
    assert_eq!(opt.side, Side::Client);
    assert!(opt.optional.unwrap().default);
    let x = by_path("config/x.toml").unwrap();
    assert_eq!(
        x.source,
        FileSource::Archive {
            entry: "client-overrides/config/x.toml".into()
        }
    );
    assert_eq!(x.hashes, Hashes::sha1(sha1_hex(b"client")));
    assert_eq!(x.mode, FileMode::ReplaceUnlessModified);
    let extra = by_path("mods/extra.jar").unwrap();
    assert_eq!(extra.mode, FileMode::Replace);
    assert_eq!(extra.size, Some(3));
    assert!(
        by_path("config/s.toml").is_none(),
        "server overrides never reach a client"
    );

    let again = mrpack::resolve_cached(&src, &sha1).await.unwrap();
    assert_eq!(again, desired);

    let out = tmp.path().join("out/x.toml");
    let archive = src.cache.get(&Hashes::sha1(&sha1)).await.unwrap();
    mrpack::extract_entry(
        &archive,
        "client-overrides/config/x.toml",
        &out,
        &sha1_hex(b"client"),
    )
    .unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"client");
    let wrong = mrpack::extract_entry(
        &archive,
        "overrides/config/x.toml",
        &out,
        &sha1_hex(b"client"),
    );
    assert_eq!(wrong.unwrap_err().code, ErrorCode::PackInvalid);
}

#[test]
fn mrpack_rejects_unsafe_paths_and_oversized_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let read = |entries: &[(&str, &[u8])], limits: Limits| {
        let p = tmp.path().join("p.mrpack");
        write_zip(&p, entries);
        mrpack::read_archive(&p, "s".into(), Hashes::sha1(sha1_hex(b"s")), limits)
    };
    let escaping = index(json!([file("../evil.jar", None)]));
    assert_eq!(
        read(&[("modrinth.index.json", &escaping[..])], Limits::default())
            .unwrap_err()
            .code,
        ErrorCode::PackUnsafePath
    );
    let ok = index(json!([]));
    assert_eq!(
        read(
            &[
                ("modrinth.index.json", &ok[..]),
                ("overrides/CON.txt", &b"x"[..])
            ],
            Limits::default()
        )
        .unwrap_err()
        .code,
        ErrorCode::PackUnsafePath
    );
    let small = Limits {
        index: 1 << 20,
        entry: 4,
        total: 100,
    };
    assert_eq!(
        read(
            &[
                ("modrinth.index.json", &ok[..]),
                ("overrides/big.txt", &b"0123456789"[..])
            ],
            small
        )
        .unwrap_err()
        .code,
        ErrorCode::PackInvalid
    );
    let tiny_total = Limits {
        index: 1 << 20,
        entry: 8,
        total: 10,
    };
    assert_eq!(
        read(
            &[
                ("modrinth.index.json", &ok[..]),
                ("overrides/a.txt", &b"123456"[..]),
                ("overrides/b.txt", &b"123456"[..])
            ],
            tiny_total
        )
        .unwrap_err()
        .code,
        ErrorCode::PackInvalid
    );
    let tiny_index = Limits {
        index: 8,
        entry: 8,
        total: 10,
    };
    assert_eq!(
        read(&[("modrinth.index.json", &ok[..])], tiny_index)
            .unwrap_err()
            .code,
        ErrorCode::PackInvalid
    );
}

#[test]
fn mrpack_format_problems_are_named() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("p.mrpack");
    write_zip(&p, &[("readme.txt", &b"no index"[..])]);
    let read = || mrpack::read_archive(&p, "s".into(), Hashes::default(), Limits::default());
    assert_eq!(read().unwrap_err().code, ErrorCode::PackInvalid);

    let mut other_game: serde_json::Value = serde_json::from_slice(&index(json!([]))).unwrap();
    other_game["game"] = json!("terraria");
    write_zip(
        &p,
        &[(
            "modrinth.index.json",
            &serde_json::to_vec(&other_game).unwrap()[..],
        )],
    );
    assert_eq!(read().unwrap_err().code, ErrorCode::PackUnsupported);

    let mut plain_http = file("mods/a.jar", None);
    plain_http["downloads"] = json!(["http://cdn.example.com/a.jar"]);
    write_zip(
        &p,
        &[("modrinth.index.json", &index(json!([plain_http]))[..])],
    );
    assert_eq!(
        read().unwrap_err().code,
        ErrorCode::PackInvalid,
        "no usable https download"
    );

    std::fs::write(&p, b"not a zip").unwrap();
    assert_eq!(read().unwrap_err().code, ErrorCode::PackInvalid);
}

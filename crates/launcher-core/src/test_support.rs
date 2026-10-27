//! Helpers shared by unit tests.
#![allow(dead_code)]

use std::path::Path;

use crate::{
    meta::mojang::VersionJson,
    platform::{Arch, Os, Platform},
};

pub const WINDOWS_X64: Platform = Platform {
    os: Os::Windows,
    arch: Arch::X64,
};
pub const LINUX_X64: Platform = Platform {
    os: Os::Linux,
    arch: Arch::X64,
};

pub fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}

pub fn version(name: &str) -> VersionJson {
    serde_json::from_str(&fixture(name)).unwrap()
}

/// Write a zip with the given entries. Generic over the body so `b"..."` literals work as-is
/// (an unsized coercion doesn't happen inside a tuple).
pub fn write_zip<B: AsRef<[u8]>>(path: &Path, entries: &[(&str, B)]) {
    let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, body) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut w, body.as_ref()).unwrap();
    }
    w.finish().unwrap();
}

/// Vanilla 1.20.1 merged with Forge 47.4.22, as `Installer::resolve` returns it.
pub fn forge_resolved() -> crate::install::ResolvedVersion {
    crate::install::ResolvedVersion {
        vanilla_id: "1.20.1".into(),
        json: crate::meta::merge::merge(
            version("1.20.1.json"),
            version("forge-1.20.1-47.4.22-version.json"),
        ),
        forge: Some(crate::install::ForgeInstall {
            kind: crate::meta::loaders::LoaderKind::Forge,
            loader_version: "47.4.22".into(),
            installer: "/pl/installer.jar".into(),
            installer_sha1: "afef69499ecb9c712e738d38aa679bd9ed1f3468".into(),
            profile: crate::meta::forge::parse_profile(
                fixture("forge-1.20.1-47.4.22-install_profile.json").as_bytes(),
            )
            .unwrap(),
        }),
    }
}

/// A Microsoft session for Steve, as the launch check returns one, with no client ID.
pub fn session() -> crate::accounts::Session {
    crate::accounts::Session {
        username: "Steve".into(),
        uuid: "5627dd98e6be3c21b8a8e92344183641".into(),
        access_token: "mc-token".into(),
        user_type: "msa".into(),
        xuid: None,
        client_id: None,
    }
}

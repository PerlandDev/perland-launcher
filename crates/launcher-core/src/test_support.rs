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

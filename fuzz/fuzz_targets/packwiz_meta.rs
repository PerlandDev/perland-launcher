#![no_main]
use launcher_core::{
    fsutil::normalize_rel,
    sources::{packwiz, SourceConfig},
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(meta) = packwiz::parse_meta(data, "mods/x.pw.toml") {
        if let Ok(file) = packwiz::meta_file(&SourceConfig::default(), "mods/x.pw.toml", &meta, false) {
            // Whatever the metafile says, the file it installs stays a canonical relative path.
            assert_eq!(normalize_rel(&file.path).as_deref(), Some(file.path.as_str()));
        }
    }
});

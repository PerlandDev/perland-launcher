#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let about = launcher_core::sources::sidecar::parse_sidecar(text, "https://example.com/p/pack.toml");
        for url in [about.art.icon, about.art.banner].into_iter().flatten() {
            assert!(url.starts_with("https://"), "{url}");
        }
    }
});

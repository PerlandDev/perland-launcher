#![no_main]
use launcher_core::fsutil::{normalize_rel, safe_component};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Some(n) = normalize_rel(text) {
            assert_eq!(normalize_rel(&n).as_deref(), Some(n.as_str()), "canonical form is stable");
            assert!(n.split('/').all(safe_component));
            assert!(!n.contains('\\') && !n.contains(':'));
        }
    }
});

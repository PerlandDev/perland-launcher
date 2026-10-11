#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = launcher_core::sources::mrpack::parse_index(data);
});

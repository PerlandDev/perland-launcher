#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = launcher_core::sources::packwiz::parse_pack(data);
});

#![no_main]
use launcher_core::invite::InviteLink;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(link) = InviteLink::parse(text) {
            let canonical = link.canonical();
            let again = InviteLink::parse(&canonical).expect("a canonical link parses");
            assert_eq!(again, link, "the canonical form is stable");
            assert_eq!(again.canonical(), canonical);
            assert!(canonical.starts_with("https://") || canonical.starts_with("http://"));
            // The host may happen to contain the token's characters; nothing else may.
            for shown in [format!("{link:?}"), link.to_string()] {
                assert!(
                    !shown.replace(link.host(), "").contains(link.token()),
                    "the token leaked"
                );
            }
        }
    }
});

#![no_main]
use launcher_app::deep_link::{parse, DeepLink};
use launcher_core::invite::InviteLink;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        match parse(text) {
            Ok(DeepLink::Join { invite }) => {
                assert!(invite.starts_with("https://"), "only https invites come from the web");
                let link = InviteLink::parse(&invite).expect("a join link carries a valid invite");
                assert_eq!(link.canonical(), invite, "and it is canonical");
            }
            Ok(DeepLink::Pack { url }) => assert!(url.starts_with("https://")),
            Err(_) => {}
        }
    }
});

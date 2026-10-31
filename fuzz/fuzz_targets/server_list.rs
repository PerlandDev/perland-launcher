#![no_main]
use launcher_core::{instances::ServerAddress, server_list};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let address = ServerAddress::new("play.example.com", 25566).unwrap();
    let Ok((_, written)) = server_list::with_server(Some(data), "Server", &address, None, &[])
    else {
        return;
    };
    let after = written.as_deref().unwrap_or(data);
    let listed = server_list::entries(after).expect("the written list parses");
    let before = server_list::entries(data).expect("the read list parses");
    assert!(listed.len() >= before.len(), "no entry is lost");
    let (_, again) = server_list::with_server(Some(after), "Server", &address, None, &[])
        .expect("the written list is accepted");
    assert_eq!(again, None, "a second pass changes nothing");
});

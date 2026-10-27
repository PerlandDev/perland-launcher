//! The sign-in loopback listener, driven by a real HTTP client standing in for the browser.
use std::time::Duration;

use launcher_core::{
    perland::{Callback, Language, Loopback, Page},
    ErrorCode,
};
use tokio::sync::watch;

fn browser() -> reqwest::Client {
    // 127.0.0.1 must never go through the environment's proxy.
    reqwest::Client::builder().no_proxy().build().unwrap()
}

const LONG: Duration = Duration::from_secs(10);

#[tokio::test]
async fn loopback_ignores_strays_then_accepts() {
    let lb = Loopback::bind().unwrap();
    let base = lb.redirect_uri();
    assert!(base.starts_with("http://127.0.0.1:") && base.ends_with("/callback"));
    let (_tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("right", Language::En, Page::Success, LONG, rx));
    let root = base.trim_end_matches("/callback").to_string();
    let c = browser();

    let r = c.get(format!("{root}/favicon.ico")).send().await.unwrap();
    assert_eq!(r.status(), 404);

    let r = c
        .get(format!("{base}?state=wrong&code=x"))
        .send()
        .await
        .unwrap();
    let ct = r.headers()["content-type"].to_str().unwrap().to_string();
    assert!(ct.starts_with("text/html"));
    assert_eq!(r.headers()["cache-control"], "no-store");
    assert!(r.text().await.unwrap().contains("This sign-in link is old"));
    assert!(!wait.is_finished());

    let r = c
        .get(format!("{base}?state=right&code=abc"))
        .send()
        .await
        .unwrap();
    assert!(r.text().await.unwrap().contains("signed in to Perland"));
    match wait.await.unwrap().unwrap() {
        Callback::Code(code) => assert_eq!(code, "abc"),
        other => panic!("{other:?}"),
    }
}

async fn one_callback(query: &str, lang: Language) -> (Callback, String) {
    let lb = Loopback::bind().unwrap();
    let base = lb.redirect_uri();
    let (_tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", lang, Page::Success, LONG, rx));
    let page = browser()
        .get(format!("{base}?{query}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    (wait.await.unwrap().unwrap(), page)
}

#[tokio::test]
async fn loopback_denied() {
    let (cb, page) = one_callback("error=access_denied&state=s", Language::En).await;
    assert!(matches!(cb, Callback::Denied));
    assert!(page.contains("Sign-in was canceled"));
}

#[tokio::test]
async fn loopback_failed() {
    let (cb, page) = one_callback("error=server_error&state=s", Language::En).await;
    match cb {
        Callback::Failed(e) => assert_eq!(e, "server_error"),
        other => panic!("{other:?}"),
    }
    assert!(page.contains("Something went wrong"));
}

#[tokio::test]
async fn loopback_right_state_without_code_is_failed() {
    let (cb, _) = one_callback("state=s", Language::En).await;
    match cb {
        Callback::Failed(e) => assert_eq!(e, "missing_code"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn success_page_is_spanish() {
    let (cb, page) = one_callback("code=zz&state=s", Language::Es).await;
    assert!(matches!(cb, Callback::Code(_)));
    assert!(page.contains("Ya iniciaste sesión"));
    assert!(page.contains("lang=\"es\""));
}

#[tokio::test]
async fn loopback_deadline() {
    let lb = Loopback::bind().unwrap();
    let (_tx, rx) = watch::channel(false);
    let err = lb
        .wait(
            "s",
            Language::En,
            Page::Success,
            Duration::from_millis(50),
            rx,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::SignInTimedOut);
}

#[tokio::test]
async fn loopback_cancel() {
    let lb = Loopback::bind().unwrap();
    let (tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", Language::En, Page::Success, LONG, rx));
    tokio::time::sleep(Duration::from_millis(20)).await;
    tx.send(true).unwrap();
    assert!(matches!(wait.await.unwrap().unwrap(), Callback::Canceled));
}

#[tokio::test]
async fn loopback_cancel_when_sender_dropped() {
    let lb = Loopback::bind().unwrap();
    let (tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", Language::En, Page::Success, LONG, rx));
    drop(tx);
    assert!(matches!(wait.await.unwrap().unwrap(), Callback::Canceled));
}

#[tokio::test]
async fn a_silent_connection_does_not_block_the_real_callback() {
    let lb = Loopback::bind().unwrap();
    let base = lb.redirect_uri();
    let addr = base
        .trim_start_matches("http://")
        .trim_end_matches("/callback")
        .to_string();
    let (_tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", Language::En, Page::Success, LONG, rx));
    // A browser's speculative connection: opens and says nothing.
    let _idle = tokio::net::TcpStream::connect(&addr).await.unwrap();
    browser()
        .get(format!("{base}?state=s&code=ok"))
        .send()
        .await
        .unwrap();
    assert!(matches!(wait.await.unwrap().unwrap(), Callback::Code(_)));
}

/// The `host:port` the listener holds.
fn addr_of(lb: &Loopback) -> String {
    lb.redirect_uri()
        .trim_start_matches("http://")
        .trim_end_matches("/callback")
        .to_string()
}

/// Sends `head` on a raw connection, waits a moment like a slow browser, then reads the whole
/// answer. A reset (the listener closing with request bytes unread) is an error here.
async fn raw_request(addr: &str, head: &[u8]) -> std::io::Result<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await?;
    stream.write_all(head).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await?;
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

/// A request head with `size` bytes of headers after the request line.
fn long_head(state: &str, size: usize) -> Vec<u8> {
    let mut head = format!("GET /callback?state={state}&code=ok HTTP/1.1\r\nHost: 127.0.0.1\r\n");
    let mut sent = 0;
    let mut n = 0;
    while sent < size {
        let line = format!("X-Filler-{n}: {}\r\n", "a".repeat(200));
        sent += line.len();
        head.push_str(&line);
        n += 1;
    }
    head.push_str("\r\n");
    head.into_bytes()
}

#[tokio::test]
async fn a_head_over_1_kib_still_gets_the_whole_page() {
    let lb = Loopback::bind().unwrap();
    let addr = addr_of(&lb);
    let (_tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", Language::En, Page::Success, LONG, rx));
    let answer = raw_request(&addr, &long_head("s", 4 << 10))
        .await
        .expect("no reset");
    assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
    assert!(answer.contains("signed in to Perland"));
    assert!(answer.ends_with("</html>"));
    assert!(matches!(wait.await.unwrap().unwrap(), Callback::Code(_)));
}

#[tokio::test]
async fn a_head_over_the_cap_is_refused() {
    let lb = Loopback::bind().unwrap();
    let addr = addr_of(&lb);
    let (_tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", Language::En, Page::Success, LONG, rx));
    let answer = raw_request(&addr, &long_head("s", 12 << 10))
        .await
        .expect("no reset");
    assert!(
        answer.starts_with("HTTP/1.1 431"),
        "{}",
        &answer[..answer.len().min(80)]
    );
    assert!(
        !wait.is_finished(),
        "a refused request doesn't end the wait"
    );
    wait.abort();
}

#[tokio::test]
async fn the_answer_waits_for_the_end_of_the_head() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let lb = Loopback::bind().unwrap();
    let addr = addr_of(&lb);
    let (_tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", Language::En, Page::Success, LONG, rx));
    let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    stream
        .write_all(b"GET /callback?state=s&code=ok HTTP/1.1\r\nHost: 127.0.0.1\r\n")
        .await
        .unwrap();
    let mut byte = [0u8; 1];
    let early = tokio::time::timeout(Duration::from_millis(200), stream.read(&mut byte)).await;
    assert!(early.is_err(), "nothing is answered before the head ends");
    stream
        .write_all(b"Accept: text/html\r\n\r\n")
        .await
        .unwrap();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await.expect("no reset");
    assert!(String::from_utf8_lossy(&answer).contains("signed in to Perland"));
    assert!(matches!(wait.await.unwrap().unwrap(), Callback::Code(_)));
}

#[tokio::test]
async fn loopback_shows_the_success_page_it_was_given() {
    let lb = Loopback::bind().unwrap();
    let base = lb.redirect_uri();
    let (_tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", Language::En, Page::MicrosoftContinue, LONG, rx));
    let page = browser()
        .get(format!("{base}?state=s&code=abc"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("Go back to Perland Launcher to finish signing in"));
    assert!(!page.contains("signed in to Perland"));
    assert!(matches!(wait.await.unwrap().unwrap(), Callback::Code(_)));
}

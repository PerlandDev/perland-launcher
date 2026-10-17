//! The sign-in loopback listener, driven by a real HTTP client standing in for the browser.
use std::time::Duration;

use launcher_core::{
    perland::{Callback, Language, Loopback},
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
    let wait = tokio::spawn(lb.wait("right", Language::En, LONG, rx));
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
    let wait = tokio::spawn(lb.wait("s", lang, LONG, rx));
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
        .wait("s", Language::En, Duration::from_millis(50), rx)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::SignInTimedOut);
}

#[tokio::test]
async fn loopback_cancel() {
    let lb = Loopback::bind().unwrap();
    let (tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", Language::En, LONG, rx));
    tokio::time::sleep(Duration::from_millis(20)).await;
    tx.send(true).unwrap();
    assert!(matches!(wait.await.unwrap().unwrap(), Callback::Canceled));
}

#[tokio::test]
async fn loopback_cancel_when_sender_dropped() {
    let lb = Loopback::bind().unwrap();
    let (tx, rx) = watch::channel(false);
    let wait = tokio::spawn(lb.wait("s", Language::En, LONG, rx));
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
    let wait = tokio::spawn(lb.wait("s", Language::En, LONG, rx));
    // A browser's speculative connection: opens and says nothing.
    let _idle = tokio::net::TcpStream::connect(&addr).await.unwrap();
    browser()
        .get(format!("{base}?state=s&code=ok"))
        .send()
        .await
        .unwrap();
    assert!(matches!(wait.await.unwrap().unwrap(), Callback::Code(_)));
}

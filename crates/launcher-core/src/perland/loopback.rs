//! The sign-in listener: the browser returns to `http://127.0.0.1:{port}/callback` after the
//! Engine has signed the player in. A tiny hand-written HTTP/1.1 reader is enough: one GET, one
//! small page back, `Connection: close`. Nothing here prints the authorization code.

use std::{fmt, net::TcpListener as StdListener, time::Duration};

use percent_encoding::percent_decode_str;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
    time::{timeout, Instant},
};

use crate::error::{ErrorCode, LauncherError, Result};

/// The longest request head read, in bytes.
const HEAD_CAP: usize = 8 << 10;
/// How long one connection may take to send its request head.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// How long an answered connection is read for what the browser still sends.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
/// The most an answered connection is read for, in bytes.
const DRAIN_CAP: usize = 64 << 10;
/// How long to pause after the listener fails to accept, so a lasting failure doesn't spin.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(75);
/// The longest `error` value kept from the callback, in characters.
const ERROR_CAP: usize = 64;

/// The language of the page the browser is left on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    En,
    Es,
}

impl Language {
    /// `es`, `es-MX`, `ES_419` give Spanish; anything else English.
    pub fn parse(tag: &str) -> Self {
        let tag = tag.trim().to_ascii_lowercase();
        match tag.split(['-', '_']).next() {
            Some("es") => Self::Es,
            _ => Self::En,
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
        }
    }
}

/// How the browser's return ended the wait.
pub enum Callback {
    /// The authorization code, to exchange at `POST /token`.
    Code(String),
    /// The player canceled on Discord (`error=access_denied`).
    Denied,
    /// The Engine's `error` value (`server_error`, `discord_unavailable`, ...).
    Failed(String),
    /// The caller gave up.
    Canceled,
}

impl fmt::Debug for Callback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(_) => f.write_str("Code(<hidden>)"),
            Self::Denied => f.write_str("Denied"),
            Self::Failed(e) => write!(f, "Failed({e:?})"),
            Self::Canceled => f.write_str("Canceled"),
        }
    }
}

/// A bound, not yet accepting, listener. Bind first so the redirect URI is known before the
/// browser opens; the port is held from `bind` on.
pub struct Loopback {
    listener: StdListener,
    port: u16,
}

impl Loopback {
    /// Binds `127.0.0.1` on a port the OS picks, at 1024 or above (the Engine refuses lower).
    pub fn bind() -> std::io::Result<Self> {
        let mut last = None;
        for _ in 0..16 {
            let listener = StdListener::bind(("127.0.0.1", 0))?;
            let port = listener.local_addr()?.port();
            if port >= 1024 {
                listener.set_nonblocking(true)?;
                return Ok(Self { listener, port });
            }
            last = Some(port);
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            format!("no loopback port of 1024 or above (last tried {last:?})"),
        ))
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }

    /// Accepts connections until one carries `state` back with a code or an error, the
    /// `deadline` passes (`sign_in_timed_out`), or `cancel` turns true or its sender is dropped.
    /// Other paths get a 404 and a callback with another `state` gets the "old link" page; the
    /// wait goes on after both. Must run inside a tokio runtime.
    pub async fn wait(
        self,
        state: &str,
        language: Language,
        deadline: Duration,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<Callback> {
        let listener = TcpListener::from_std(self.listener).map_err(|e| {
            LauncherError::auth(ErrorCode::SignInFailed, "Couldn't listen for the sign-in.")
                .with_detail(e.to_string())
        })?;
        let expires = Instant::now() + deadline;
        let mut connections: JoinSet<Option<Callback>> = JoinSet::new();
        loop {
            if *cancel.borrow() {
                return Ok(Callback::Canceled);
            }
            tokio::select! {
                biased;
                changed = cancel.changed() => {
                    // A true value, or a dropped sender, both end the wait.
                    if changed.is_err() || *cancel.borrow() {
                        return Ok(Callback::Canceled);
                    }
                }
                () = tokio::time::sleep_until(expires) => {
                    return Err(LauncherError::auth(
                        ErrorCode::SignInTimedOut,
                        "Sign-in took too long. Try again.",
                    ));
                }
                Some(done) = connections.join_next() => {
                    if let Ok(Some(callback)) = done {
                        return Ok(callback);
                    }
                }
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        let state = state.to_string();
                        connections.spawn(serve(stream, state, language));
                    }
                    // Out of file descriptors, say: wait a little rather than spin.
                    Err(_) => tokio::time::sleep(ACCEPT_BACKOFF).await,
                },
            }
        }
    }
}

/// What one request amounts to.
#[derive(Debug, PartialEq, Eq)]
enum Request {
    Callback {
        code: Option<String>,
        error: Option<String>,
        state: Option<String>,
    },
    NotFound,
    MethodNotAllowed,
    Malformed,
    TooLarge,
}

/// Where the request head ends (just past its blank line), if it does.
fn head_end(buf: &[u8]) -> Option<usize> {
    let crlf = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4);
    let lf = buf.windows(2).position(|w| w == b"\n\n").map(|i| i + 2);
    match (crlf, lf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Reads the whole request head, up to its blank line (or `HEAD_CAP` bytes, or the end of the
/// stream), so the answer never leaves request bytes unread: closing with some would reset the
/// connection, and the browser could show an error instead of the page.
async fn read_head<R: AsyncRead + Unpin>(stream: &mut R) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while buf.len() <= HEAD_CAP && head_end(&buf).is_none() {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}

fn parse_request(head: &[u8]) -> Request {
    match head_end(head) {
        None if head.len() > HEAD_CAP => return Request::TooLarge,
        Some(end) if end > HEAD_CAP => return Request::TooLarge,
        _ => {}
    }
    let end = head.iter().position(|b| *b == b'\n').unwrap_or(head.len());
    let Ok(line) = std::str::from_utf8(&head[..end]) else {
        return Request::Malformed;
    };
    parse_request_line(line.trim_end_matches('\r'))
}

fn parse_request_line(line: &str) -> Request {
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Request::Malformed;
    };
    if !version.starts_with("HTTP/1.") {
        return Request::Malformed;
    }
    if method != "GET" {
        return Request::MethodNotAllowed;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/callback" {
        return Request::NotFound;
    }
    let (mut code, mut error, mut state) = (None, None, None);
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode_str(&value.replace('+', " "))
            .decode_utf8_lossy()
            .into_owned();
        match key {
            "code" => code = Some(value),
            "error" => error = Some(value),
            "state" => state = Some(value),
            _ => {}
        }
    }
    Request::Callback { code, error, state }
}

#[derive(Clone, Copy)]
enum Page {
    Success,
    Denied,
    Failed,
    OldLink,
}

fn page_html(page: Page, language: Language) -> String {
    let text = match (page, language) {
        (Page::Success, Language::En) => {
            "You're signed in to Perland. You can close this tab and go back to the launcher."
        }
        (Page::Success, Language::Es) => {
            "Ya iniciaste sesión en Perland. Puedes cerrar esta pestaña y volver al launcher."
        }
        (Page::Denied, Language::En) => "Sign-in was canceled. You can close this tab.",
        (Page::Denied, Language::Es) => {
            "Cancelaste el inicio de sesión. Puedes cerrar esta pestaña."
        }
        (Page::Failed, Language::En) => {
            "Something went wrong. Go back to the launcher and try again."
        }
        (Page::Failed, Language::Es) => "Algo salió mal. Vuelve al launcher e inténtalo de nuevo.",
        (Page::OldLink, Language::En) => {
            "This sign-in link is old. Go back to the launcher and try again."
        }
        (Page::OldLink, Language::Es) => {
            "Este enlace para iniciar sesión ya no sirve. Vuelve al launcher e inténtalo de nuevo."
        }
    };
    // The text holds an apostrophe and no markup, so it needs no escaping.
    format!(
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Perland</title>\
<style>body{{margin:0;min-height:100vh;display:grid;place-items:center;\
font:16px/1.5 system-ui,sans-serif;background:#101418;color:#e8edf2}}\
p{{max-width:28em;margin:1rem;text-align:center}}</style></head>\
<body><p>{text}</p></body></html>",
        lang = language.tag()
    )
}

fn response(status: &str, content_type: &str, body: &str, extra: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
Cache-Control: no-store\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn html_response(status: &str, page: Page, language: Language) -> Vec<u8> {
    response(
        status,
        "text/html; charset=utf-8",
        &page_html(page, language),
        "",
    )
}

fn plain(status: &str, extra: &str) -> Vec<u8> {
    response(status, "text/plain; charset=utf-8", status, extra)
}

/// Keeps what the Engine called the failure: short, and nothing but a plain identifier.
fn clean_error(raw: &str) -> String {
    let kept: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .take(ERROR_CAP)
        .collect();
    if kept.is_empty() {
        "unknown_error".into()
    } else {
        kept
    }
}

/// Answers one connection. Returns the outcome only when the request ended the wait.
async fn serve(mut stream: TcpStream, state: String, language: Language) -> Option<Callback> {
    let head = timeout(READ_TIMEOUT, read_head(&mut stream))
        .await
        .ok()?
        .ok()?;
    let (bytes, outcome) = match parse_request(&head) {
        Request::NotFound => (plain("404 Not Found", ""), None),
        Request::MethodNotAllowed => (plain("405 Method Not Allowed", "Allow: GET\r\n"), None),
        Request::Malformed => (plain("400 Bad Request", ""), None),
        Request::TooLarge => (plain("431 Request Header Fields Too Large", ""), None),
        Request::Callback {
            code,
            error,
            state: got,
        } => {
            if got.as_deref() != Some(state.as_str()) {
                (
                    html_response("400 Bad Request", Page::OldLink, language),
                    None,
                )
            } else if let Some(error) = error {
                if error == "access_denied" {
                    (
                        html_response("200 OK", Page::Denied, language),
                        Some(Callback::Denied),
                    )
                } else {
                    (
                        html_response("200 OK", Page::Failed, language),
                        Some(Callback::Failed(clean_error(&error))),
                    )
                }
            } else if let Some(code) = code.filter(|c| !c.is_empty()) {
                (
                    html_response("200 OK", Page::Success, language),
                    Some(Callback::Code(code)),
                )
            } else {
                (
                    html_response("200 OK", Page::Failed, language),
                    Some(Callback::Failed("missing_code".into())),
                )
            }
        }
    };
    let _ = stream.write_all(&bytes).await;
    let _ = stream.shutdown().await;
    // Whatever the browser still sends (a head over the cap) is read and dropped until it
    // closes, briefly, apart from the wait, so the answer is not lost to a reset.
    tokio::spawn(drain(stream));
    outcome
}

/// Reads and drops what is left on a connection already answered, until the peer closes it,
/// `DRAIN_CAP` bytes or `DRAIN_TIMEOUT`.
async fn drain(mut stream: TcpStream) {
    let mut chunk = [0u8; 1024];
    let mut left = DRAIN_CAP;
    let _ = timeout(DRAIN_TIMEOUT, async {
        while left > 0 {
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => left = left.saturating_sub(n),
            }
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_request_line_reads_code_and_state() {
        assert_eq!(
            parse_request(b"GET /callback?code=a&state=b HTTP/1.1\r\nHost: x\r\n\r\n"),
            Request::Callback {
                code: Some("a".into()),
                error: None,
                state: Some("b".into())
            }
        );
    }

    #[test]
    fn parse_request_line_decodes_percent_escapes() {
        assert_eq!(
            parse_request(b"GET /callback?error=a%20b&state=x%2By HTTP/1.1\r\n\r\n"),
            Request::Callback {
                code: None,
                error: Some("a b".into()),
                state: Some("x+y".into())
            }
        );
    }

    #[test]
    fn parse_request_line_rejects_other_methods_paths_and_junk() {
        assert_eq!(
            parse_request(b"POST /callback?code=a HTTP/1.1\r\n\r\n"),
            Request::MethodNotAllowed
        );
        assert_eq!(
            parse_request(b"GET /favicon.ico HTTP/1.1\r\n\r\n"),
            Request::NotFound
        );
        assert_eq!(
            parse_request(b"GET /callback/extra?code=a HTTP/1.1\r\n\r\n"),
            Request::NotFound
        );
        assert_eq!(parse_request(b"hello\r\n"), Request::Malformed);
        assert_eq!(parse_request(b"GET /callback\r\n"), Request::Malformed);
        assert_eq!(parse_request(&[0xff, 0xfe, b'\n']), Request::Malformed);
    }

    #[test]
    fn parse_request_line_rejects_a_head_over_8_kib() {
        let mut long = b"GET /callback?code=".to_vec();
        long.extend(std::iter::repeat_n(b'a', 9000));
        assert_eq!(parse_request(&long), Request::TooLarge);
        long.extend_from_slice(b" HTTP/1.1\r\n");
        assert_eq!(parse_request(&long), Request::TooLarge);
    }

    #[tokio::test]
    async fn read_head_stops_at_the_cap() {
        let (mut client, mut server) = tokio::io::duplex(64 << 10);
        client.write_all(&vec![b'a'; 20_000]).await.unwrap();
        let head = read_head(&mut server).await.unwrap();
        assert!(head.len() > HEAD_CAP && head.len() < 20_000);
        assert_eq!(parse_request(&head), Request::TooLarge);
    }

    #[tokio::test]
    async fn read_head_reads_the_whole_head() {
        let mut sent = b"GET /callback?code=a&state=b HTTP/1.1\r\nHost: x\r\n".to_vec();
        for n in 0..20 {
            sent.extend_from_slice(format!("X-{n}: {}\r\n", "a".repeat(200)).as_bytes());
        }
        sent.extend_from_slice(b"\r\n");
        assert!(sent.len() > 4 << 10);
        let (mut client, mut server) = tokio::io::duplex(64 << 10);
        client.write_all(&sent).await.unwrap();
        let head = read_head(&mut server).await.unwrap();
        assert_eq!(
            head, sent,
            "every byte of the head is read before answering"
        );
        assert!(matches!(parse_request(&head), Request::Callback { .. }));
    }

    #[test]
    fn head_end_finds_the_blank_line() {
        assert_eq!(head_end(b"GET / HTTP/1.1\r\nA: b\r\n\r\nrest"), Some(24));
        assert_eq!(head_end(b"GET / HTTP/1.1\n\n"), Some(16));
        assert_eq!(head_end(b"GET / HTTP/1.1\r\nA: b\r\n"), None);
    }

    #[test]
    fn a_complete_head_over_the_cap_is_too_large() {
        let mut head = b"GET /callback?code=a&state=b HTTP/1.1\r\n".to_vec();
        head.extend(std::iter::repeat_n(b'a', HEAD_CAP));
        head.extend_from_slice(b"\r\n\r\n");
        assert_eq!(parse_request(&head), Request::TooLarge);
    }

    #[test]
    fn language_parse() {
        assert_eq!(Language::parse("es"), Language::Es);
        assert_eq!(Language::parse("es-MX"), Language::Es);
        assert_eq!(Language::parse("ES_419"), Language::Es);
        assert_eq!(Language::parse("en"), Language::En);
        assert_eq!(Language::parse("fr"), Language::En);
        assert_eq!(Language::parse(""), Language::En);
    }

    #[test]
    fn pages_are_self_contained() {
        for page in [Page::Success, Page::Denied, Page::Failed, Page::OldLink] {
            for lang in [Language::En, Language::Es] {
                let html = page_html(page, lang);
                assert!(html.contains(&format!("<html lang=\"{}\">", lang.tag())));
                assert!(html.contains("charset=\"utf-8\""));
                assert!(
                    !html.contains("<script") && !html.contains("src=") && !html.contains("href=")
                );
            }
        }
    }

    #[test]
    fn callback_debug_hides_the_code() {
        let shown = format!("{:?}", Callback::Code("topsecret".into()));
        assert!(!shown.contains("topsecret"));
    }

    #[test]
    fn clean_error_keeps_identifiers_only() {
        assert_eq!(clean_error("server_error"), "server_error");
        assert_eq!(clean_error("<b>x</b>"), "bxb");
        assert_eq!(clean_error("!!!"), "unknown_error");
        assert_eq!(clean_error(&"a".repeat(500)).len(), ERROR_CAP);
    }

    #[test]
    fn bind_gives_a_loopback_port_of_1024_or_above() {
        let lb = Loopback::bind().unwrap();
        let uri = lb.redirect_uri();
        let port: u16 = uri
            .trim_start_matches("http://127.0.0.1:")
            .trim_end_matches("/callback")
            .parse()
            .unwrap();
        assert!(port >= 1024);
    }
}

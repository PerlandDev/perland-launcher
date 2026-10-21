//! Following a server's log: the Engine's server-sent events stream, cleaned for display.
//! The JWT travels in a header only and nothing here prints one.

use std::time::Duration;

use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::watch;

use super::{
    auth::SessionJwt,
    stacks::{StackClient, StreamRefusal},
};
use crate::{error::Result, sync::BoxFuture};

/// How long to wait before opening the stream again after it ended or dropped.
pub const RECONNECT_AFTER: Duration = Duration::from_secs(5);
/// The longest event line kept, in bytes; a longer one is dropped.
const LINE_CAP: usize = 1 << 20;

/// One line of the server's log, with the control codes taken out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub text: String,
    /// RFC3339Nano, as the Engine sent it.
    pub timestamp: Option<String>,
}

/// What the stream says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseEvent {
    Log(LogLine),
    /// The Engine closed the stream: `eof`, `max_duration` or `error`.
    End(String),
}

/// Removes what a console would show as color or style: `§` and the character after it,
/// ANSI CSI sequences (`ESC [ ... final byte`) and other control characters except tab.
pub fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '§' => {
                chars.next();
            }
            '\u{1b}' => {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for n in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&n) {
                            break;
                        }
                    }
                }
            }
            '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[derive(Deserialize)]
struct LogData {
    #[serde(default)]
    text: String,
    timestamp: Option<String>,
}

#[derive(Deserialize)]
struct EndData {
    #[serde(default)]
    reason: String,
}

/// Reads server-sent events from bytes as they arrive, in any chunking.
#[derive(Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    /// An over-long line is being dropped up to its next newline.
    skipping: bool,
    event: String,
    data: Vec<String>,
}

impl SseParser {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            let raw: Vec<u8> = self.buffer.drain(..=end).collect();
            let mut line = &raw[..raw.len() - 1];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            if std::mem::take(&mut self.skipping) || line.len() > LINE_CAP {
                continue;
            }
            if let Some(event) = self.line(&String::from_utf8_lossy(line)) {
                events.push(event);
            }
        }
        if self.buffer.len() > LINE_CAP {
            self.buffer.clear();
            self.skipping = true;
        }
        events
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => self.event = value.to_string(),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        let event = std::mem::take(&mut self.event);
        let data = std::mem::take(&mut self.data).join("\n");
        match event.as_str() {
            "log" => serde_json::from_str::<LogData>(&data).ok().map(|d| {
                SseEvent::Log(LogLine {
                    text: d.text,
                    timestamp: d.timestamp,
                })
            }),
            "end" => serde_json::from_str::<EndData>(&data)
                .ok()
                .map(|d| SseEvent::End(d.reason)),
            _ => None,
        }
    }
}

/// A timestamp as a string that sorts by time: `YYYY-MM-DDTHH:MM:SS`, then the fraction padded
/// to nine digits, then `Z`. Only the UTC (`Z`) form the daemon writes is understood; anything
/// else is `None`.
fn time_key(timestamp: &str) -> Option<String> {
    let rest = timestamp.strip_suffix('Z')?;
    let (whole, fraction) = rest.split_once('.').unwrap_or((rest, ""));
    let b = whole.as_bytes();
    let shape = b.len() == 19
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            _ => c.is_ascii_digit(),
        });
    let fraction_ok = fraction.len() <= 9 && fraction.bytes().all(|c| c.is_ascii_digit());
    (shape && fraction_ok).then(|| format!("{whole}.{fraction:0<9}"))
}

/// Resolves when `stop` is true, or when its sender is gone (nobody can ask to keep going).
async fn stopped(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|stop| *stop).await;
}

/// Follows the log of `stack` until `stop` turns true or the Engine refuses. When the stream
/// ends or the connection drops it waits `RECONNECT_AFTER` and opens it again from the last
/// line seen, never repeating one. A 401 asks `jwt` for a new token once per attempt; any
/// other refusal (a full set of streams, a missing server) is returned.
pub async fn follow_logs(
    client: &StackClient,
    jwt: impl Fn() -> BoxFuture<'static, Result<SessionJwt>>,
    stack: &str,
    on_lines: impl FnMut(Vec<LogLine>) + Send,
    stop: watch::Receiver<bool>,
) -> Result<()> {
    follow_logs_after(client, jwt, stack, on_lines, stop, RECONNECT_AFTER).await
}

/// `follow_logs` with its own wait before reconnecting. Tests use a short one: tokio's paused
/// clock would also fire the HTTP client's connect timeout while a socket is pending.
pub async fn follow_logs_after(
    client: &StackClient,
    jwt: impl Fn() -> BoxFuture<'static, Result<SessionJwt>>,
    stack: &str,
    mut on_lines: impl FnMut(Vec<LogLine>) + Send,
    mut stop: watch::Receiver<bool>,
    reconnect_after: Duration,
) -> Result<()> {
    // Docker's RFC3339Nano trims trailing zeros from the fraction (even all of it), so
    // timestamps are compared as `time_key`s, padded to one width.
    // `last` is the raw timestamp sent back as `since`; `last_key` is its normalized form.
    let mut last: Option<String> = None;
    let mut last_key: Option<String> = None;
    let mut connected = false;
    loop {
        if *stop.borrow() {
            return Ok(());
        }
        let opened = tokio::select! {
            _ = stopped(&mut stop) => return Ok(()),
            opened = open(client, &jwt, stack, last.as_deref()) => opened,
        };
        match opened {
            Ok(response) => {
                connected = true;
                let mut body = response.bytes_stream();
                let mut parser = SseParser::default();
                'read: loop {
                    let chunk = tokio::select! {
                        _ = stopped(&mut stop) => return Ok(()),
                        chunk = body.next() => chunk,
                    };
                    let Some(Ok(chunk)) = chunk else { break };
                    let mut lines = Vec::new();
                    let mut ended = false;
                    for event in parser.push(&chunk) {
                        match event {
                            SseEvent::Log(mut line) => {
                                // A line with no timestamp (or one in a shape we don't
                                // know) can't be compared, so it is never dropped as a
                                // repeat and never moves the resume point.
                                let key = line.timestamp.as_deref().and_then(time_key);
                                if let (Some(key), Some(seen)) = (&key, &last_key) {
                                    if key <= seen {
                                        continue;
                                    }
                                }
                                if let Some(key) = key {
                                    last = line.timestamp.clone();
                                    last_key = Some(key);
                                }
                                line.text = strip_formatting(&line.text);
                                lines.push(line);
                            }
                            SseEvent::End(_) => ended = true,
                        }
                    }
                    if !lines.is_empty() {
                        on_lines(lines);
                        if *stop.borrow() {
                            return Ok(());
                        }
                    }
                    if ended {
                        break 'read;
                    }
                }
            }
            Err(StreamRefusal::Unreachable(_)) if connected => {}
            Err(StreamRefusal::Unreachable(e) | StreamRefusal::Refused(e)) => return Err(e),
        }
        tokio::select! {
            _ = stopped(&mut stop) => return Ok(()),
            _ = tokio::time::sleep(reconnect_after) => {}
        }
    }
}

/// Opens the stream; a 401 gets one new token.
async fn open(
    client: &StackClient,
    jwt: &impl Fn() -> BoxFuture<'static, Result<SessionJwt>>,
    stack: &str,
    since: Option<&str>,
) -> std::result::Result<reqwest::Response, StreamRefusal> {
    let token = jwt().await.map_err(StreamRefusal::Refused)?;
    match client.stream_logs_request(&token, stack, since).await {
        Err(StreamRefusal::Refused(e)) if e.code == crate::ErrorCode::PerlandSignedOut => {
            let token = jwt().await.map_err(StreamRefusal::Refused)?;
            client.stream_logs_request(&token, stack, since).await
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(ts: &str, text: &str) -> String {
        format!(
            "event: log\ndata: {{\"stream\":\"stdout\",\"timestamp\":\"{ts}\",\"text\":\"{text}\"}}\n\n"
        )
    }

    fn line(ts: &str, text: &str) -> SseEvent {
        SseEvent::Log(LogLine {
            text: text.into(),
            timestamp: Some(ts.into()),
        })
    }

    #[test]
    fn strip_removes_section_codes_and_ansi() {
        assert_eq!(
            strip_formatting("§aHi \u{1b}[31mthere\u{1b}[0m"),
            "Hi there"
        );
        assert_eq!(strip_formatting("a\tb\u{7}c\r"), "a\tbc");
    }

    #[test]
    fn parser_reads_log_events() {
        let mut p = SseParser::default();
        let got = p.push((log("t1", "one") + &log("t2", "two")).as_bytes());
        assert_eq!(got, [line("t1", "one"), line("t2", "two")]);
    }

    #[test]
    fn parser_joins_chunks() {
        let mut p = SseParser::default();
        let whole = log("t1", "héllo");
        let bytes = whole.as_bytes();
        // Split inside the multi-byte character as well.
        let cut = whole.find('é').unwrap() + 1;
        assert!(p.push(&bytes[..10]).is_empty());
        assert!(p.push(&bytes[10..cut]).is_empty());
        assert_eq!(p.push(&bytes[cut..]), [line("t1", "héllo")]);
    }

    #[test]
    fn parser_ignores_comments_and_unknown_events() {
        let mut p = SseParser::default();
        let body = format!(
            ": ping\n\nevent: other\ndata: {{}}\n\nevent: log\ndata: not json\n\n{}",
            log("t1", "ok")
        );
        assert_eq!(p.push(body.as_bytes()), [line("t1", "ok")]);
    }

    #[test]
    fn time_keys_order_mixed_fraction_widths() {
        let a = time_key("2026-10-21T10:00:01Z").unwrap();
        let b = time_key("2026-10-21T10:00:01.5Z").unwrap();
        let c = time_key("2026-10-21T10:00:01.123456789Z").unwrap();
        assert!(a < c && c < b);
        assert_eq!(a, "2026-10-21T10:00:01.000000000");
        assert_eq!(time_key("2026-10-21T10:00:01+02:00"), None);
        assert_eq!(time_key("soon"), None);
    }

    #[test]
    fn parser_drops_an_over_long_line_to_its_newline() {
        let mut p = SseParser::default();
        // A long junk line whose tail looks like a field, then a real event.
        let mut long = vec![b'x'; LINE_CAP + 1];
        assert!(p.push(&long).is_empty());
        long = b"tail\nevent: end\ndata: {\"reason\":\"eof\"}\n\n".to_vec();
        // The rest of the long line ("tail") is ignored; the next event still parses.
        assert_eq!(p.push(&long), [SseEvent::End("eof".into())]);
    }

    #[test]
    fn parser_reads_end() {
        let mut p = SseParser::default();
        assert_eq!(
            p.push(b"event: end\ndata: {\"reason\":\"max_duration\"}\n\n"),
            [SseEvent::End("max_duration".into())]
        );
    }

    #[test]
    fn parser_handles_crlf_and_multiline_data() {
        let mut p = SseParser::default();
        let got =
            p.push(b"event: log\r\ndata: {\"timestamp\":\"t1\",\r\ndata: \"text\":\"a\"}\r\n\r\n");
        assert_eq!(got, [line("t1", "a")]);
    }
}

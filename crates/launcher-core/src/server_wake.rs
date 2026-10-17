//! Pinging a Minecraft server and waking a sleeping Perland one.
//!
//! A sleeping Perland server has a "waker" holding its port. It answers the server list with
//! the version name `Sleeping` and kicks the first allowed join, which starts the server. The
//! packets here are the few the Engine's waker (and any server) speaks: the handshake, status
//! request and login start, VarInt-framed and uncompressed.

use std::time::Duration;

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{timeout, timeout_at, Instant},
};

use crate::{
    error::{ErrorCode, LauncherError, Result},
    instances::ServerAddress,
    sync::Cancel,
};

/// How long a connection may take to open.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the packets of one ping or knock may take once connected.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(5);
/// The protocol number the handshake claims (Minecraft 1.21.1). The waker echoes it back.
const PROTOCOL: i32 = 767;
/// The longest response string read.
const MAX_STRING: usize = 32 * 1024;

/// The version name the waker's status answer carries.
const SLEEPING: &str = "Sleeping";
/// The waker's kick for a name that is not on the server's whitelist.
const NOT_WHITELISTED: &str = "not whitelisted on this server";
/// A phrase of the waker's kick when the running quota has no room to start the server.
const NO_ROOM: &str = "cannot start right now";

/// Who answered a status request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ping {
    /// A sleeping Perland server's waker.
    Waker,
    /// A running server.
    Server,
    /// Nothing usable came back.
    NoAnswer,
}

/// What the waker said to a join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Knock {
    /// The server is starting.
    Woken,
    /// The name isn't on the server's whitelist, so it won't start for this player.
    NotWhitelisted,
    /// The team has no room to run the server right now.
    NoRoom,
}

/// Why [`wait_until_up`] stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitEnd {
    Up,
    TimedOut,
    Cancelled,
}

fn push_varint(buf: &mut Vec<u8>, v: i32) {
    let mut u = v as u32;
    while u & !0x7F != 0 {
        buf.push((u & 0x7F) as u8 | 0x80);
        u >>= 7;
    }
    buf.push(u as u8);
}

fn push_string(buf: &mut Vec<u8>, s: &str) {
    push_varint(buf, s.len() as i32);
    buf.extend_from_slice(s.as_bytes());
}

/// A packet: its length, then the id and payload.
fn frame(id: i32, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(payload.len() + 5);
    push_varint(&mut body, id);
    body.extend_from_slice(payload);
    let mut out = Vec::with_capacity(body.len() + 5);
    push_varint(&mut out, body.len() as i32);
    out.extend_from_slice(&body);
    out
}

/// The handshake; `next` is the state it asks for (1 status, 2 login).
fn handshake(host: &str, port: u16, next: i32) -> Vec<u8> {
    let mut p = Vec::new();
    push_varint(&mut p, PROTOCOL);
    push_string(&mut p, host);
    p.extend_from_slice(&port.to_be_bytes());
    push_varint(&mut p, next);
    frame(0x00, &p)
}

/// Login start: the name, then a zero UUID (offline-style; the waker reads only the name).
fn login_start(name: &str) -> Vec<u8> {
    let mut p = Vec::new();
    push_string(&mut p, name);
    p.extend_from_slice(&[0u8; 16]);
    frame(0x00, &p)
}

/// Reads a VarInt and how many bytes it took.
async fn read_varint<R: AsyncRead + Unpin>(r: &mut R) -> std::io::Result<(i32, usize)> {
    let mut v: u32 = 0;
    for i in 0..5 {
        let b = r.read_u8().await?;
        v |= u32::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            return Ok((v as i32, i + 1));
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "varint too long",
    ))
}

fn bad(why: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, why)
}

/// What the server sent back.
enum Reply {
    /// A packet with this id whose payload starts with a string.
    Text(i32, String),
    /// Same, but the string is longer than we read: something answered, but we can't use it.
    TooLong(i32),
    /// A packet with an empty payload.
    Other,
}

async fn read_reply<R: AsyncRead + Unpin>(r: &mut R) -> std::io::Result<Reply> {
    let (len, _) = read_varint(r).await?;
    let len = usize::try_from(len).map_err(|_| bad("negative packet length"))?;
    let (id, id_len) = read_varint(r).await?;
    let rest = len.checked_sub(id_len).ok_or_else(|| bad("short packet"))?;
    if rest == 0 {
        return Ok(Reply::Other);
    }
    let (n, n_len) = read_varint(r).await?;
    let n = usize::try_from(n).map_err(|_| bad("negative string length"))?;
    if n > MAX_STRING {
        return Ok(Reply::TooLong(id));
    }
    if n_len + n > rest {
        return Err(bad("string runs past its packet"));
    }
    let mut bytes = vec![0u8; n];
    r.read_exact(&mut bytes).await?;
    Ok(Reply::Text(
        id,
        String::from_utf8_lossy(&bytes).into_owned(),
    ))
}

async fn connect(address: &ServerAddress) -> std::io::Result<TcpStream> {
    let stream = timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((address.host.as_str(), address.port)),
    )
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "connect timed out"))??;
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

async fn status_exchange(address: &ServerAddress) -> std::io::Result<Reply> {
    let mut stream = connect(address).await?;
    timeout(EXCHANGE_TIMEOUT, async {
        stream
            .write_all(&handshake(&address.host, address.port, 1))
            .await?;
        stream.write_all(&frame(0x00, &[])).await?;
        read_reply(&mut stream).await
    })
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "exchange timed out"))?
}

/// Asks who holds the address: a sleeping server's waker, a running server, or nobody.
pub async fn ping(address: &ServerAddress) -> Ping {
    match status_exchange(address).await {
        Ok(Reply::Text(0, json)) => {
            let sleeping = serde_json::from_str::<serde_json::Value>(&json)
                .ok()
                .and_then(|v| v["version"]["name"].as_str().map(|n| n == SLEEPING))
                .unwrap_or(false);
            if sleeping {
                Ping::Waker
            } else {
                Ping::Server
            }
        }
        // A status packet too big to read still came from a server.
        Ok(Reply::TooLong(0)) => Ping::Server,
        _ => Ping::NoAnswer,
    }
}

/// The plain text of a Disconnect: its JSON `text`, else the raw string.
fn kick_text(raw: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(v) => v
            .get("text")
            .and_then(|t| t.as_str())
            .map_or_else(|| raw.to_string(), str::to_string),
        Err(_) => raw.to_string(),
    }
}

fn classify(text: &str) -> Knock {
    if text.contains(NOT_WHITELISTED) {
        Knock::NotWhitelisted
    } else if text.contains(NO_ROOM) {
        Knock::NoRoom
    } else {
        Knock::Woken
    }
}

fn wake_failed(address: &ServerAddress, e: impl std::fmt::Display) -> LauncherError {
    LauncherError::network(ErrorCode::ServerWakeFailed, "Couldn't wake the server")
        .with_param("host", &address.host)
        .with_detail(e)
}

/// Joins as `name` so a sleeping server's waker starts the server, and reads its kick. The
/// connection never gets past login, so nothing is left on a running server either.
///
/// Anything that isn't a recognised refusal counts as woken: the waker's own kick is the
/// "starting" answer, and a server that answers some other way is up already.
pub async fn knock(address: &ServerAddress, name: &str) -> Result<Knock> {
    let mut stream = connect(address)
        .await
        .map_err(|e| wake_failed(address, e))?;
    let reply = timeout(EXCHANGE_TIMEOUT, async {
        stream
            .write_all(&handshake(&address.host, address.port, 2))
            .await?;
        stream.write_all(&login_start(name)).await?;
        read_reply(&mut stream).await
    })
    .await
    .map_err(|_| wake_failed(address, "the server did not answer in time"))?
    .map_err(|e| wake_failed(address, e))?;
    Ok(match reply {
        // Login-state Disconnect.
        Reply::Text(0, raw) => classify(&kick_text(&raw)),
        _ => Knock::Woken,
    })
}

/// Pings every `every` until a running server answers, `limit` passes, or `cancel` fires.
pub async fn wait_until_up(
    address: &ServerAddress,
    cancel: &Cancel,
    every: Duration,
    limit: Duration,
) -> WaitEnd {
    let deadline = Instant::now() + limit;
    loop {
        if cancel.is_cancelled() {
            return WaitEnd::Cancelled;
        }
        tokio::select! {
            _ = cancel.cancelled() => return WaitEnd::Cancelled,
            answer = timeout_at(deadline, ping(address)) => match answer {
                Ok(Ping::Server) => return WaitEnd::Up,
                Ok(_) => {}
                Err(_) => return WaitEnd::TimedOut,
            },
        }
        if Instant::now() + every >= deadline {
            // The next ping would start after the limit.
            tokio::select! {
                _ = cancel.cancelled() => return WaitEnd::Cancelled,
                _ = tokio::time::sleep_until(deadline) => return WaitEnd::TimedOut,
            }
        }
        tokio::select! {
            _ = cancel.cancelled() => return WaitEnd::Cancelled,
            _ = tokio::time::sleep(every) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn handshake_bytes() {
        assert_eq!(
            handshake("a.b", 25565, 2),
            [10, 0x00, 0xFF, 0x05, 3, b'a', b'.', b'b', 0x63, 0xDD, 0x02]
        );
    }

    #[tokio::test]
    async fn varint_round_trips() {
        for v in [0, 127, 128, 25565, i32::MAX] {
            let mut buf = Vec::new();
            push_varint(&mut buf, v);
            let (back, used) = read_varint(&mut buf.as_slice()).await.unwrap();
            assert_eq!((back, used), (v, buf.len()));
        }
    }

    /// What a fake answers to a status request.
    #[derive(Clone)]
    enum Fake {
        /// Status says `Sleeping`; a login is kicked with this text.
        Waker(String),
        /// Status says `Paper 1.21.1`.
        Server,
    }

    struct Running {
        address: ServerAddress,
        /// The names the fake saw in login starts.
        names: Arc<Mutex<Vec<String>>>,
    }

    fn address_of(port: u16) -> ServerAddress {
        ServerAddress {
            host: "127.0.0.1".into(),
            port,
        }
    }

    async fn read_packet(s: &mut TcpStream) -> std::io::Result<(i32, Vec<u8>)> {
        let (len, _) = read_varint(s).await?;
        let (id, id_len) = read_varint(s).await?;
        let mut body = vec![0u8; len as usize - id_len];
        s.read_exact(&mut body).await?;
        Ok((id, body))
    }

    fn read_string(body: &[u8]) -> (String, &[u8]) {
        let mut slice = body;
        let mut v = 0usize;
        let mut shift = 0;
        loop {
            let b = slice[0];
            slice = &slice[1..];
            v |= usize::from(b & 0x7F) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                break;
            }
        }
        (String::from_utf8(slice[..v].to_vec()).unwrap(), &slice[v..])
    }

    /// Serves one connection like the Engine's `Handle`.
    async fn serve(mut s: TcpStream, fake: Fake, names: Arc<Mutex<Vec<String>>>) {
        let Ok((0, hs)) = read_packet(&mut s).await else {
            return;
        };
        // protocol varint, host string, port, next state
        let (_, after_protocol) = {
            let mut i = 0;
            while hs[i] & 0x80 != 0 {
                i += 1;
            }
            ((), &hs[i + 1..])
        };
        let (_host, rest) = read_string(after_protocol);
        let next = rest[2];
        match next {
            1 => {
                let Ok((0, _)) = read_packet(&mut s).await else {
                    return;
                };
                let name = match fake {
                    Fake::Waker(_) => "Sleeping",
                    Fake::Server => "Paper 1.21.1",
                };
                let json = format!(
                    r#"{{"version":{{"name":"{name}","protocol":767}},"players":{{"max":20,"online":0}},"description":{{"text":"hi"}}}}"#
                );
                let mut p = Vec::new();
                push_string(&mut p, &json);
                let _ = s.write_all(&frame(0x00, &p)).await;
            }
            2 => {
                let Ok((0, body)) = read_packet(&mut s).await else {
                    return;
                };
                let (name, _) = read_string(&body);
                names.lock().unwrap().push(name);
                if let Fake::Waker(text) = fake {
                    let json = serde_json::json!({ "text": text }).to_string();
                    let mut p = Vec::new();
                    push_string(&mut p, &json);
                    let _ = s.write_all(&frame(0x00, &p)).await;
                }
            }
            _ => {}
        }
        let _ = s.shutdown().await;
    }

    async fn run(listener: TcpListener, fake: Fake) -> Running {
        let port = listener.local_addr().unwrap().port();
        let names = Arc::new(Mutex::new(Vec::new()));
        let seen = names.clone();
        tokio::spawn(async move {
            loop {
                let Ok((s, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(serve(s, fake.clone(), seen.clone()));
            }
        });
        Running {
            address: address_of(port),
            names,
        }
    }

    async fn fake(f: Fake) -> Running {
        run(TcpListener::bind("127.0.0.1:0").await.unwrap(), f).await
    }

    /// A port nothing listens on.
    async fn closed_port() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    }

    async fn knock_with(text: &str) -> Knock {
        let w = fake(Fake::Waker(text.into())).await;
        knock(&w.address, "Steve").await.unwrap()
    }

    #[tokio::test]
    async fn ping_sees_the_waker() {
        let w = fake(Fake::Waker(String::new())).await;
        assert_eq!(ping(&w.address).await, Ping::Waker);
    }

    #[tokio::test]
    async fn ping_sees_a_server() {
        let s = fake(Fake::Server).await;
        assert_eq!(ping(&s.address).await, Ping::Server);
    }

    #[tokio::test]
    async fn ping_without_answer() {
        assert_eq!(ping(&address_of(closed_port().await)).await, Ping::NoAnswer);
    }

    #[tokio::test]
    async fn knock_sends_the_name() {
        let w = fake(Fake::Waker("§aWaking up...".into())).await;
        knock(&w.address, "Steve").await.unwrap();
        assert_eq!(*w.names.lock().unwrap(), ["Steve"]);
    }

    #[tokio::test]
    async fn knock_reads_not_whitelisted() {
        assert_eq!(
            knock_with("§cYou are not whitelisted on this server.").await,
            Knock::NotWhitelisted
        );
    }

    #[tokio::test]
    async fn knock_reads_no_room() {
        assert_eq!(
            knock_with("§fasalith §ccannot start right now.\n\nThe team has no room.").await,
            Knock::NoRoom
        );
    }

    #[tokio::test]
    async fn knock_reads_woken() {
        assert_eq!(knock_with("§aWaking up §fasalith§a...").await, Knock::Woken);
    }

    #[tokio::test]
    async fn knock_on_a_closed_port_fails() {
        let err = knock(&address_of(closed_port().await), "Steve")
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ServerWakeFailed);
        assert_eq!(
            err.params.get("host").map(String::as_str),
            Some("127.0.0.1")
        );
    }

    #[tokio::test]
    async fn wait_until_up_returns_up() {
        let port = closed_port().await;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let l = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
            run(l, Fake::Server).await;
            // keep the accept task alive for the test's duration
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let end = wait_until_up(
            &address_of(port),
            &Cancel::default(),
            Duration::from_millis(20),
            Duration::from_secs(3),
        )
        .await;
        assert_eq!(end, WaitEnd::Up);
    }

    #[tokio::test]
    async fn wait_until_up_keeps_waiting_on_a_waker() {
        let w = fake(Fake::Waker(String::new())).await;
        let end = wait_until_up(
            &w.address,
            &Cancel::default(),
            Duration::from_millis(10),
            Duration::from_millis(80),
        )
        .await;
        assert_eq!(end, WaitEnd::TimedOut);
    }

    #[tokio::test]
    async fn wait_until_up_times_out() {
        let started = Instant::now();
        let end = wait_until_up(
            &address_of(closed_port().await),
            &Cancel::default(),
            Duration::from_millis(20),
            Duration::from_millis(60),
        )
        .await;
        assert_eq!(end, WaitEnd::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn wait_until_up_cancels() {
        let cancel = Cancel::default();
        let c = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            c.cancel();
        });
        let started = Instant::now();
        let end = wait_until_up(
            &address_of(closed_port().await),
            &cancel,
            Duration::from_millis(500),
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(end, WaitEnd::Cancelled);
        assert!(started.elapsed() < Duration::from_millis(400));
    }
}

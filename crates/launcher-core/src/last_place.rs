//! Where the player last was in an instance: the server they connected to or the world they
//! played, kept in `<instance>/.perland/last-place.json` (like `playtime.json`, it moves with a
//! rename and goes with a delete). Home's Continue takes them back there.
//!
//! A server comes from the game's own `Connecting to <host>, <port>` line, a world from the
//! newest `saves/<folder>/level.dat` (or `session.lock`). Neither is trusted further than the
//! files as they are now: a deleted world or a server that left the player's lists is nowhere.
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::{error::Result, fsutil::write_atomic, instances::ServerAddress, server_list};

const FILE: &str = "last-place.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LastPlace {
    #[serde(rename_all = "camelCase")]
    Server {
        host: String,
        port: u16,
        /// It was the instance's own Perland server, which may move: Continue follows it.
        #[serde(default)]
        own: bool,
        /// Unix seconds when the game connected.
        at: u64,
    },
    #[serde(rename_all = "camelCase")]
    World {
        /// The folder under `saves/`: what `--quickPlaySingleplayer` takes.
        folder: String,
        /// Unix seconds of the world's last save.
        at: u64,
    },
}

fn path(perland_dir: &Path) -> PathBuf {
    perland_dir.join(FILE)
}

/// The stored place. Missing, unreadable or damaged reads as nowhere: never worth an error.
pub async fn read(perland_dir: &Path) -> Option<LastPlace> {
    let bytes = tokio::fs::read(path(perland_dir)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub async fn write(perland_dir: &Path, place: &LastPlace) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(place).map_err(std::io::Error::other)?;
    write_atomic(&path(perland_dir), &bytes).await?;
    Ok(())
}

/// The server in the game's `Connecting to <host>, <port>` line, plain or inside a log4j XML
/// event. The host and port pass the same check as every address that reaches the game.
pub fn connect_line(line: &str) -> Option<ServerAddress> {
    const MARK: &str = "Connecting to ";
    let rest = &line[line.rfind(MARK)? + MARK.len()..];
    let rest = rest.split("]]>").next().unwrap_or(rest);
    let (host, port) = rest.trim().rsplit_once(", ")?;
    ServerAddress::new(host.trim(), port.trim().parse().ok()?)
}

/// A LAN or local address (loopback, private, link-local, unique-local). A LAN world gets a new
/// port each time it opens, so it's only worth continuing when the player listed it.
pub fn is_local(address: &ServerAddress) -> bool {
    match address.host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            let first = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
        }
        Err(_) => host_is_local_name(&address.host),
    }
}

fn host_is_local_name(host: &str) -> bool {
    host == "localhost" || host.ends_with(".local")
}

/// Can this Minecraft version open a world straight away (`--quickPlaySingleplayer`)? Releases
/// from 1.20, their pre-releases and release candidates, snapshots from 23w14a, and the
/// year-numbered versions (26.1 on). Anything unrecognised is no.
pub fn opens_worlds(minecraft: &str) -> bool {
    if let Some((year, week)) = snapshot(minecraft) {
        return (year, week) >= (23, 14);
    }
    let base = minecraft.split('-').next().unwrap_or(minecraft);
    let mut parts = base.split('.').map(|p| p.parse::<u32>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(1), Some(minor)) => minor >= 20,
        (Some(major), _) => major >= 25,
        _ => false,
    }
}

/// `YYwWWx` (23w14a): the year and week.
fn snapshot(id: &str) -> Option<(u32, u32)> {
    let (year, rest) = id.split_once('w')?;
    if year.len() != 2 || rest.len() != 3 || !rest.as_bytes()[2].is_ascii_lowercase() {
        return None;
    }
    Some((year.parse().ok()?, rest[..2].parse().ok()?))
}

/// One plain folder name: no separators, not `.` or `..`. A record never steps outside `saves/`.
pub fn safe_folder(folder: &str) -> bool {
    !folder.is_empty()
        && folder != "."
        && folder != ".."
        && !folder.contains(['/', '\\', '\0'])
        && folder.trim() == folder
}

/// The world still exists: `saves/<folder>/level.dat` is a file.
pub async fn world_exists(game_dir: &Path, folder: &str) -> bool {
    safe_folder(folder)
        && tokio::fs::metadata(game_dir.join("saves").join(folder).join("level.dat"))
            .await
            .is_ok_and(|m| m.is_file())
}

/// The world played last, if it was saved or opened at or after `since`: its folder and when.
pub async fn latest_world(game_dir: &Path, since: SystemTime) -> Option<(String, SystemTime)> {
    let mut dir = tokio::fs::read_dir(game_dir.join("saves")).await.ok()?;
    let mut best: Option<(String, SystemTime)> = None;
    while let Ok(Some(entry)) = dir.next_entry().await {
        let Ok(folder) = entry.file_name().into_string() else {
            continue;
        };
        if !safe_folder(&folder) {
            continue;
        }
        let modified = |name: &'static str| {
            let file = entry.path().join(name);
            async move {
                tokio::fs::metadata(file)
                    .await
                    .ok()
                    .filter(|m| m.is_file())
                    .and_then(|m| m.modified().ok())
            }
        };
        let Some(level) = modified("level.dat").await else {
            continue;
        };
        let time = modified("session.lock")
            .await
            .map_or(level, |l| l.max(level));
        if time >= since && best.as_ref().is_none_or(|(_, t)| time > *t) {
            best = Some((folder, time));
        }
    }
    best
}

/// What the player calls `address` in this game folder: its name in the Multiplayer list (the
/// address when the name is empty), or the address when it's the Direct Connect one. `None`
/// when the player can no longer see it in the game.
pub async fn listed_name(game_dir: &Path, address: &ServerAddress) -> Option<String> {
    if let Ok(bytes) = tokio::fs::read(game_dir.join(server_list::FILE_NAME)).await {
        let entries = server_list::entries(&bytes).unwrap_or_default();
        if let Some((name, _)) = entries
            .iter()
            .find(|(_, ip)| server_list::same_address(ip, address))
        {
            let name = name.trim();
            return Some(if name.is_empty() {
                label(address)
            } else {
                name.to_string()
            });
        }
    }
    let options = tokio::fs::read(game_dir.join("options.txt")).await.ok()?;
    String::from_utf8_lossy(&options)
        .lines()
        .filter_map(|line| line.strip_prefix("lastServer:"))
        .any(|ip| server_list::same_address(ip, address))
        .then(|| label(address))
}

/// An address the way a player types it: the port only when it isn't the default.
pub fn label(address: &ServerAddress) -> String {
    if address.port == 25565 {
        address.host.clone()
    } else {
        address.display()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn addr(host: &str, port: i64) -> ServerAddress {
        ServerAddress::new(host, port).unwrap()
    }

    #[test]
    fn connect_line_reads_plain_lines() {
        let line = "[12:00:01] [Render thread/INFO]: Connecting to play.example.com, 25565";
        assert_eq!(connect_line(line), Some(addr("play.example.com", 25565)));
    }

    #[test]
    fn connect_line_reads_xml_events() {
        let line =
            "    <log4j:Message><![CDATA[Connecting to Play.Example.com, 25566]]></log4j:Message>";
        assert_eq!(connect_line(line), Some(addr("play.example.com", 25566)));
    }

    #[test]
    fn connect_line_takes_ipv6() {
        let line = "[Render thread/INFO]: Connecting to 2001:db8::1, 25565";
        assert_eq!(connect_line(line), Some(addr("2001:db8::1", 25565)));
    }

    #[test]
    fn connect_line_ignores_others() {
        for line in [
            "Connecting to",
            "Connecting to bad host, 1",
            "Connecting to a.b, 0",
            "Connecting to a.b, 70000",
            "[INFO]: Loaded 7 recipes",
        ] {
            assert_eq!(connect_line(line), None, "{line}");
        }
    }

    #[test]
    fn opens_worlds_by_version() {
        for yes in [
            "1.20",
            "1.20.1",
            "1.21.4",
            "1.20-pre1",
            "1.20.5-rc2",
            "23w14a",
            "24w01a",
            "26.1",
        ] {
            assert!(opens_worlds(yes), "{yes}");
        }
        for no in [
            "1.19.4",
            "1.19.4-rc1",
            "23w13a",
            "1.12.2",
            "b1.7.3",
            "rd-132211",
            "",
            "fabric",
        ] {
            assert!(!opens_worlds(no), "{no}");
        }
    }

    #[test]
    fn unsafe_world_folders_never_show() {
        for bad in ["", ".", "..", "../escape", "a/b", "a\\b", " padded"] {
            assert!(!safe_folder(bad), "{bad:?}");
        }
        assert!(safe_folder("Mundo de Ana"));
        assert!(safe_folder("New World (1)"));
    }

    #[test]
    fn is_local_spots_lan_addresses() {
        for local in [
            "192.168.1.5",
            "127.0.0.1",
            "10.0.0.2",
            "fe80::1",
            "::1",
            "localhost",
        ] {
            assert!(is_local(&addr(local, 25565)), "{local}");
        }
        for public in ["play.example.com", "8.8.8.8", "2001:db8::1"] {
            assert!(!is_local(&addr(public, 25565)), "{public}");
        }
    }

    fn world(game: &Path, folder: &str, at: SystemTime) {
        let dir = game.join("saves").join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let file = std::fs::File::create(dir.join("level.dat")).unwrap();
        file.set_modified(at).unwrap();
    }

    #[tokio::test]
    async fn latest_world_picks_the_newest_since_the_start() {
        let tmp = tempfile::tempdir().unwrap();
        let start = SystemTime::now() - Duration::from_secs(600);
        world(tmp.path(), "Old", start - Duration::from_secs(60));
        world(tmp.path(), "Mundo de Ana", start + Duration::from_secs(120));
        world(tmp.path(), "Other", start + Duration::from_secs(60));
        std::fs::create_dir_all(tmp.path().join("saves/No level")).unwrap();
        let (folder, at) = latest_world(tmp.path(), start).await.unwrap();
        assert_eq!(folder, "Mundo de Ana");
        assert_eq!(at, start + Duration::from_secs(120));
        assert_eq!(
            latest_world(tmp.path(), start + Duration::from_secs(300)).await,
            None
        );
        assert_eq!(latest_world(&tmp.path().join("none"), start).await, None);
    }

    #[tokio::test]
    async fn opening_a_world_counts_through_its_session_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let start = SystemTime::now() - Duration::from_secs(600);
        world(tmp.path(), "Crashed", start - Duration::from_secs(60));
        let lock = std::fs::File::create(tmp.path().join("saves/Crashed/session.lock")).unwrap();
        lock.set_modified(start + Duration::from_secs(5)).unwrap();
        let (folder, _) = latest_world(tmp.path(), start).await.unwrap();
        assert_eq!(folder, "Crashed");
    }

    #[tokio::test]
    async fn world_exists_needs_its_level() {
        let tmp = tempfile::tempdir().unwrap();
        world(tmp.path(), "Mundo", SystemTime::now());
        assert!(world_exists(tmp.path(), "Mundo").await);
        assert!(!world_exists(tmp.path(), "Gone").await);
        assert!(!world_exists(tmp.path(), "../Mundo").await);
    }

    #[tokio::test]
    async fn read_write_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".perland");
        let place = LastPlace::Server {
            host: "play.example.com".into(),
            port: 25566,
            own: true,
            at: 1_792_400_000,
        };
        write(&dir, &place).await.unwrap();
        assert_eq!(read(&dir).await, Some(place));
        let raw = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(raw.contains("\"kind\": \"server\""), "{raw}");
        let world = LastPlace::World {
            folder: "Mundo".into(),
            at: 1,
        };
        write(&dir, &world).await.unwrap();
        assert_eq!(read(&dir).await, Some(world));
    }

    #[tokio::test]
    async fn damaged_file_reads_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read(tmp.path()).await, None);
        std::fs::write(tmp.path().join(FILE), b"nope").unwrap();
        assert_eq!(read(tmp.path()).await, None);
        std::fs::write(tmp.path().join(FILE), br#"{"kind":"realm"}"#).unwrap();
        assert_eq!(read(tmp.path()).await, None);
    }

    #[tokio::test]
    async fn listed_name_finds_the_server_list_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let listed = addr("play.example.com", 25566);
        let (_, bytes) = server_list::with_server(None, "Asalith Fields", &listed, None).unwrap();
        std::fs::write(tmp.path().join(server_list::FILE_NAME), bytes.unwrap()).unwrap();
        assert_eq!(
            listed_name(tmp.path(), &listed).await.as_deref(),
            Some("Asalith Fields")
        );
        assert_eq!(
            listed_name(tmp.path(), &addr("play.example.com", 25565)).await,
            None
        );
    }

    #[tokio::test]
    async fn listed_name_falls_back_to_direct_connect() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("options.txt"),
            "version:3465\nlastServer:MC.example.org\nlang:es_mx\n",
        )
        .unwrap();
        let address = addr("mc.example.org", 25565);
        assert_eq!(
            listed_name(tmp.path(), &address).await.as_deref(),
            Some("mc.example.org")
        );
        assert_eq!(
            listed_name(tmp.path(), &addr("other.org", 25565)).await,
            None
        );
    }
}

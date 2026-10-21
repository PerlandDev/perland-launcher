//! The game's multiplayer server list (`<game dir>/servers.dat`): a server instance's server is
//! put at the top so Play opens the game as usual and the player picks it from Multiplayer.
//!
//! The file is uncompressed big-endian NBT: a root compound holding a `servers` list of
//! compounds (`name`, `ip`, and optional `icon`, `acceptTextures`, `hidden`, …). The player's
//! own entries are kept byte for byte: every tag round-trips, and strings stay raw (Java's
//! modified UTF-8) unless this module writes them.
use std::path::Path;

use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::write_atomic,
    instances::ServerAddress,
};

pub const FILE_NAME: &str = "servers.dat";

/// Minecraft's own nesting limit for NBT.
const MAX_DEPTH: usize = 512;

const DEFAULT_PORT: &str = "25565";

#[derive(Debug, Clone, PartialEq)]
enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    /// Kept as bits so a NaN round-trips exactly.
    Float(u32),
    Double(u64),
    ByteArray(Vec<u8>),
    /// Raw modified UTF-8.
    String(Vec<u8>),
    /// The element type id, and the elements.
    List(u8, Vec<Tag>),
    Compound(Fields),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(..) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
}

/// A compound's named tags, in file order.
type Fields = Vec<(Vec<u8>, Tag)>;

/// Why a file couldn't be read; only ever shown as detail.
type ParseResult<T> = std::result::Result<T, &'static str>;

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> ParseResult<&'a [u8]> {
        if n > self.bytes.len() {
            return Err("unexpected end of file");
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> ParseResult<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("took N bytes"))
    }

    fn u8(&mut self) -> ParseResult<u8> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> ParseResult<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn i32(&mut self) -> ParseResult<i32> {
        Ok(i32::from_be_bytes(self.array()?))
    }

    fn i64(&mut self) -> ParseResult<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }

    /// A length prefix, refused when the file can't hold `len` items of `size` bytes, so a
    /// damaged length never allocates more than the file.
    fn len(&mut self, size: usize) -> ParseResult<usize> {
        let len = usize::try_from(self.i32()?).map_err(|_| "negative length")?;
        if len.saturating_mul(size) > self.bytes.len() {
            return Err("length past the end of file");
        }
        Ok(len)
    }

    fn string(&mut self) -> ParseResult<Vec<u8>> {
        let len = self.u16()? as usize;
        Ok(self.take(len)?.to_vec())
    }

    fn payload(&mut self, id: u8, depth: usize) -> ParseResult<Tag> {
        if depth > MAX_DEPTH {
            return Err("nested too deeply");
        }
        Ok(match id {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.u16()? as i16),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(self.i32()? as u32),
            6 => Tag::Double(self.i64()? as u64),
            7 => {
                let len = self.len(1)?;
                Tag::ByteArray(self.take(len)?.to_vec())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let element = self.u8()?;
                // Every element takes at least a byte, except an empty compound's end tag.
                let len = self.len(1)?;
                if element == 0 && len > 0 {
                    return Err("list of end tags");
                }
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.payload(element, depth + 1)?);
                }
                Tag::List(element, items)
            }
            10 => {
                let mut fields = Vec::new();
                loop {
                    let id = self.u8()?;
                    if id == 0 {
                        break;
                    }
                    let name = self.string()?;
                    fields.push((name, self.payload(id, depth + 1)?));
                }
                Tag::Compound(fields)
            }
            11 => {
                let len = self.len(4)?;
                Tag::IntArray((0..len).map(|_| self.i32()).collect::<ParseResult<_>>()?)
            }
            12 => {
                let len = self.len(8)?;
                Tag::LongArray((0..len).map(|_| self.i64()).collect::<ParseResult<_>>()?)
            }
            _ => return Err("unknown tag type"),
        })
    }
}

/// The root compound's name and fields.
fn parse(bytes: &[u8]) -> ParseResult<(Vec<u8>, Fields)> {
    let mut reader = Reader { bytes };
    if reader.u8()? != 10 {
        return Err("the root isn't a compound");
    }
    let name = reader.string()?;
    let Tag::Compound(fields) = reader.payload(10, 0)? else {
        unreachable!("payload 10 is a compound");
    };
    Ok((name, fields))
}

fn put_string(out: &mut Vec<u8>, s: &[u8]) {
    // Every string here was read with a u16 length or is clamped by `modified_utf8`.
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s);
}

fn put_payload(out: &mut Vec<u8>, tag: &Tag) {
    match tag {
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Double(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::ByteArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            out.extend_from_slice(v);
        }
        Tag::String(v) => put_string(out, v),
        Tag::List(element, items) => {
            out.push(*element);
            out.extend_from_slice(&(items.len() as i32).to_be_bytes());
            for item in items {
                put_payload(out, item);
            }
        }
        Tag::Compound(fields) => {
            for (name, value) in fields {
                out.push(value.id());
                put_string(out, name);
                put_payload(out, value);
            }
            out.push(0);
        }
        Tag::IntArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for i in v {
                out.extend_from_slice(&i.to_be_bytes());
            }
        }
        Tag::LongArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for i in v {
                out.extend_from_slice(&i.to_be_bytes());
            }
        }
    }
}

fn serialize(root_name: &[u8], fields: &[(Vec<u8>, Tag)]) -> Vec<u8> {
    let mut out = vec![10];
    put_string(&mut out, root_name);
    for (name, value) in fields {
        out.push(value.id());
        put_string(&mut out, name);
        put_payload(&mut out, value);
    }
    out.push(0);
    out
}

/// Java's modified UTF-8 (what NBT strings use): NUL as `C0 80`, and characters outside the
/// BMP as two encoded surrogates. Cut at a character boundary to fit the u16 length.
fn modified_utf8(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        let mut units = [0u16; 2];
        let mut encoded = Vec::with_capacity(6);
        for &unit in c.encode_utf16(&mut units).iter() {
            match unit {
                0x0001..=0x007F => encoded.push(unit as u8),
                0x0000 | 0x0080..=0x07FF => {
                    encoded.push(0xC0 | (unit >> 6) as u8);
                    encoded.push(0x80 | (unit & 0x3F) as u8);
                }
                _ => {
                    encoded.push(0xE0 | (unit >> 12) as u8);
                    encoded.push(0x80 | ((unit >> 6) & 0x3F) as u8);
                    encoded.push(0x80 | (unit & 0x3F) as u8);
                }
            }
        }
        if out.len() + encoded.len() > u16::MAX as usize {
            break;
        }
        out.extend_from_slice(&encoded);
    }
    out
}

/// `host:port` the way the game reads an address: lowercase, a missing port is 25565, and an
/// IPv6 literal keeps its brackets. Raw bytes that aren't text never match.
fn normalize(ip: &[u8]) -> Option<String> {
    let ip = std::str::from_utf8(ip).ok()?.trim().to_ascii_lowercase();
    if ip.is_empty() {
        return None;
    }
    if let Some(rest) = ip.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = after.strip_prefix(':').unwrap_or(DEFAULT_PORT);
        return Some(format!("[{host}]:{port}"));
    }
    match ip.matches(':').count() {
        0 => Some(format!("{ip}:{DEFAULT_PORT}")),
        1 => Some(ip),
        // A bare IPv6 literal has no port.
        _ => Some(format!("[{ip}]:{DEFAULT_PORT}")),
    }
}

fn field<'t>(fields: &'t [(Vec<u8>, Tag)], name: &str) -> Option<&'t Tag> {
    fields
        .iter()
        .find(|(n, _)| n == name.as_bytes())
        .map(|(_, t)| t)
}

fn set_field(fields: &mut Fields, name: &str, value: Tag) {
    match fields.iter_mut().find(|(n, _)| n == name.as_bytes()) {
        Some((_, slot)) => *slot = value,
        None => fields.push((name.as_bytes().to_vec(), value)),
    }
}

fn entry_ip(entry: &Tag) -> Option<String> {
    match entry {
        Tag::Compound(fields) => match field(fields, "ip") {
            Some(Tag::String(ip)) => normalize(ip),
            _ => None,
        },
        _ => None,
    }
}

/// The game keeps quick-play and direct-connect servers as hidden entries, out of the list.
fn is_hidden(entry: &Tag) -> bool {
    matches!(entry, Tag::Compound(f) if matches!(field(f, "hidden"), Some(Tag::Byte(b)) if *b != 0))
}

fn unhide(entry: &mut Tag) {
    if let Tag::Compound(fields) = entry {
        if field(fields, "hidden").is_some() {
            set_field(fields, "hidden", Tag::Byte(0));
        }
    }
}

/// One entry of the list as text: `(name, ip)`. Hidden entries are left out, as in the game.
pub fn entries(bytes: &[u8]) -> std::result::Result<Vec<(String, String)>, &'static str> {
    let (_, fields) = parse(bytes)?;
    let servers = match field(&fields, "servers") {
        None => return Ok(Vec::new()),
        Some(Tag::List(10 | 0, items)) => items,
        Some(_) => return Err("servers isn't a list of compounds"),
    };
    let text = |fields: &[(Vec<u8>, Tag)], name| match field(fields, name) {
        Some(Tag::String(s)) => String::from_utf8_lossy(s).into_owned(),
        _ => String::new(),
    };
    Ok(servers
        .iter()
        .filter(|s| !is_hidden(s))
        .filter_map(|s| match s {
            Tag::Compound(f) => Some((text(f, "name"), text(f, "ip"))),
            _ => None,
        })
        .collect())
}

/// What [`ensure_listed`] did to the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listed {
    /// The server was already in the list; nothing was written.
    Already,
    /// The server's earlier address was in the list and now points at the new one.
    Moved,
    /// A new entry went to the top of the list.
    Added,
    /// The server's entry kept its place and now carries the server's new name.
    Renamed,
}

fn failed(address: &ServerAddress, detail: impl std::fmt::Display) -> LauncherError {
    LauncherError::game(
        ErrorCode::ServerListFailed,
        "Couldn't add the server to the game's server list",
    )
    .with_fix(format!("Add {} in Multiplayer by hand", address.display()))
    .with_param("address", address.display())
    .with_detail(detail)
}

/// The list with `address` in it: left alone when it already lists the server (the player may
/// have renamed or moved it), the entry for `previous` repointed when the server moved, else a
/// new `name` entry at the top. A hidden entry for the server becomes visible at the top. A file
/// that isn't valid NBT is never overwritten: that would lose the player's servers.
///
/// `ours` are the names the launcher wrote for this server before: an entry still named one of
/// them takes `name` (the server's display name changed), while a name the player typed stays.
pub fn with_server(
    current: Option<&[u8]>,
    name: &str,
    address: &ServerAddress,
    previous: Option<&ServerAddress>,
    ours: &[&str],
) -> std::result::Result<(Listed, Option<Vec<u8>>), &'static str> {
    let (root_name, mut fields) = match current {
        Some(bytes) => parse(bytes)?,
        None => (Vec::new(), Vec::new()),
    };
    let mut servers = match field(&fields, "servers") {
        None => Vec::new(),
        // An empty list may be typed `end`.
        Some(Tag::List(10 | 0, items)) => items.clone(),
        Some(_) => return Err("servers isn't a list of compounds"),
    };
    let target = address.display();
    let wanted = normalize(target.as_bytes());
    let listed = if let Some(i) = servers.iter().position(|s| entry_ip(s) == wanted) {
        let renamed = rename_ours(&mut servers[i], name, ours);
        if !is_hidden(&servers[i]) {
            if !renamed {
                return Ok((Listed::Already, None));
            }
            set_field(&mut fields, "servers", Tag::List(10, servers));
            return Ok((Listed::Renamed, Some(serialize(&root_name, &fields))));
        }
        let mut entry = servers.remove(i);
        unhide(&mut entry);
        servers.insert(0, entry);
        Listed::Added
    } else if let Some(i) = previous
        .map(|p| normalize(p.display().as_bytes()))
        .and_then(|old| servers.iter().position(|s| entry_ip(s) == old))
    {
        rename_ours(&mut servers[i], name, ours);
        if let Tag::Compound(entry) = &mut servers[i] {
            set_field(entry, "ip", Tag::String(target.into_bytes()));
        }
        if is_hidden(&servers[i]) {
            let mut entry = servers.remove(i);
            unhide(&mut entry);
            servers.insert(0, entry);
        }
        Listed::Moved
    } else {
        servers.insert(
            0,
            Tag::Compound(vec![
                (b"name".to_vec(), Tag::String(modified_utf8(name))),
                (b"ip".to_vec(), Tag::String(target.into_bytes())),
            ]),
        );
        Listed::Added
    };
    set_field(&mut fields, "servers", Tag::List(10, servers));
    Ok((listed, Some(serialize(&root_name, &fields))))
}

/// Gives `entry` the name `name` when it is still named one of `ours`, the names the launcher
/// wrote; returns whether it changed.
fn rename_ours(entry: &mut Tag, name: &str, ours: &[&str]) -> bool {
    let Tag::Compound(fields) = entry else {
        return false;
    };
    let wanted = modified_utf8(name);
    let current = match field(fields, "name") {
        Some(Tag::String(raw)) => raw.clone(),
        _ => return false,
    };
    if current == wanted || !ours.iter().any(|o| modified_utf8(o) == current) {
        return false;
    }
    set_field(fields, "name", Tag::String(wanted));
    true
}

/// Put `address` in `<game_dir>/servers.dat` (see [`with_server`]). `previous` is the address
/// this was last called with for the instance, so a server that moved keeps its one entry.
pub async fn ensure_listed(
    game_dir: &Path,
    name: &str,
    address: &ServerAddress,
    previous: Option<&ServerAddress>,
    ours: &[&str],
) -> Result<Listed> {
    let path = game_dir.join(FILE_NAME);
    let current = match tokio::fs::read(&path).await {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(failed(address, e)),
    };
    let (listed, bytes) = with_server(current.as_deref(), name, address, previous, ours)
        .map_err(|why| failed(address, format!("{}: {why}", path.display())))?;
    if let Some(bytes) = bytes {
        write_atomic(&path, &bytes)
            .await
            .map_err(|e| failed(address, e.detail.unwrap_or(e.message)))?;
    }
    Ok(listed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(host: &str, port: i64) -> ServerAddress {
        ServerAddress::new(host, port).unwrap()
    }

    fn entry(name: &str, ip: &str) -> Tag {
        Tag::Compound(vec![
            (b"name".to_vec(), Tag::String(name.as_bytes().to_vec())),
            (b"ip".to_vec(), Tag::String(ip.as_bytes().to_vec())),
        ])
    }

    fn file(servers: Vec<Tag>) -> Vec<u8> {
        serialize(b"", &[(b"servers".to_vec(), Tag::List(10, servers))])
    }

    /// `(name, ip, hidden)` of each entry, in order.
    fn entries(bytes: &[u8]) -> Vec<(String, String, bool)> {
        let (_, fields) = parse(bytes).unwrap();
        let Some(Tag::List(_, servers)) = field(&fields, "servers") else {
            panic!("no servers list");
        };
        servers
            .iter()
            .map(|s| {
                let Tag::Compound(f) = s else { panic!() };
                let text = |n| match field(f, n) {
                    Some(Tag::String(s)) => String::from_utf8(s.clone()).unwrap(),
                    _ => String::new(),
                };
                (text("name"), text("ip"), is_hidden(s))
            })
            .collect()
    }

    /// The bytes the game writes for one server named `A` at `a.example.com` (hand-assembled).
    const GAME_FILE: &[u8] = &[
        10, 0, 0, // root compound, empty name
        9, 0, 7, b's', b'e', b'r', b'v', b'e', b'r', b's', // list "servers"
        10, 0, 0, 0, 1, // of 1 compound
        8, 0, 2, b'i', b'p', 0, 13, b'a', b'.', b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.',
        b'c', b'o', b'm', // ip
        8, 0, 4, b'n', b'a', b'm', b'e', 0, 1, b'A', // name
        1, 0, 6, b'h', b'i', b'd', b'd', b'e', b'n', 0, // hidden: 0
        0, // end of server
        0, // end of root
    ];

    #[test]
    fn reads_and_writes_the_games_bytes_unchanged() {
        let (name, fields) = parse(GAME_FILE).unwrap();
        assert_eq!(serialize(&name, &fields), GAME_FILE);
        assert_eq!(
            entries(GAME_FILE),
            vec![("A".into(), "a.example.com".into(), false)]
        );
    }

    #[test]
    fn every_tag_type_round_trips() {
        let fields = vec![
            (b"b".to_vec(), Tag::Byte(-1)),
            (b"s".to_vec(), Tag::Short(-2)),
            (b"i".to_vec(), Tag::Int(-3)),
            (b"l".to_vec(), Tag::Long(-4)),
            (b"f".to_vec(), Tag::Float(f32::NAN.to_bits() | 1)),
            (b"d".to_vec(), Tag::Double(1.5f64.to_bits())),
            (b"ba".to_vec(), Tag::ByteArray(vec![1, 255])),
            (b"str".to_vec(), Tag::String(vec![0xC0, 0x80])),
            (b"empty".to_vec(), Tag::List(0, vec![])),
            (b"c".to_vec(), Tag::Compound(vec![])),
            (b"ia".to_vec(), Tag::IntArray(vec![i32::MIN, 7])),
            (b"la".to_vec(), Tag::LongArray(vec![i64::MAX])),
        ];
        let bytes = serialize(b"root", &fields);
        assert_eq!(parse(&bytes).unwrap(), (b"root".to_vec(), fields));
    }

    #[test]
    fn damaged_files_are_refused() {
        let mut huge_list = vec![10, 0, 0, 9, 0, 1, b'x', 10];
        huge_list.extend_from_slice(&i32::MAX.to_be_bytes());
        let mut deep = vec![10, 0, 0];
        for _ in 0..600 {
            deep.extend_from_slice(&[10, 0, 0]);
        }
        for bad in [
            &[][..],
            &[8, 0, 0][..],
            &GAME_FILE[..GAME_FILE.len() - 1],
            &[10, 0, 0, 99, 0, 0][..],
            &[10, 0, 0, 7, 0, 0, 255, 255, 255, 255][..],
            &[10, 0, 0, 9, 0, 0, 0, 0, 0, 0, 1][..],
            &huge_list[..],
            &deep[..],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_missing_file_gets_the_server() {
        let (listed, bytes) =
            with_server(None, "Asalith", &addr("play.example.com", 25566), None, &[]).unwrap();
        assert_eq!(listed, Listed::Added);
        assert_eq!(
            entries(&bytes.unwrap()),
            vec![("Asalith".into(), "play.example.com:25566".into(), false)]
        );
    }

    #[test]
    fn a_new_server_goes_on_top_and_keeps_the_rest() {
        let (listed, bytes) = with_server(
            Some(GAME_FILE),
            "Asalith",
            &addr("play.example.com", 25566),
            None,
            &[],
        )
        .unwrap();
        assert_eq!(listed, Listed::Added);
        let bytes = bytes.unwrap();
        assert_eq!(
            entries(&bytes),
            vec![
                ("Asalith".into(), "play.example.com:25566".into(), false),
                ("A".into(), "a.example.com".into(), false),
            ]
        );
        assert!(
            bytes.ends_with(&GAME_FILE[18..]),
            "the player's entry is byte for byte"
        );
    }

    #[test]
    fn a_listed_server_is_left_alone() {
        let current = file(vec![
            entry("Mine", "a.example.com"),
            entry("Renamed by me", "PLAY.example.com:25566"),
        ]);
        let (listed, bytes) = with_server(
            Some(&current),
            "Asalith",
            &addr("play.example.com", 25566),
            None,
            &[],
        )
        .unwrap();
        assert_eq!(listed, Listed::Already);
        assert_eq!(bytes, None, "nothing to write");
    }

    #[test]
    fn the_default_port_matches_a_bare_host() {
        let current = file(vec![entry("Mine", "play.example.com")]);
        let (listed, _) = with_server(
            Some(&current),
            "Asalith",
            &addr("play.example.com", 25565),
            None,
            &[],
        )
        .unwrap();
        assert_eq!(listed, Listed::Already);
        let v6 = file(vec![entry("Six", "2001:db8::1")]);
        let (listed, _) =
            with_server(Some(&v6), "Asalith", &addr("2001:db8::1", 25565), None, &[]).unwrap();
        assert_eq!(listed, Listed::Already);
    }

    #[test]
    fn a_hidden_entry_comes_out_on_top() {
        let mut hidden = entry("Quick play", "play.example.com:25566");
        if let Tag::Compound(f) = &mut hidden {
            f.push((b"hidden".to_vec(), Tag::Byte(1)));
            f.push((b"acceptTextures".to_vec(), Tag::Byte(1)));
        }
        let current = file(vec![entry("Mine", "a.example.com"), hidden]);
        let (listed, bytes) = with_server(
            Some(&current),
            "Asalith",
            &addr("play.example.com", 25566),
            None,
            &[],
        )
        .unwrap();
        assert_eq!(listed, Listed::Added);
        let bytes = bytes.unwrap();
        assert_eq!(
            entries(&bytes),
            vec![
                ("Quick play".into(), "play.example.com:25566".into(), false),
                ("Mine".into(), "a.example.com".into(), false),
            ]
        );
        let (_, fields) = parse(&bytes).unwrap();
        let Some(Tag::List(_, servers)) = field(&fields, "servers") else {
            panic!()
        };
        let Tag::Compound(top) = &servers[0] else {
            panic!()
        };
        assert_eq!(
            field(top, "acceptTextures"),
            Some(&Tag::Byte(1)),
            "the player's resource pack choice stays"
        );
    }

    #[test]
    fn a_moved_server_keeps_its_entry() {
        let current = file(vec![
            entry("Mine", "a.example.com"),
            entry("My name for it", "play.example.com:25566"),
        ]);
        let (listed, bytes) = with_server(
            Some(&current),
            "Asalith",
            &addr("play.example.com", 25570),
            Some(&addr("play.example.com", 25566)),
            &[],
        )
        .unwrap();
        assert_eq!(listed, Listed::Moved);
        assert_eq!(
            entries(&bytes.unwrap()),
            vec![
                ("Mine".into(), "a.example.com".into(), false),
                (
                    "My name for it".into(),
                    "play.example.com:25570".into(),
                    false
                ),
            ]
        );
    }

    #[test]
    fn an_entry_the_launcher_named_takes_the_new_name() {
        let current = file(vec![
            entry("Mine", "a.example.com"),
            entry("survival_del_viernes", "play.example.com:25566"),
        ]);
        let (listed, bytes) = with_server(
            Some(&current),
            "Survival con los panas",
            &addr("play.example.com", 25566),
            None,
            &["survival_del_viernes"],
        )
        .unwrap();
        assert_eq!(listed, Listed::Renamed);
        assert_eq!(
            entries(&bytes.unwrap()),
            vec![
                ("Mine".into(), "a.example.com".into(), false),
                (
                    "Survival con los panas".into(),
                    "play.example.com:25566".into(),
                    false
                ),
            ],
            "the entry keeps its place"
        );
    }

    #[test]
    fn a_name_the_player_typed_stays() {
        let current = file(vec![entry("Friday night", "play.example.com:25566")]);
        let (listed, bytes) = with_server(
            Some(&current),
            "Survival con los panas",
            &addr("play.example.com", 25566),
            None,
            &["survival_del_viernes"],
        )
        .unwrap();
        assert_eq!(listed, Listed::Already);
        assert_eq!(bytes, None);
    }

    #[test]
    fn a_moved_entry_the_launcher_named_is_renamed_too() {
        let current = file(vec![entry(
            "survival_del_viernes",
            "play.example.com:25566",
        )]);
        let (listed, bytes) = with_server(
            Some(&current),
            "Survival con los panas",
            &addr("play.example.com", 25570),
            Some(&addr("play.example.com", 25566)),
            &["survival_del_viernes"],
        )
        .unwrap();
        assert_eq!(listed, Listed::Moved);
        assert_eq!(
            entries(&bytes.unwrap()),
            vec![(
                "Survival con los panas".into(),
                "play.example.com:25570".into(),
                false
            )]
        );
    }

    #[test]
    fn a_removed_old_entry_means_a_new_one() {
        let current = file(vec![entry("Mine", "a.example.com")]);
        let (listed, bytes) = with_server(
            Some(&current),
            "Asalith",
            &addr("play.example.com", 25570),
            Some(&addr("play.example.com", 25566)),
            &[],
        )
        .unwrap();
        assert_eq!(listed, Listed::Added);
        assert_eq!(entries(&bytes.unwrap()).len(), 2);
    }

    #[test]
    fn an_ipv6_server_is_written_with_brackets() {
        let (_, bytes) = with_server(None, "Six", &addr("2001:db8::1", 25566), None, &[]).unwrap();
        assert_eq!(entries(&bytes.unwrap())[0].1, "[2001:db8::1]:25566");
    }

    #[test]
    fn an_empty_list_typed_end_is_accepted() {
        let current = serialize(b"", &[(b"servers".to_vec(), Tag::List(0, vec![]))]);
        let (listed, bytes) =
            with_server(Some(&current), "A", &addr("a.example.com", 1), None, &[]).unwrap();
        assert_eq!(listed, Listed::Added);
        assert_eq!(entries(&bytes.unwrap()).len(), 1);
    }

    #[test]
    fn names_are_modified_utf8() {
        assert_eq!(modified_utf8("Aé"), b"A\xC3\xA9");
        assert_eq!(modified_utf8("\0"), [0xC0, 0x80]);
        // U+1F600 as two surrogates, three bytes each.
        assert_eq!(modified_utf8("😀"), [0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80]);
        assert!(modified_utf8(&"é".repeat(40_000)).len() <= u16::MAX as usize);
    }

    #[test]
    fn a_damaged_file_is_never_overwritten() {
        assert!(with_server(Some(b"not nbt"), "A", &addr("a.example.com", 1), None, &[]).is_err());
        let wrong = serialize(b"", &[(b"servers".to_vec(), Tag::Int(3))]);
        assert!(with_server(Some(&wrong), "A", &addr("a.example.com", 1), None, &[]).is_err());
    }

    #[tokio::test]
    async fn ensure_listed_writes_once() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("minecraft");
        let address = addr("play.example.com", 25566);
        assert_eq!(
            ensure_listed(&game, "Asalith", &address, None, &[])
                .await
                .unwrap(),
            Listed::Added
        );
        let written = std::fs::read(game.join(FILE_NAME)).unwrap();
        assert_eq!(
            ensure_listed(&game, "Asalith", &address, None, &[])
                .await
                .unwrap(),
            Listed::Already
        );
        assert_eq!(std::fs::read(game.join(FILE_NAME)).unwrap(), written);
    }

    #[tokio::test]
    async fn ensure_listed_keeps_a_damaged_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(FILE_NAME), b"junk").unwrap();
        let err = ensure_listed(tmp.path(), "A", &addr("a.example.com", 1), None, &[])
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ServerListFailed);
        assert_eq!(err.params["address"], "a.example.com:1");
        assert_eq!(std::fs::read(tmp.path().join(FILE_NAME)).unwrap(), b"junk");
    }
}

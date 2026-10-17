//! What another launcher's instance folder is: its name, Minecraft version and loader, and
//! where its game files live. Detection only prefills the form, so a missing, malformed or
//! unsupported field is `None` rather than a failure. The same goes for the settings the
//! instance kept of its own ([`Kept`]).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{
    instances::LoaderSpec,
    launch::jvm_args,
    meta::loaders::{valid_component, LoaderKind},
};

/// What [`super::inspect`] found in a picked folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Detected {
    pub name: Option<String>,
    pub minecraft: Option<String>,
    pub loader: Option<LoaderSpec>,
    /// The folder whose contents become the new instance's `minecraft/` folder.
    pub game_dir: PathBuf,
    /// True when `game_dir` is the picked folder itself, so the other launcher's metadata
    /// files sit at its top level and are left behind. Pass it to [`super::copy_game_dir`].
    pub skip_root_meta: bool,
    pub kept: Kept,
}

/// Settings the other launcher stored for this instance itself, as it stored them. A value the
/// instance left to that launcher's global settings is `None`: those aren't imported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Kept {
    /// Maximum memory, in MiB.
    pub memory_mb: Option<u32>,
    /// Extra Java arguments, one by one, before this launcher's checks.
    pub java_args: Option<Vec<String>>,
    pub playtime: Option<Played>,
}

/// Time the other launcher counted for the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Played {
    pub seconds: u64,
    /// Unix seconds when the last run ended (0: unknown).
    pub last_played: u64,
}

/// Folders and files of which at least one is in any Minecraft game folder that was played.
const GAME_MARKERS: &[&str] = &["mods", "saves", "config", "resourcepacks", "shaderpacks"];

/// `None` when the folder doesn't look like an instance at all. `dir` is canonical.
pub(super) async fn detect(dir: &Path) -> Option<Detected> {
    if let Some(found) = prism(dir).await {
        return Some(found);
    }
    // The player picked a Prism/MultiMC instance's inner game folder: describe the instance.
    let inner = matches!(
        dir.file_name().and_then(|n| n.to_str()),
        Some(".minecraft" | "minecraft")
    );
    if let (true, Some(parent)) = (inner, dir.parent()) {
        if let Some(found) = prism(parent).await {
            return Some(Detected {
                game_dir: dir.to_path_buf(),
                ..found
            });
        }
    }
    if let Some(found) = curseforge(dir).await {
        return Some(found);
    }
    if let Some(found) = modrinth(dir).await {
        return Some(found);
    }
    if let Some(found) = atlauncher(dir).await {
        return Some(found);
    }
    plain(dir).await
}

/// Prism Launcher / MultiMC: `instance.cfg` plus `mmc-pack.json`, game files in `.minecraft/`
/// (older MultiMC: `minecraft/`).
async fn prism(dir: &Path) -> Option<Detected> {
    if !is_file(&dir.join("instance.cfg")).await || !is_file(&dir.join("mmc-pack.json")).await {
        return None;
    }
    let cfg = read_text(&dir.join("instance.cfg"))
        .await
        .unwrap_or_default();
    let name = ini_value(&cfg, "name").and_then(|n| text(Some(&n)));
    let pack = read_json(&dir.join("mmc-pack.json")).await;
    let components = pack
        .as_ref()
        .and_then(|p| p.get("components"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let version_of = |uid: &str| {
        components
            .iter()
            .find(|c| c.get("uid").and_then(Value::as_str) == Some(uid))
            .and_then(|c| str_at(c, "version").or_else(|| str_at(c, "cachedVersion")))
    };
    let minecraft = version(version_of("net.minecraft"));
    let loader = [
        ("net.fabricmc.fabric-loader", LoaderKind::Fabric),
        ("org.quiltmc.quilt-loader", LoaderKind::Quilt),
        ("net.minecraftforge", LoaderKind::Forge),
        ("net.neoforged", LoaderKind::NeoForge),
    ]
    .into_iter()
    .find_map(|(uid, kind)| loader(kind, version_of(uid), minecraft.as_deref()));
    let game_dir = if is_dir(&dir.join(".minecraft")).await {
        dir.join(".minecraft")
    } else if is_dir(&dir.join("minecraft")).await {
        dir.join("minecraft")
    } else {
        // Never launched: no game files yet, so the copy brings nothing.
        dir.join(".minecraft")
    };
    Some(Detected {
        name,
        minecraft,
        loader,
        game_dir,
        skip_root_meta: false,
        kept: prism_kept(&cfg),
    })
}

/// Prism keeps an instance's own memory and Java arguments only while their `Override*` flag is
/// on; otherwise its global settings apply. Memory is in MiB, and Prism starts the game with
/// the larger of the two amounts as the maximum. `totalTimePlayed` and `lastTimePlayed` (the
/// last run's length) are seconds, `lastLaunchTime` is milliseconds since the Unix epoch.
fn prism_kept(cfg: &str) -> Kept {
    let number = |key: &str| ini_value(cfg, key).and_then(|v| v.trim().parse::<u64>().ok());
    let memory_mb = ini_bool(cfg, "OverrideMemory")
        .then(|| number("MinMemAlloc").max(number("MaxMemAlloc")))
        .flatten()
        .and_then(memory);
    let java_args = ini_bool(cfg, "OverrideJavaArgs")
        .then(|| ini_value(cfg, "JvmArgs"))
        .flatten()
        .and_then(|raw| args(&raw));
    let playtime = number("totalTimePlayed")
        .filter(|s| *s > 0)
        .map(|seconds| Played {
            seconds,
            last_played: match number("lastLaunchTime").filter(|ms| *ms > 0) {
                Some(ms) => (ms / 1000).saturating_add(number("lastTimePlayed").unwrap_or(0)),
                None => 0,
            },
        });
    Kept {
        memory_mb,
        java_args,
        playtime,
    }
}

/// CurseForge: `minecraftinstance.json`, with the loader as `forge-47.2.0`,
/// `neoforge-20.4.80-beta` or `fabric-0.15.11-1.20.1`.
async fn curseforge(dir: &Path) -> Option<Detected> {
    let file = dir.join("minecraftinstance.json");
    if !is_file(&file).await {
        return None;
    }
    let json = read_json(&file).await.unwrap_or(Value::Null);
    let base = json.get("baseModLoader").unwrap_or(&Value::Null);
    let minecraft =
        version(str_at(&json, "gameVersion").or_else(|| str_at(base, "minecraftVersion")));
    let loader = str_at(base, "name").and_then(|full| {
        let (kind, rest) = full.split_once('-')?;
        let kind = loader_kind(kind)?;
        let rest = match minecraft.as_deref() {
            Some(mc) => rest.strip_suffix(&format!("-{mc}")).unwrap_or(rest),
            // Fabric and Quilt name the game version last; without it, guess it's the tail.
            None if matches!(kind, LoaderKind::Fabric | LoaderKind::Quilt) => {
                rest.split_once('-').map_or(rest, |(v, _)| v)
            }
            None => rest,
        };
        loader(kind, Some(rest), minecraft.as_deref())
    });
    // Memory is in MB, used only while `isMemoryOverride` is on. The app records launches
    // and the last one, not time played.
    let memory_mb = (json.get("isMemoryOverride").and_then(Value::as_bool) == Some(true))
        .then(|| json.get("allocatedMemory").and_then(Value::as_u64))
        .flatten()
        .and_then(memory);
    Some(Detected {
        name: text(str_at(&json, "name")),
        minecraft,
        loader,
        game_dir: dir.to_path_buf(),
        skip_root_meta: true,
        kept: Kept {
            memory_mb,
            java_args: str_at(&json, "javaArgsOverride").and_then(args),
            playtime: None,
        },
    })
}

/// Modrinth App (legacy profiles): `profile.json` with a `metadata` object.
async fn modrinth(dir: &Path) -> Option<Detected> {
    let file = dir.join("profile.json");
    if !is_file(&file).await {
        return None;
    }
    let json = read_json(&file).await?;
    let meta = json.get("metadata").filter(|m| m.is_object())?;
    let minecraft = version(str_at(meta, "game_version"));
    let loader = str_at(meta, "loader")
        .and_then(loader_kind)
        .and_then(|kind| {
            let id = meta.get("loader_version").and_then(|v| str_at(v, "id"));
            loader(kind, id, minecraft.as_deref())
        });
    // Absent `memory` and `java` follow the app's global settings. Memory is in MB; the time
    // played is in seconds, split into what the app reported to Modrinth and what it hasn't yet.
    let java_args = json
        .get("java")
        .and_then(|j| j.get("extra_arguments"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|list| !list.is_empty());
    let played = |key: &str| meta.get(key).and_then(Value::as_u64).unwrap_or(0);
    let seconds = played("submitted_time_played").saturating_add(played("recent_time_played"));
    let playtime = (seconds > 0).then(|| Played {
        seconds,
        last_played: str_at(meta, "last_played")
            .and_then(unix_seconds)
            .unwrap_or(0),
    });
    Some(Detected {
        name: text(str_at(meta, "name")),
        minecraft,
        loader,
        game_dir: dir.to_path_buf(),
        skip_root_meta: true,
        kept: Kept {
            memory_mb: json
                .get("memory")
                .and_then(|m| m.get("maximum"))
                .and_then(Value::as_u64)
                .and_then(memory),
            java_args,
            playtime,
        },
    })
}

/// ATLauncher: `instance.json` with a `launcher` object.
async fn atlauncher(dir: &Path) -> Option<Detected> {
    let file = dir.join("instance.json");
    if !is_file(&file).await {
        return None;
    }
    let json = read_json(&file).await?;
    let launcher = json.get("launcher").filter(|l| l.is_object())?;
    let minecraft = version(str_at(&json, "id"));
    let loader = launcher.get("loaderVersion").and_then(|lv| {
        let kind = str_at(lv, "type").and_then(loader_kind)?;
        loader(kind, str_at(lv, "version"), minecraft.as_deref())
    });
    // A missing `maximumMemory` (MB) or `javaArguments` follows ATLauncher's global settings.
    // It counts launches, not time played.
    Some(Detected {
        name: text(str_at(launcher, "name")),
        minecraft,
        loader,
        game_dir: dir.to_path_buf(),
        skip_root_meta: true,
        kept: Kept {
            memory_mb: launcher
                .get("maximumMemory")
                .and_then(Value::as_u64)
                .and_then(memory),
            java_args: str_at(launcher, "javaArguments").and_then(args),
            playtime: None,
        },
    })
}

/// Any other game folder (the official launcher's `.minecraft`, newer launchers): nothing
/// to read, so the player picks the version and loader.
async fn plain(dir: &Path) -> Option<Detected> {
    let mut found = is_file(&dir.join("options.txt")).await;
    for marker in GAME_MARKERS {
        found = found || is_dir(&dir.join(marker)).await;
    }
    found.then(|| Detected {
        name: None,
        minecraft: None,
        loader: None,
        game_dir: dir.to_path_buf(),
        skip_root_meta: true,
        kept: Kept::default(),
    })
}

fn loader_kind(name: &str) -> Option<LoaderKind> {
    match name.trim().to_ascii_lowercase().as_str() {
        "fabric" => Some(LoaderKind::Fabric),
        "quilt" => Some(LoaderKind::Quilt),
        "forge" => Some(LoaderKind::Forge),
        "neoforge" => Some(LoaderKind::NeoForge),
        _ => None,
    }
}

/// A loader with a usable version. Some launchers write Forge as `1.20.1-47.2.0`; the
/// launcher keeps only `47.2.0`.
fn loader(
    kind: LoaderKind,
    version_text: Option<&str>,
    minecraft: Option<&str>,
) -> Option<LoaderSpec> {
    let raw = version_text?.trim();
    let raw = minecraft
        .and_then(|mc| raw.strip_prefix(&format!("{mc}-")))
        .unwrap_or(raw);
    Some(LoaderSpec {
        kind,
        version: version(Some(raw))?,
    })
}

/// A version string, kept only when it's a plain version (no spaces, slashes or the like).
fn version(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|v| valid_component(v))
        .map(str::to_string)
}

/// Free text such as a name: trimmed, `None` when empty.
fn text(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// A memory amount in MiB, when there is one.
fn memory(mb: u64) -> Option<u32> {
    u32::try_from(mb).ok().filter(|mb| *mb > 0)
}

/// Java arguments typed as one string, split. `None` when there are none or the string can't
/// be read.
fn args(raw: &str) -> Option<Vec<String>> {
    jvm_args::split(raw).ok().filter(|args| !args.is_empty())
}

/// An RFC 3339 time (`2024-01-02T03:04:05.123Z`, `…+02:00`) as Unix seconds.
fn unix_seconds(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    let num = |range: std::ops::Range<usize>| raw.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let rest = raw.get(19..)?;
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.get(..1)? {
                "+" => 1,
                "-" => -1,
                _ => return None,
            };
            let h = rest.get(1..3)?.parse::<i64>().ok()?;
            let m = rest.get(4..6)?.parse::<i64>().ok()?;
            sign * (h * 3600 + m * 60)
        }
    };
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hour * 3600 + minute * 60 + second - offset).ok()
}

fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// The value of `key=value` in an INI file, ignoring sections and comments. Values written
/// with surrounding quotes (Qt does that for some characters) lose them, and Qt's escapes
/// (`\"`, `\\`, `\n`, `\t`) are read back.
fn ini_value(ini: &str, key: &str) -> Option<String> {
    ini.lines().find_map(|line| {
        let (k, v) = line.trim().split_once('=')?;
        if k.trim() != key {
            return None;
        }
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(v);
        Some(ini_unescape(v))
    })
}

fn ini_unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// A `true`/`false` INI value; anything else is false.
fn ini_bool(ini: &str, key: &str) -> bool {
    ini_value(ini, key).is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

async fn read_text(path: &Path) -> Option<String> {
    let bytes = tokio::fs::read(path).await.ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The CurseForge app can start its JSON with a byte order mark, which JSON doesn't allow.
async fn read_json(path: &Path) -> Option<Value> {
    let bytes = tokio::fs::read(path).await.ok()?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    serde_json::from_slice(bytes).ok()
}

/// Links don't count: the copy never follows them.
async fn is_file(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path)
        .await
        .is_ok_and(|m| m.is_file())
}

async fn is_dir(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path)
        .await
        .is_ok_and(|m| m.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_value_ignores_sections_comments_and_quotes() {
        let ini = "[General]\n# name=nope\nInstanceType=OneSix\nname = \"My, Pack\"\n";
        assert_eq!(ini_value(ini, "name").as_deref(), Some("My, Pack"));
        assert_eq!(ini_value(ini, "missing"), None);
    }

    #[test]
    fn versions_must_be_plain() {
        assert_eq!(version(Some(" 1.20.1 ")).as_deref(), Some("1.20.1"));
        assert_eq!(version(Some("../evil")), None);
        assert_eq!(version(Some("")), None);
        assert_eq!(
            loader(LoaderKind::Forge, Some("1.20.1-47.2.0"), Some("1.20.1")),
            Some(LoaderSpec {
                kind: LoaderKind::Forge,
                version: "47.2.0".into()
            })
        );
    }

    #[test]
    fn ini_value_reads_qt_escapes() {
        let ini = r#"JvmArgs="-Da=\"b c\" -Dpath=C:\\x""#;
        assert_eq!(
            ini_value(ini, "JvmArgs").as_deref(),
            Some(r#"-Da="b c" -Dpath=C:\x"#)
        );
        assert!(ini_bool("OverrideMemory=true", "OverrideMemory"));
        assert!(!ini_bool("OverrideMemory=false", "OverrideMemory"));
        assert!(!ini_bool("", "OverrideMemory"));
    }

    #[test]
    fn rfc3339_times_become_unix_seconds() {
        assert_eq!(unix_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            unix_seconds("2024-01-02T03:04:05.123456Z"),
            Some(1_704_164_645)
        );
        assert_eq!(
            unix_seconds("2024-01-02T05:04:05+02:00"),
            Some(1_704_164_645)
        );
        assert_eq!(
            unix_seconds("2024-02-29T00:00:00-01:30"),
            Some(1_709_170_200)
        );
        assert_eq!(unix_seconds("yesterday"), None);
        assert_eq!(unix_seconds("2024-01-02T03:04:05"), None);
    }
}

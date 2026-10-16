//! What another launcher's instance folder is: its name, Minecraft version and loader, and
//! where its game files live. Detection only prefills the form, so a missing, malformed or
//! unsupported field is `None` rather than a failure.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{
    instances::LoaderSpec,
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
    let name = read_text(&dir.join("instance.cfg"))
        .await
        .and_then(|cfg| ini_value(&cfg, "name"))
        .and_then(|n| text(Some(&n)));
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
    })
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
    Some(Detected {
        name: text(str_at(&json, "name")),
        minecraft,
        loader,
        game_dir: dir.to_path_buf(),
        skip_root_meta: true,
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
    Some(Detected {
        name: text(str_at(meta, "name")),
        minecraft,
        loader,
        game_dir: dir.to_path_buf(),
        skip_root_meta: true,
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
    Some(Detected {
        name: text(str_at(launcher, "name")),
        minecraft,
        loader,
        game_dir: dir.to_path_buf(),
        skip_root_meta: true,
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

fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// The value of `key=value` in an INI file, ignoring sections and comments. Values written
/// with surrounding quotes (Qt does that for some characters) lose them.
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
        Some(v.to_string())
    })
}

async fn read_text(path: &Path) -> Option<String> {
    let bytes = tokio::fs::read(path).await.ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

async fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&tokio::fs::read(path).await.ok()?).ok()
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
}

//! Whether an instance can run shader packs. Shader packs do nothing on their own: they need a
//! shader mod, Iris (Fabric, Quilt, NeoForge) or Oculus (Forge, and NeoForge 1.20.1, which runs
//! Forge mods), or OptiFine. A shader pack added by hand to an instance without one never shows
//! up in game, so the Shaders tab and Browse say so and offer the mod.
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{error::Result, instances::GameSpec, meta::loaders::LoaderKind, mods};

/// The shader mod to offer, by its Modrinth slug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderMod {
    Iris,
    Oculus,
}

impl ShaderMod {
    pub fn slug(self) -> &'static str {
        match self {
            ShaderMod::Iris => "iris",
            ShaderMod::Oculus => "oculus",
        }
    }
}

/// What the Shaders tab needs to know about an instance's shader mod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderSupport {
    /// The mod that runs shader packs on this instance's loader; `None` on vanilla.
    pub wanted: Option<ShaderMod>,
    /// An enabled jar in `mods/` is a shader mod (Iris, Oculus or OptiFine), the player's or the
    /// pack's own.
    pub installed: bool,
}

pub fn wanted(game: &GameSpec) -> Option<ShaderMod> {
    match game.loader.as_ref()?.kind {
        LoaderKind::Fabric | LoaderKind::Quilt => Some(ShaderMod::Iris),
        LoaderKind::Forge => Some(ShaderMod::Oculus),
        // NeoForge 1.20.1 runs Forge mods and has no Iris build; the planner falls back the same way.
        LoaderKind::NeoForge if game.minecraft == "1.20.1" => Some(ShaderMod::Oculus),
        LoaderKind::NeoForge => Some(ShaderMod::Iris),
    }
}

/// The mod ids that run shader packs. OptiFine has none in its jar, but a few repackagings do.
const SHADER_MOD_IDS: [&str; 3] = ["iris", "oculus", "optifine"];

/// Whether `jar`'s file name says it is a shader mod: one of its words is a shader mod's name.
fn named_like_one(jar: &Path) -> bool {
    let name = jar
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| SHADER_MOD_IDS.contains(&word))
}

/// Whether one jar is a shader mod. Blocking.
fn runs_shaders(jar: &Path) -> bool {
    let ids = crate::sync::mod_ids::mod_ids(jar);
    if !ids.is_empty() {
        return SHADER_MOD_IDS.iter().any(|id| ids.contains(*id));
    }
    // No loader metadata. A jar that isn't a zip can't be loaded by the game at all.
    let Some(archive) = std::fs::File::open(jar)
        .ok()
        .and_then(|f| zip::ZipArchive::new(f).ok())
    else {
        return false;
    };
    // OptiFine declares no mod id; its own package is how it is known.
    if archive
        .file_names()
        .flatten()
        .any(|n| n.starts_with("optifine/") || n.starts_with("net/optifine/"))
    {
        return true;
    }
    // Only when the jar has no metadata at all is the file name the clue.
    named_like_one(jar)
}

/// Whether an enabled jar in `game_dir/mods` is a shader mod. A `.jar.disabled` never counts.
pub async fn installed(game_dir: &Path) -> Result<bool> {
    let mods_dir = game_dir.join("mods");
    let mut jars: Vec<PathBuf> = mods::list(game_dir)
        .await?
        .into_iter()
        .filter(|m| m.enabled)
        .map(|m| mods_dir.join(m.file_name))
        .collect();
    // Jars named like a shader mod first: the answer is usually among them, so the rest stay unread.
    jars.sort_by_key(|jar| !named_like_one(jar));
    // A scan that couldn't finish found nothing: the note only offers the mod, it never blocks.
    Ok(
        tokio::task::spawn_blocking(move || jars.iter().any(|jar| runs_shaders(jar)))
            .await
            .unwrap_or(false),
    )
}

/// [`ShaderSupport`] for an instance with `game` in `game_dir`. Vanilla reads no jars.
pub async fn support(game: &GameSpec, game_dir: &Path) -> Result<ShaderSupport> {
    let Some(wanted) = wanted(game) else {
        return Ok(ShaderSupport {
            wanted: None,
            installed: false,
        });
    };
    Ok(ShaderSupport {
        wanted: Some(wanted),
        installed: installed(game_dir).await?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{instances::LoaderSpec, test_support::write_zip};

    fn game(minecraft: &str, loader: Option<LoaderKind>) -> GameSpec {
        GameSpec {
            minecraft: minecraft.into(),
            loader: loader.map(|kind| LoaderSpec {
                kind,
                version: "1".into(),
            }),
        }
    }

    fn jar(name: &str, entries: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        write_zip(&path, entries);
        (dir, path)
    }

    const IRIS_FABRIC: (&str, &str) = ("fabric.mod.json", r#"{"id": "iris", "version": "1.8"}"#);
    const OCULUS_TOML: &str = "modLoader = \"javafml\"\n[[mods]]\nmodId = \"oculus\"\n";

    #[test]
    fn each_loader_wants_its_shader_mod() {
        assert_eq!(
            wanted(&game("1.21.1", Some(LoaderKind::Fabric))),
            Some(ShaderMod::Iris)
        );
        assert_eq!(
            wanted(&game("1.21.1", Some(LoaderKind::Quilt))),
            Some(ShaderMod::Iris)
        );
        assert_eq!(
            wanted(&game("1.20.1", Some(LoaderKind::Forge))),
            Some(ShaderMod::Oculus)
        );
        assert_eq!(
            wanted(&game("1.21.1", Some(LoaderKind::NeoForge))),
            Some(ShaderMod::Iris)
        );
        // NeoForge 1.20.1 runs Forge mods, and Iris has no build for it.
        assert_eq!(
            wanted(&game("1.20.1", Some(LoaderKind::NeoForge))),
            Some(ShaderMod::Oculus)
        );
        assert_eq!(wanted(&game("1.21.1", None)), None);
        assert_eq!(ShaderMod::Oculus.slug(), "oculus");
    }

    #[test]
    fn a_shader_mod_is_known_by_its_metadata() {
        assert!(runs_shaders(&jar("a.jar", &[IRIS_FABRIC]).1));
        assert!(runs_shaders(
            &jar(
                "b.jar",
                &[("quilt.mod.json", r#"{"quilt_loader": {"id": "iris"}}"#)]
            )
            .1
        ));
        assert!(runs_shaders(
            &jar("c.jar", &[("META-INF/mods.toml", OCULUS_TOML)]).1
        ));
        assert!(runs_shaders(
            &jar(
                "d.jar",
                &[(
                    "META-INF/neoforge.mods.toml",
                    "[[mods]]\nmodId = \"iris\"\n"
                )]
            )
            .1
        ));
        assert!(!runs_shaders(
            &jar("e.jar", &[("fabric.mod.json", r#"{"id": "sodium"}"#)]).1
        ));
    }

    #[test]
    fn optifine_is_known_by_its_own_package() {
        // OptiFine ships no loader metadata, only its own classes.
        let (_dir, path) = jar(
            "preview_OptiFine_1.20.1_HD_U_I6.jar",
            &[
                ("optifine/Installer.class", "x"),
                ("net/optifine/Config.class", "x"),
            ],
        );
        assert!(runs_shaders(&path));
    }

    #[test]
    fn the_file_name_counts_only_without_metadata() {
        // A jar that says what it is is believed, whatever its name.
        assert!(!runs_shaders(
            &jar(
                "iris-1.8.jar",
                &[("fabric.mod.json", r#"{"id": "sodium"}"#)]
            )
            .1
        ));
        assert!(runs_shaders(&jar("sodium.jar", &[IRIS_FABRIC]).1));
        // No metadata at all: the name is the only clue.
        assert!(runs_shaders(
            &jar("iris-fabric-1.8.0.jar", &[("readme.txt", "hi")]).1
        ));
        assert!(runs_shaders(
            &jar("Oculus_1.20.1.jar", &[("readme.txt", "hi")]).1
        ));
        assert!(!runs_shaders(
            &jar("irisblocks.jar", &[("readme.txt", "hi")]).1
        ));
        // Not a jar at all: the game can't load it either.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("iris.jar"), b"not a zip").unwrap();
        assert!(!runs_shaders(&dir.path().join("iris.jar")));
    }

    #[tokio::test]
    async fn only_an_enabled_jar_counts() {
        let dir = tempfile::tempdir().unwrap();
        let game_dir = dir.path();
        assert!(!installed(game_dir).await.unwrap(), "no mods folder");
        let mods = game_dir.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        write_zip(
            &mods.join("sodium.jar"),
            &[("fabric.mod.json", r#"{"id": "sodium"}"#)],
        );
        write_zip(&mods.join("iris.jar.disabled"), &[IRIS_FABRIC]);
        assert!(!installed(game_dir).await.unwrap());
        write_zip(&mods.join("shaders-here.jar"), &[IRIS_FABRIC]);
        assert!(installed(game_dir).await.unwrap());
    }

    #[tokio::test]
    async fn support_names_the_mod_and_whether_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        write_zip(
            &mods.join("oculus.jar"),
            &[("META-INF/mods.toml", OCULUS_TOML)],
        );
        assert_eq!(
            support(&game("1.20.1", Some(LoaderKind::Forge)), dir.path())
                .await
                .unwrap(),
            ShaderSupport {
                wanted: Some(ShaderMod::Oculus),
                installed: true,
            }
        );
        assert_eq!(
            support(&game("1.20.1", None), dir.path()).await.unwrap(),
            ShaderSupport {
                wanted: None,
                installed: false,
            }
        );
    }
}

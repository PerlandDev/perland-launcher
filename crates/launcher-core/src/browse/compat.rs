//! Which Modrinth content fits which instance (spec §3.1): kinds, loader tags, picking a
//! version, choosing its file, and the instance a "New instance" install gets.
use serde::{Deserialize, Serialize};

use super::api::{ApiFile, ApiVersion};
use crate::{
    download::Hashes,
    fsutil::safe_component,
    instances::GameSpec,
    meta::{catalog::compare_versions, loaders::LoaderKind},
    sources::check_pack_url,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Modpack,
    Mod,
    ResourcePack,
    Shader,
}

impl ContentKind {
    pub const ALL: [ContentKind; 4] = [
        ContentKind::Modpack,
        ContentKind::Mod,
        ContentKind::ResourcePack,
        ContentKind::Shader,
    ];

    /// Modrinth's name for the kind (`project_type`).
    pub fn project_type(self) -> &'static str {
        match self {
            ContentKind::Modpack => "modpack",
            ContentKind::Mod => "mod",
            ContentKind::ResourcePack => "resourcepack",
            ContentKind::Shader => "shader",
        }
    }

    pub fn from_project_type(s: &str) -> Option<ContentKind> {
        Self::ALL.into_iter().find(|k| k.project_type() == s)
    }

    /// The game-folder subfolder its files go in; modpacks become instances instead.
    pub fn folder(self) -> Option<&'static str> {
        match self {
            ContentKind::Modpack => None,
            ContentKind::Mod => Some("mods"),
            ContentKind::ResourcePack => Some("resourcepacks"),
            ContentKind::Shader => Some("shaderpacks"),
        }
    }

    /// The file extension the launcher installs for the kind (lowercase, with the dot).
    pub fn extension(self) -> &'static str {
        match self {
            ContentKind::Modpack => ".mrpack",
            ContentKind::Mod => ".jar",
            ContentKind::ResourcePack | ContentKind::Shader => ".zip",
        }
    }
}

/// What content has to fit: an instance's Minecraft version and loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub minecraft: String,
    pub loader: Option<LoaderKind>,
}

impl Target {
    pub fn of(game: &GameSpec) -> Target {
        Target {
            minecraft: game.minecraft.clone(),
            loader: game.loader.as_ref().map(|l| l.kind),
        }
    }
}

/// The shader-pack tag every shader search and plan uses: Iris and Oculus both read these packs.
pub const SHADER_TAG: &str = "iris";

/// Tags Modrinth lists among a project's categories that are loaders, not categories.
pub const NON_CATEGORY_TAGS: [&str; 10] = [
    "fabric",
    "quilt",
    "forge",
    "neoforge",
    "iris",
    "optifine",
    "canvas",
    "vanilla",
    "minecraft",
    "datapack",
];

pub fn loader_tag(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Fabric => "fabric",
        LoaderKind::Quilt => "quilt",
        LoaderKind::Forge => "forge",
        LoaderKind::NeoForge => "neoforge",
    }
}

/// The Modrinth loader tags an instance with `loader` can run: Quilt runs Fabric mods, and
/// NeoForge on 1.20.1 runs Forge mods.
pub fn loader_tags(loader: LoaderKind, minecraft: Option<&str>) -> Vec<&'static str> {
    match loader {
        LoaderKind::Quilt => vec!["quilt", "fabric"],
        LoaderKind::NeoForge if minecraft == Some("1.20.1") => vec!["neoforge", "forge"],
        other => vec![loader_tag(other)],
    }
}

/// The mod (by Modrinth slug) that runs shader packs on `loader`.
pub fn shader_mod(loader: Option<LoaderKind>) -> Option<&'static str> {
    match loader? {
        LoaderKind::Fabric | LoaderKind::Quilt | LoaderKind::NeoForge => Some("iris"),
        LoaderKind::Forge => Some("oculus"),
    }
}

/// 0 for releases, 1 for betas, 2 for anything else.
pub fn stability(version_type: &str) -> u8 {
    match version_type {
        "release" => 0,
        "beta" => 1,
        _ => 2,
    }
}

/// The file to install: the primary one when it has the kind's extension, else the first that
/// does (never a `-sources` jar). It must be https (loopback http in tests), carry a sha1 and
/// be one safe file name.
pub fn usable_file(v: &ApiVersion, kind: ContentKind) -> Option<&ApiFile> {
    let ext = kind.extension();
    let suits = |f: &ApiFile| {
        let lower = f.filename.to_ascii_lowercase();
        lower.ends_with(ext) && !lower.ends_with(&format!("-sources{ext}"))
    };
    let file = v
        .files
        .iter()
        .find(|f| f.primary && suits(f))
        .or_else(|| v.files.iter().find(|f| suits(f)))?;
    let sha1 = file
        .hashes
        .sha1
        .as_deref()
        .and_then(|s| Hashes::from_format("sha1", s));
    (safe_component(&file.filename) && check_pack_url(&file.url).is_ok() && sha1.is_some())
        .then_some(file)
}

/// The hashes the object cache verifies a file against: its sha1 and, when given, its sha512.
pub fn file_hashes(file: &ApiFile) -> Hashes {
    Hashes {
        sha1: file
            .hashes
            .sha1
            .as_deref()
            .and_then(|s| Hashes::from_format("sha1", s))
            .and_then(|h| h.sha1),
        sha512: file
            .hashes
            .sha512
            .as_deref()
            .and_then(|s| Hashes::from_format("sha512", s))
            .and_then(|h| h.sha512),
        ..Hashes::default()
    }
}

/// Does version `v` of a `kind` project run on `target` (spec §3.1)?
pub fn fits(v: &ApiVersion, kind: ContentKind, target: &Target) -> bool {
    if !v.game_versions.contains(&target.minecraft) || usable_file(v, kind).is_none() {
        return false;
    }
    let has = |tag: &str| v.loaders.iter().any(|l| l == tag);
    match kind {
        ContentKind::Mod => target.loader.is_some_and(|loader| {
            loader_tags(loader, Some(&target.minecraft))
                .into_iter()
                .any(has)
        }),
        ContentKind::Shader => target.loader.is_some() && has(SHADER_TAG),
        ContentKind::ResourcePack => true,
        ContentKind::Modpack => false,
    }
}

/// Releases first, then betas, then alphas; the newest within each (spec §3.4).
pub fn pick(versions: &[ApiVersion], keep: impl Fn(&ApiVersion) -> bool) -> Option<&ApiVersion> {
    versions.iter().filter(|v| keep(v)).min_by(|a, b| {
        stability(&a.version_type)
            .cmp(&stability(&b.version_type))
            .then_with(|| b.date_published.cmp(&a.date_published))
    })
}

/// A release version number like `1.20` or `1.20.1` (no snapshots or pre-releases).
pub fn is_release(mc: &str) -> bool {
    let parts: Vec<&str> = mc.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

pub fn newest_release(game_versions: &[String]) -> Option<&str> {
    game_versions
        .iter()
        .filter(|g| is_release(g))
        .max_by(|a, b| compare_versions(a, b))
        .map(String::as_str)
}

/// The instance a "New instance" install creates for `v`: its newest Minecraft release and the
/// first loader the launcher supports, in the order Fabric, Quilt, NeoForge, Forge. Shaders get
/// Fabric (for Iris), resource packs no loader.
pub fn new_target(v: &ApiVersion, kind: ContentKind) -> Option<Target> {
    usable_file(v, kind)?;
    let minecraft = newest_release(&v.game_versions)?.to_string();
    let has = |tag: &str| v.loaders.iter().any(|l| l == tag);
    let loader = match kind {
        ContentKind::Mod => Some(
            [
                LoaderKind::Fabric,
                LoaderKind::Quilt,
                LoaderKind::NeoForge,
                LoaderKind::Forge,
            ]
            .into_iter()
            .find(|k| has(loader_tag(*k)))?,
        ),
        ContentKind::Shader => {
            if !has(SHADER_TAG) {
                return None;
            }
            Some(LoaderKind::Fabric)
        }
        ContentKind::ResourcePack => None,
        ContentKind::Modpack => return None,
    };
    Some(Target { minecraft, loader })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::api::{ApiFile, ApiHashes, ApiVersion};

    const SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

    fn file(name: &str, primary: bool) -> ApiFile {
        ApiFile {
            url: format!("https://cdn.modrinth.com/data/x/versions/1/{name}"),
            filename: name.into(),
            primary,
            size: 10,
            hashes: ApiHashes {
                sha1: Some(SHA1.into()),
                sha512: None,
            },
        }
    }

    fn v(
        id: &str,
        kind: &str,
        date: &str,
        mc: &[&str],
        loaders: &[&str],
        files: Vec<ApiFile>,
    ) -> ApiVersion {
        ApiVersion {
            id: id.into(),
            project_id: "p".into(),
            name: id.into(),
            version_number: id.into(),
            version_type: kind.into(),
            game_versions: mc.iter().map(|s| s.to_string()).collect(),
            loaders: loaders.iter().map(|s| s.to_string()).collect(),
            date_published: date.into(),
            files,
            dependencies: vec![],
        }
    }

    fn jar(id: &str, mc: &[&str], loaders: &[&str]) -> ApiVersion {
        v(
            id,
            "release",
            "2026-01-01T00:00:00Z",
            mc,
            loaders,
            vec![file("m.jar", true)],
        )
    }

    fn target(mc: &str, loader: Option<LoaderKind>) -> Target {
        Target {
            minecraft: mc.into(),
            loader,
        }
    }

    #[test]
    fn loader_tags_follow_the_spec() {
        assert_eq!(
            loader_tags(LoaderKind::Quilt, Some("1.20.1")),
            vec!["quilt", "fabric"]
        );
        assert_eq!(
            loader_tags(LoaderKind::NeoForge, Some("1.20.1")),
            vec!["neoforge", "forge"]
        );
        assert_eq!(
            loader_tags(LoaderKind::NeoForge, Some("1.21.1")),
            vec!["neoforge"]
        );
        assert_eq!(loader_tags(LoaderKind::Forge, None), vec!["forge"]);
        assert_eq!(loader_tags(LoaderKind::Fabric, None), vec!["fabric"]);
        assert_eq!(shader_mod(Some(LoaderKind::Forge)), Some("oculus"));
        assert_eq!(shader_mod(Some(LoaderKind::NeoForge)), Some("iris"));
        assert_eq!(shader_mod(None), None);
    }

    #[test]
    fn kinds_map_to_project_types_and_folders() {
        for kind in ContentKind::ALL {
            assert_eq!(
                ContentKind::from_project_type(kind.project_type()),
                Some(kind)
            );
        }
        assert_eq!(ContentKind::from_project_type("plugin"), None);
        assert_eq!(ContentKind::ResourcePack.folder(), Some("resourcepacks"));
        assert_eq!(ContentKind::Shader.folder(), Some("shaderpacks"));
        assert_eq!(ContentKind::Modpack.folder(), None);
        assert_eq!(
            serde_json::to_value(ContentKind::ResourcePack).unwrap(),
            "resource_pack"
        );
    }

    #[test]
    fn fits_checks_game_loader_and_kind() {
        let fabric = jar("a", &["1.20.1"], &["fabric"]);
        assert!(fits(
            &fabric,
            ContentKind::Mod,
            &target("1.20.1", Some(LoaderKind::Fabric))
        ));
        assert!(fits(
            &fabric,
            ContentKind::Mod,
            &target("1.20.1", Some(LoaderKind::Quilt))
        ));
        assert!(!fits(&fabric, ContentKind::Mod, &target("1.20.1", None)));
        assert!(!fits(
            &fabric,
            ContentKind::Mod,
            &target("1.21.1", Some(LoaderKind::Fabric))
        ));
        assert!(!fits(
            &fabric,
            ContentKind::Mod,
            &target("1.20.1", Some(LoaderKind::Forge))
        ));
        let forge = jar("f", &["1.20.1"], &["forge"]);
        assert!(fits(
            &forge,
            ContentKind::Mod,
            &target("1.20.1", Some(LoaderKind::NeoForge))
        ));
        let shader = v(
            "s",
            "release",
            "2026-01-01T00:00:00Z",
            &["1.20.1"],
            &["iris", "optifine"],
            vec![file("s.zip", true)],
        );
        assert!(fits(
            &shader,
            ContentKind::Shader,
            &target("1.20.1", Some(LoaderKind::Forge))
        ));
        assert!(!fits(&shader, ContentKind::Shader, &target("1.20.1", None)));
        let optifine_only = v(
            "o",
            "release",
            "2026-01-01T00:00:00Z",
            &["1.20.1"],
            &["optifine"],
            vec![file("o.zip", true)],
        );
        assert!(!fits(
            &optifine_only,
            ContentKind::Shader,
            &target("1.20.1", Some(LoaderKind::Fabric))
        ));
        let pack = v(
            "r",
            "release",
            "2026-01-01T00:00:00Z",
            &["1.20.1"],
            &["minecraft"],
            vec![file("r.zip", true)],
        );
        assert!(fits(
            &pack,
            ContentKind::ResourcePack,
            &target("1.20.1", None)
        ));
        assert!(!fits(&pack, ContentKind::Modpack, &target("1.20.1", None)));
    }

    #[test]
    fn pick_prefers_releases_then_the_newest() {
        let list = vec![
            v(
                "beta-new",
                "beta",
                "2026-03-01T00:00:00Z",
                &["1.20.1"],
                &["fabric"],
                vec![file("b.jar", true)],
            ),
            v(
                "rel-old",
                "release",
                "2026-01-01T00:00:00Z",
                &["1.20.1"],
                &["fabric"],
                vec![file("a.jar", true)],
            ),
            v(
                "rel-mid",
                "release",
                "2026-02-01T00:00:00Z",
                &["1.20.1"],
                &["fabric"],
                vec![file("c.jar", true)],
            ),
            v(
                "alpha",
                "alpha",
                "2026-04-01T00:00:00Z",
                &["1.20.1"],
                &["fabric"],
                vec![file("d.jar", true)],
            ),
        ];
        assert_eq!(pick(&list, |_| true).unwrap().id, "rel-mid");
        assert_eq!(
            pick(&list, |v| v.version_type != "release").unwrap().id,
            "beta-new"
        );
        assert_eq!(pick(&list[3..], |_| true).unwrap().id, "alpha");
        assert!(pick(&list, |_| false).is_none());
    }

    #[test]
    fn usable_file_prefers_the_right_extension_and_refuses_unsafe_files() {
        // A sources jar marked primary loses to the real jar.
        let sources_primary = v(
            "x",
            "release",
            "d",
            &["1.20.1"],
            &["fabric"],
            vec![file("m-sources.jar", true), file("m.jar", false)],
        );
        assert_eq!(
            usable_file(&sources_primary, ContentKind::Mod)
                .unwrap()
                .filename,
            "m.jar"
        );
        // A resource pack whose primary is a jar uses its zip.
        let rp = v(
            "r",
            "release",
            "d",
            &["1.20.1"],
            &[],
            vec![file("r.jar", true), file("r.zip", false)],
        );
        assert_eq!(
            usable_file(&rp, ContentKind::ResourcePack)
                .unwrap()
                .filename,
            "r.zip"
        );
        // Plain http, no sha1, unsafe names and the wrong extension are not usable.
        let mut http = file("h.jar", true);
        http.url = "http://example.com/h.jar".into();
        let mut no_sha1 = file("n.jar", true);
        no_sha1.hashes.sha1 = None;
        for bad in [
            http,
            no_sha1,
            file("CON.jar", true),
            file("x.jar.", true),
            file("a.txt", true),
        ] {
            let name = bad.filename.clone();
            let version = v("b", "release", "d", &["1.20.1"], &["fabric"], vec![bad]);
            assert!(usable_file(&version, ContentKind::Mod).is_none(), "{name}");
            assert!(
                !fits(
                    &version,
                    ContentKind::Mod,
                    &target("1.20.1", Some(LoaderKind::Fabric))
                ),
                "{name}"
            );
        }
        let mrpack = v(
            "m",
            "release",
            "d",
            &["1.20.1"],
            &["fabric"],
            vec![file("Pack 1.0.mrpack", true)],
        );
        assert_eq!(
            usable_file(&mrpack, ContentKind::Modpack).unwrap().filename,
            "Pack 1.0.mrpack"
        );
    }

    #[test]
    fn new_instances_take_the_newest_release_and_a_supported_loader() {
        let mc = ["1.20.1", "1.21.1", "24w14a", "1.21.2-pre1"];
        assert_eq!(newest_release(&mc.map(String::from)), Some("1.21.1"));
        assert!(
            is_release("1.20")
                && is_release("1.20.1")
                && !is_release("1.21-pre1")
                && !is_release("24w14a")
        );
        let forge_and_fabric = jar("a", &mc, &["forge", "fabric"]);
        assert_eq!(
            new_target(&forge_and_fabric, ContentKind::Mod),
            Some(target("1.21.1", Some(LoaderKind::Fabric)))
        );
        let neo = jar("n", &["1.21.1"], &["neoforge", "forge"]);
        assert_eq!(
            new_target(&neo, ContentKind::Mod).unwrap().loader,
            Some(LoaderKind::NeoForge)
        );
        let datapack = jar("d", &["1.21.1"], &["datapack"]);
        assert_eq!(new_target(&datapack, ContentKind::Mod), None);
        let shader = v(
            "s",
            "release",
            "d",
            &["1.21.1"],
            &["iris"],
            vec![file("s.zip", true)],
        );
        assert_eq!(
            new_target(&shader, ContentKind::Shader).unwrap().loader,
            Some(LoaderKind::Fabric)
        );
        let rp = v(
            "r",
            "release",
            "d",
            &["1.21.1"],
            &["minecraft"],
            vec![file("r.zip", true)],
        );
        assert_eq!(
            new_target(&rp, ContentKind::ResourcePack),
            Some(target("1.21.1", None))
        );
        let snapshots_only = jar("z", &["24w14a"], &["fabric"]);
        assert_eq!(new_target(&snapshots_only, ContentKind::Mod), None);
    }
}

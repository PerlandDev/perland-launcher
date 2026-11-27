//! Files dropped on the window: what they are (for the drop overlay), and adding them to an
//! instance. Mods go to `mods/` as the player's own, packs to `resourcepacks/` or
//! `shaderpacks/`. A `.mrpack` is imported by the UI when it is dropped on its own.
use std::path::PathBuf;

use launcher_core::{
    browse::compat::ContentKind,
    content,
    dropped::{self, DroppedKind},
    mods, ErrorCode, LauncherError, Result,
};

use crate::{
    app::AppCore,
    dto::{DropAdded, DropResult, DropSkipReason, DropSkipped, DroppedFile},
    events::Host,
    runner::Runner,
};

fn name_of(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// What each path is, in order. Reads only names and zip directories.
async fn classify_all(paths: Vec<PathBuf>) -> Result<Vec<(PathBuf, DroppedKind)>> {
    tokio::task::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|p| {
                let kind = dropped::classify(&p);
                (p, kind)
            })
            .collect()
    })
    .await
    .map_err(|e| std::io::Error::other(e).into())
}

/// A refusal that means "skip this one file", not "stop the drop".
fn skip_reason(error: &LauncherError) -> Option<DropSkipReason> {
    match error.code {
        ErrorCode::ModExists => Some(DropSkipReason::Exists),
        ErrorCode::ModManaged => Some(DropSkipReason::Managed),
        _ => None,
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    pub async fn drop_inspect(&self, paths: Vec<PathBuf>) -> Result<Vec<DroppedFile>> {
        Ok(classify_all(paths)
            .await?
            .into_iter()
            .map(|(path, kind)| DroppedFile {
                name: name_of(&path),
                path: path.to_string_lossy().into_owned(),
                kind,
            })
            .collect())
    }

    /// Adds what fits and skips the rest with its reason. A failure that isn't about one file
    /// (the instance is gone, the disk is full) stops the drop; files added before it stay.
    pub async fn drop_add(&self, id: &str, paths: Vec<PathBuf>) -> Result<DropResult> {
        let instance = self.instances.load(id).await?;
        let modded = instance.game.loader.is_some();
        let dir = self.instances.game_dir(id);
        let mut result = DropResult::default();
        for (path, kind) in classify_all(paths).await? {
            let name = name_of(&path);
            let outcome = match kind {
                DroppedKind::Mod | DroppedKind::ShaderPack if !modded => {
                    Err(DropSkipReason::NeedsLoader)
                }
                DroppedKind::Mod => self.add_mod(id, &dir, &path, &name).await?,
                DroppedKind::ResourcePack => {
                    self.add_pack(id, &dir, ContentKind::ResourcePack, &path, &name)
                        .await?
                }
                DroppedKind::ShaderPack => {
                    self.add_pack(id, &dir, ContentKind::Shader, &path, &name)
                        .await?
                }
                DroppedKind::Modpack => Err(DropSkipReason::Modpack),
                DroppedKind::UnknownZip => Err(DropSkipReason::NotAPack),
                DroppedKind::Other => Err(DropSkipReason::NotContent),
            };
            match outcome {
                Ok(()) => result.added.push(DropAdded { name, kind }),
                Err(reason) => result.skipped.push(DropSkipped { name, kind, reason }),
            }
        }
        Ok(result)
    }

    /// `Ok(Err(reason))` for a file skipped; `Err` stops the drop.
    async fn add_mod(
        &self,
        id: &str,
        dir: &std::path::Path,
        path: &std::path::Path,
        name: &str,
    ) -> Result<std::result::Result<(), DropSkipReason>> {
        let added = match self.refuse_managed(id, name).await {
            Ok(()) => mods::add(dir, path).await.map(|_| ()),
            Err(error) => Err(error),
        };
        settle(added)
    }

    async fn add_pack(
        &self,
        id: &str,
        dir: &std::path::Path,
        kind: ContentKind,
        path: &std::path::Path,
        name: &str,
    ) -> Result<std::result::Result<(), DropSkipReason>> {
        let folder = kind.folder().unwrap_or_default();
        let added = match self.refuse_managed_in(id, folder, name).await {
            Ok(()) => content::add(dir, kind, path).await.map(|_| ()),
            Err(error) => Err(error),
        };
        settle(added)
    }
}

fn settle(added: Result<()>) -> Result<std::result::Result<(), DropSkipReason>> {
    match added {
        Ok(()) => Ok(Ok(())),
        Err(error) => match skip_reason(&error) {
            Some(reason) => Ok(Err(reason)),
            None => Err(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use launcher_core::{accounts::BuildKind, instances::LoaderSpec, meta::loaders::LoaderKind};

    use super::*;
    use crate::{
        app::tests::{core, TestCore},
        dto::NewInstance,
    };

    fn zip(path: &Path, entries: &[&str]) -> PathBuf {
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for name in entries {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
        }
        w.finish().unwrap();
        path.to_path_buf()
    }

    fn file(path: &Path) -> PathBuf {
        std::fs::write(path, b"jar").unwrap();
        path.to_path_buf()
    }

    async fn instance(core: &TestCore, name: &str, loader: bool) -> String {
        let request = NewInstance {
            name: name.into(),
            minecraft: "1.20.1".into(),
            loader: loader.then(|| LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.14".into(),
            }),
        };
        core.instance_create(request).await.unwrap().id
    }

    fn added(name: &str, kind: DroppedKind) -> DropAdded {
        DropAdded {
            name: name.into(),
            kind,
        }
    }

    fn skipped(name: &str, kind: DroppedKind, reason: DropSkipReason) -> DropSkipped {
        DropSkipped {
            name: name.into(),
            kind,
            reason,
        }
    }

    #[tokio::test]
    async fn inspect_names_each_file_and_its_kind_in_order() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let d = tmp.path();
        let paths = vec![
            file(&d.join("sodium.jar")),
            zip(&d.join("BSL.zip"), &["shaders/a.fsh"]),
            zip(&d.join("Faithful.zip"), &["pack.mcmeta"]),
            file(&d.join("Sky.mrpack")),
            zip(&d.join("photos.zip"), &["a.png"]),
            file(&d.join("notes.txt")),
        ];
        let seen = core.drop_inspect(paths).await.unwrap();
        let view: Vec<(&str, DroppedKind)> =
            seen.iter().map(|f| (f.name.as_str(), f.kind)).collect();
        assert_eq!(
            view,
            vec![
                ("sodium.jar", DroppedKind::Mod),
                ("BSL.zip", DroppedKind::ShaderPack),
                ("Faithful.zip", DroppedKind::ResourcePack),
                ("Sky.mrpack", DroppedKind::Modpack),
                ("photos.zip", DroppedKind::UnknownZip),
                ("notes.txt", DroppedKind::Other),
            ]
        );
        assert_eq!(seen[0].path, d.join("sodium.jar").to_string_lossy());
    }

    #[tokio::test]
    async fn add_routes_each_file_to_its_folder_and_says_what_it_skipped() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let id = instance(&core, "Fabric", true).await;
        let d = tmp.path();
        let paths = vec![
            file(&d.join("sodium.jar")),
            zip(&d.join("BSL.zip"), &["BSL/shaders/a.fsh"]),
            zip(&d.join("Faithful.zip"), &["pack.mcmeta"]),
            file(&d.join("Sky.mrpack")),
            zip(&d.join("photos.zip"), &["a.png"]),
            file(&d.join("notes.txt")),
        ];
        let result = core.drop_add(&id, paths).await.unwrap();
        assert_eq!(
            result.added,
            vec![
                added("sodium.jar", DroppedKind::Mod),
                added("BSL.zip", DroppedKind::ShaderPack),
                added("Faithful.zip", DroppedKind::ResourcePack),
            ]
        );
        assert_eq!(
            result.skipped,
            vec![
                skipped("Sky.mrpack", DroppedKind::Modpack, DropSkipReason::Modpack),
                skipped(
                    "photos.zip",
                    DroppedKind::UnknownZip,
                    DropSkipReason::NotAPack
                ),
                skipped("notes.txt", DroppedKind::Other, DropSkipReason::NotContent),
            ]
        );
        let game = core.instances.game_dir(&id);
        assert!(game.join("mods/sodium.jar").is_file());
        assert!(game.join("shaderpacks/BSL.zip").is_file());
        assert!(game.join("resourcepacks/Faithful.zip").is_file());
        assert_eq!(core.mods_list(&id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_vanilla_instance_takes_resource_packs_but_not_mods_or_shaders() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let id = instance(&core, "Vanilla", false).await;
        let d = tmp.path();
        let paths = vec![
            file(&d.join("sodium.jar")),
            zip(&d.join("BSL.zip"), &["shaders/a.fsh"]),
            zip(&d.join("Faithful.zip"), &["pack.mcmeta"]),
        ];
        let result = core.drop_add(&id, paths).await.unwrap();
        assert_eq!(
            result.added,
            vec![added("Faithful.zip", DroppedKind::ResourcePack)]
        );
        assert_eq!(
            result.skipped,
            vec![
                skipped("sodium.jar", DroppedKind::Mod, DropSkipReason::NeedsLoader),
                skipped(
                    "BSL.zip",
                    DroppedKind::ShaderPack,
                    DropSkipReason::NeedsLoader
                ),
            ]
        );
        let game = core.instances.game_dir(&id);
        assert!(!game.join("mods").exists());
        assert!(!game.join("shaderpacks/BSL.zip").exists());
    }

    #[tokio::test]
    async fn files_already_there_or_owned_by_the_pack_are_skipped_untouched() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let id = instance(&core, "Pack", true).await;
        let game = core.instances.game_dir(&id);
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/mine.jar.disabled"), b"old").unwrap();
        crate::testing::managed_for(&["mods/Create.jar", "resourcepacks/Faithful.zip"])
            .save(&core.instances.perland_dir(&id))
            .await
            .unwrap();
        let d = tmp.path();
        let paths = vec![
            file(&d.join("mine.jar")),
            // The pack's own files, even while missing from the folder, are never replaced.
            file(&d.join("create.jar")),
            zip(&d.join("Faithful.zip"), &["pack.mcmeta"]),
            file(&d.join("extra.jar")),
        ];
        let result = core.drop_add(&id, paths).await.unwrap();
        assert_eq!(result.added, vec![added("extra.jar", DroppedKind::Mod)]);
        assert_eq!(
            result.skipped,
            vec![
                skipped("mine.jar", DroppedKind::Mod, DropSkipReason::Exists),
                skipped("create.jar", DroppedKind::Mod, DropSkipReason::Managed),
                skipped(
                    "Faithful.zip",
                    DroppedKind::ResourcePack,
                    DropSkipReason::Managed
                ),
            ]
        );
        assert_eq!(
            std::fs::read(game.join("mods/mine.jar.disabled")).unwrap(),
            b"old"
        );
        assert!(!game.join("mods/create.jar").exists());
        assert!(!game.join("resourcepacks/Faithful.zip").exists());
    }

    #[tokio::test]
    async fn an_instance_that_isnt_there_is_refused() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let jar = file(&tmp.path().join("a.jar"));
        let err = core.drop_add("ghost", vec![jar]).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InstanceNotFound);
    }
}

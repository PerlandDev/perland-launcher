//! Share an instance as a `.mrpack` (spec `2026-10-21-share-instance-mrpack-design.md`): the
//! dialog's numbers, writing the file, and Show in folder.
use std::path::PathBuf;

use launcher_core::{
    instances::Instance,
    locks::InstanceLock,
    share::{self, ShareChoice, SharePlan},
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::{busy, AppCore},
    dto::SharePreview,
    events::Host,
    runner::Runner,
};

/// `out` as a `.mrpack` path. Save dialogs that return the typed name as is can hand back
/// "Fabric 1.20.1" (extension "1"), so any other ending gets `.mrpack` added; only a name that
/// is plainly another kind of archive is refused.
fn mrpack_path(out: PathBuf) -> Result<PathBuf> {
    let ext = out
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        Some("mrpack") => Ok(out),
        Some("zip" | "jar" | "7z" | "rar") => Err(LauncherError::disk(
            ErrorCode::SharePathInvalid,
            "Shared instances are saved as a .mrpack file",
        )
        .with_fix("Pick a file name that ends in .mrpack")),
        _ if out.file_name().is_some() => {
            let mut name = out.file_name().unwrap_or_default().to_os_string();
            name.push(".mrpack");
            Ok(out.with_file_name(name))
        }
        _ => Err(LauncherError::disk(
            ErrorCode::SharePathInvalid,
            "Shared instances are saved as a .mrpack file",
        )
        .with_fix("Pick a file name that ends in .mrpack")),
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    pub(crate) async fn share_plan(&self, instance: &Instance) -> Result<SharePlan> {
        let scan = share::scan(&self.instances.game_dir(&instance.id)).await?;
        if scan.skipped_secrets > 0 || scan.skipped_unreadable > 0 {
            tracing::info!(
                secrets = scan.skipped_secrets,
                unreadable = scan.skipped_unreadable,
                "share: left out files that look like they hold a secret or couldn't be read"
            );
        }
        let sha1s = scan.files.iter().filter_map(|f| f.sha1.clone()).collect();
        let linked = self.runner.browser().share_lookup(sha1s).await?;
        Ok(SharePlan { scan, linked })
    }

    /// What the Share dialog shows. The plan is kept for the export that usually follows.
    pub async fn share_preview(&self, id: &str) -> Result<SharePreview> {
        let instance = self.instances.load(id).await?;
        let plan = self.share_plan(&instance).await?;
        let preview = SharePreview {
            name: instance.name.clone(),
            file_name: share::file_name_for(&instance.name),
            counts: share::counts(&plan),
        };
        // Only the latest dialog's plan is kept: one left by a cancelled dialog is dropped here.
        let mut plans = self.share_plans.lock().unwrap_or_else(|e| e.into_inner());
        plans.clear();
        plans.insert(instance.id, plan);
        Ok(preview)
    }

    /// Write the instance to `out` (a `.mrpack` the player picked in the save dialog). Refused
    /// while the game runs or content installs: a world being saved would be copied half-written.
    pub async fn share_export(
        &self,
        id: &str,
        choice: ShareChoice,
        out: PathBuf,
    ) -> Result<PathBuf> {
        let out = mrpack_path(out)?;
        let instance = self.instances.load(id).await?;
        if self.is_busy(&instance.id) || self.is_installing(&instance.id) {
            return Err(busy(&instance.id));
        }
        let _lock =
            InstanceLock::try_acquire(&self.instances.perland_dir(&instance.id)).map_err(|e| {
                if e.code == ErrorCode::InstanceLocked {
                    busy(&instance.id)
                } else {
                    e
                }
            })?;
        let kept = self
            .share_plans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&instance.id);
        let plan = match kept {
            Some(plan) => plan,
            None => self.share_plan(&instance).await?,
        };
        let meta = share::PackMeta {
            name: instance.name.clone(),
            version_id: share::today_utc(),
            minecraft: instance.game.minecraft.clone(),
            loader: instance.game.loader.clone(),
        };
        share::write(
            &self.instances.game_dir(&instance.id),
            &plan,
            choice,
            meta,
            &out,
        )
        .await?;
        *self.last_share.lock().unwrap_or_else(|e| e.into_inner()) = Some(out.clone());
        Ok(out)
    }

    /// The file the last share wrote, for Show in folder (never a path from the webview).
    pub fn share_last(&self) -> Option<PathBuf> {
        self.last_share
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        download::Hashes,
        instances::LoaderSpec,
        meta::loaders::LoaderKind,
        share::{Category, Linked},
        sources::{mrpack, FileSource},
        ErrorCode,
    };

    use crate::{
        app::{tests::core, Run},
        dto::NewInstance,
        events::RunStateKind,
        testing::{FakeRunner, RecordingHost},
    };

    use super::*;

    type Core = std::sync::Arc<AppCore<FakeRunner, RecordingHost>>;

    const CDN: &str = "https://cdn.modrinth.com/data/AAAA/versions/v1/known.jar";

    async fn fabric(core: &Core) -> Instance {
        let instance = core
            .instance_create(NewInstance {
                name: "Sky Friends".into(),
                minecraft: "1.20.1".into(),
                loader: Some(LoaderSpec {
                    kind: LoaderKind::Fabric,
                    version: "0.16.14".into(),
                }),
            })
            .await
            .unwrap();
        let game = core.instances.game_dir(&instance.id);
        for (rel, body) in [
            ("mods/known.jar", &b"known"[..]),
            ("mods/own.jar", b"own"),
            ("config/a.toml", b"a = 1"),
        ] {
            let path = game.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        instance
    }

    fn knows_known(runner: &FakeRunner) {
        let sha1 = launcher_core::download::hash::sha1_hex(b"known");
        runner.browser.share_linked.lock().unwrap().insert(
            sha1.clone(),
            Linked {
                sha1,
                sha512: launcher_core::download::hash::sha512_hex(b"known"),
                size: 5,
                url: CDN.into(),
                env: None,
            },
        );
    }

    fn all() -> ShareChoice {
        ShareChoice {
            mods: true,
            settings: true,
            resource_packs: true,
            shader_packs: true,
            worlds: false,
        }
    }

    #[tokio::test]
    async fn preview_counts_what_modrinth_knows() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        knows_known(&runner);
        let instance = fabric(&core).await;
        let preview = core.share_preview(&instance.id).await.unwrap();
        assert_eq!(preview.name, "Sky Friends");
        assert_eq!(preview.file_name, "Sky Friends.mrpack");
        let mods = &preview.counts[0];
        assert_eq!(
            (mods.category, mods.files, mods.linked, mods.embedded_bytes),
            (Category::Mods, 2, 1, 3)
        );
        assert_eq!(runner.browser.share_lookups.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn export_writes_a_pack_the_reader_reads() {
        let (core, runner, _, tmp) = core(BuildKind::Dev);
        knows_known(&runner);
        let instance = fabric(&core).await;
        let restore = core.instances.perland_dir(&instance.id).join("restore/1");
        std::fs::create_dir_all(&restore).unwrap();
        std::fs::write(restore.join("mods.jar"), b"old").unwrap();
        core.share_preview(&instance.id).await.unwrap();
        let out = tmp.path().join("Sky Friends.mrpack");
        assert_eq!(
            core.share_export(&instance.id, all(), out.clone())
                .await
                .unwrap(),
            out
        );
        // The preview's plan was used: no second lookup.
        assert_eq!(runner.browser.share_lookups.lock().unwrap().len(), 1);
        let desired = mrpack::read_archive(
            &out,
            "s".into(),
            Hashes::sha1("0".repeat(40)),
            mrpack::Limits::default(),
        )
        .unwrap();
        assert_eq!(desired.name, "Sky Friends");
        assert_eq!(desired.minecraft, "1.20.1");
        let known = desired
            .files
            .iter()
            .find(|f| f.path == "mods/known.jar")
            .unwrap();
        assert_eq!(
            known.source,
            FileSource::Remote {
                urls: vec![CDN.into()]
            }
        );
        assert_eq!(core.share_last(), Some(out.clone()));
        // Restore points and instance.json live beside the game folder: never in the pack.
        let zip = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
        assert!(
            names.iter().all(|n| !n.contains(".perland")
                && !n.contains("restore")
                && !n.ends_with("instance.json")),
            "{names:?}"
        );
    }

    #[tokio::test]
    async fn export_without_a_preview_looks_up_again() {
        let (core, runner, _, tmp) = core(BuildKind::Dev);
        let instance = fabric(&core).await;
        core.share_export(&instance.id, all(), tmp.path().join("p"))
            .await
            .unwrap();
        assert!(tmp.path().join("p.mrpack").is_file());
        assert_eq!(runner.browser.share_lookups.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn export_must_be_a_mrpack() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let instance = fabric(&core).await;
        let err = core
            .share_export(&instance.id, all(), tmp.path().join("notes.zip"))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::SharePathInvalid);
        assert_eq!(core.share_last(), None);
    }

    #[test]
    fn typed_names_get_the_extension() {
        // Only the file name changes; compared alone, since Windows joins with a backslash.
        let path = |p: &str| {
            mrpack_path(PathBuf::from(p))
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        };
        assert_eq!(path("/d/Sky.mrpack").unwrap(), "Sky.mrpack");
        assert_eq!(path("/d/Sky.MRPACK").unwrap(), "Sky.MRPACK");
        assert_eq!(path("/d/Sky").unwrap(), "Sky.mrpack");
        assert_eq!(path("/d/Fabric 1.20.1").unwrap(), "Fabric 1.20.1.mrpack");
        assert_eq!(
            path("/d/Sky.zip").unwrap_err().code,
            ErrorCode::SharePathInvalid
        );
    }

    #[tokio::test]
    async fn only_the_latest_plan_is_kept() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        let one = fabric(&core).await;
        let two = core
            .instance_create(NewInstance {
                name: "Other".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        core.share_preview(&one.id).await.unwrap();
        core.share_preview(&two.id).await.unwrap();
        let plans = core.share_plans.lock().unwrap();
        assert_eq!(plans.keys().collect::<Vec<_>>(), vec![&two.id]);
    }

    #[tokio::test]
    async fn export_waits_for_the_game() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let instance = fabric(&core).await;
        core.runs.lock().unwrap().insert(
            instance.id.clone(),
            Run {
                phase: RunStateKind::Running,
                abort: None,
                killer: None,
                cancel: Default::default(),
                answer: None,
                offer: None,
                update_only: false,
                synced: None,
            },
        );
        let out = tmp.path().join("p.mrpack");
        let err = core
            .share_export(&instance.id, all(), out.clone())
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InstanceBusy);
        assert!(!out.exists());
    }

    #[tokio::test]
    async fn a_failed_lookup_is_the_preview_error() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        runner
            .browser
            .share_lookup_fails
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let instance = fabric(&core).await;
        let err = core.share_preview(&instance.id).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Offline);
    }
}

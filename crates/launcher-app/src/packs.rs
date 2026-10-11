//! Pack commands other than playing: adding a pack, its status, its mods, manual downloads
//! and art (spec addendum §4.4). Runs and syncs live in `runs.rs`.
use std::sync::Arc;

use launcher_core::{
    instances::{Instance, SourceRef},
    sources::PackInput,
    sync::lockfile,
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::AppCore,
    dto::{PackAddRequest, PackStatus, PackUpdate},
    events::Host,
    runner::Runner,
};

fn no_pack() -> LauncherError {
    LauncherError::pack(ErrorCode::NoPack, "This instance doesn't come from a pack")
}

fn link_invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackLinkInvalid,
        "That doesn't look like a pack link",
    )
    .with_fix("Pack links start with https:// and usually end in pack.toml")
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// A new instance from a pack link (https `pack.toml` or `modrinth:`) or a `.mrpack` file.
    /// Nothing but the art downloads until its first Play.
    pub async fn pack_add(&self, request: PackAddRequest) -> Result<Instance> {
        let input = match request {
            PackAddRequest::Link { url } => match PackInput::parse(&url)? {
                PackInput::MrpackFile { .. } => return Err(link_invalid()),
                input => input,
            },
            PackAddRequest::File { path } => {
                let is_mrpack = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("mrpack"));
                let is_file = tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_file());
                if !is_mrpack || !is_file {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    return Err(LauncherError::pack(
                        ErrorCode::PackInvalid,
                        format!("{name} isn't a .mrpack file"),
                    )
                    .with_param("file", name));
                }
                PackInput::MrpackFile { path }
            }
        };
        self.runner.add_pack(input).await
    }

    /// The instance `id`, if it comes from a pack.
    pub(crate) async fn pack_instance(&self, id: &str) -> Result<Instance> {
        let instance = self.instances.load(id).await?;
        if instance.source == SourceRef::None {
            return Err(no_pack());
        }
        Ok(instance)
    }

    /// The banner's Update: take the pack's update without playing.
    pub async fn pack_update(self: &Arc<Self>, id: &str) -> Result<()> {
        let instance = self.pack_instance(id).await?;
        let checks = self.checks_for_updates(&instance).await;
        self.start_run(id, instance, None, checks)
    }

    /// For the banner: the installed version and the update on offer. A failed check is
    /// reported in `check_error`, not as an error: the banner just says it couldn't check.
    pub async fn pack_status(&self, id: &str) -> Result<PackStatus> {
        let instance = self.pack_instance(id).await?;
        let installed = lockfile::load_pack(&self.instances.perland_dir(id)).await;
        let (update, check_error) = match &installed {
            None => (None, None),
            Some(_) => match self.runner.find_update(instance).await {
                Ok(update) => (update.as_ref().map(PackUpdate::from), None),
                Err(error) => (None, Some(error)),
            },
        };
        Ok(PackStatus {
            installed_version: installed.map(|d| d.pack_version),
            update,
            check_error,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use launcher_core::{
        accounts::BuildKind, instances::SourceRef, sources::PackInput, ErrorCode, LauncherError,
    };

    use crate::{
        app::tests::core,
        dto::{NewInstance, PackAddRequest},
        events::RunStateKind::*,
        testing::{desired, exit, pack_instance, pending},
    };

    #[tokio::test]
    async fn the_banner_update_syncs_without_playing() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", Some(&desired("1.0.3", vec![])))
            .await
            .id;
        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.3", "1.0.4"))));
        core.pack_update(&id).await.unwrap();
        host.wait_for(&id, Idle).await;
        assert_eq!(host.states(&id), vec![Checking, Queued, Syncing, Idle]);
        assert_eq!(
            *runner.syncs.lock().unwrap(),
            vec![(id.clone(), Some("1.0.4".to_string()))]
        );
        let exit = host.last_exit(&id).unwrap();
        assert!(exit.update_only && !exit.is_crash() && exit.error.is_none());
        assert!(runner.prepared.lock().unwrap().is_empty(), "no game");
    }

    #[tokio::test]
    async fn pack_update_refuses_vanilla_and_busy_instances() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        core.account_add_offline("Steve").await.unwrap();
        let plain = core
            .instance_create(NewInstance {
                name: "Plain".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        assert_eq!(
            core.pack_update(&plain.id).await.unwrap_err().code,
            ErrorCode::NoPack
        );
        let id = pack_instance(&core, "Sky", Some(&desired("1.0.3", vec![])))
            .await
            .id;
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            core.pack_update(&id).await.unwrap_err().code,
            ErrorCode::AlreadyRunning
        );
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn pack_status_reports_the_update_or_why_not() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", Some(&desired("1.0.3", vec![])))
            .await
            .id;
        let status = core.pack_status(&id).await.unwrap();
        assert_eq!(status.installed_version.as_deref(), Some("1.0.3"));
        assert!(status.update.is_none() && status.check_error.is_none());

        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.3", "1.0.4"))));
        assert_eq!(
            core.pack_status(&id)
                .await
                .unwrap()
                .update
                .unwrap()
                .to_version,
            "1.0.4"
        );

        runner.updates.lock().unwrap().insert(
            id.clone(),
            Err(LauncherError::network(
                ErrorCode::UpdateCheckFailed,
                "Couldn't check for updates",
            )),
        );
        assert_eq!(
            core.pack_status(&id)
                .await
                .unwrap()
                .check_error
                .unwrap()
                .code,
            ErrorCode::UpdateCheckFailed
        );

        let never = pack_instance(&core, "Fresh", None).await.id;
        let checks = runner.checks.load(Ordering::SeqCst);
        let status = core.pack_status(&never).await.unwrap();
        assert!(status.installed_version.is_none() && status.update.is_none());
        assert_eq!(
            runner.checks.load(Ordering::SeqCst),
            checks,
            "nothing installed to compare against"
        );
    }

    #[tokio::test]
    async fn pack_add_takes_links_and_mrpack_files_only() {
        let (core, runner, _host, tmp) = core(BuildKind::Dev);
        let link = |url: &str| PackAddRequest::Link { url: url.into() };
        let url = "https://perlanddev.github.io/asalith-modpack/pack.toml";
        let added = core.pack_add(link(url)).await.unwrap();
        assert_eq!(added.source, SourceRef::Packwiz { url: url.into() });
        core.pack_add(link("modrinth:sky")).await.unwrap();

        let mrpack = tmp.path().join("Sky Islands.MRPACK");
        std::fs::write(&mrpack, b"zip").unwrap();
        core.pack_add(PackAddRequest::File {
            path: mrpack.clone(),
        })
        .await
        .unwrap();
        assert_eq!(
            *runner.added.lock().unwrap(),
            vec![
                PackInput::Packwiz { url: url.into() },
                PackInput::Modrinth {
                    project: "sky".into(),
                    version: None
                },
                PackInput::MrpackFile { path: mrpack },
            ]
        );

        // A link is never a file, and never plain http.
        for bad in [
            "C:/Downloads/pack.mrpack",
            "http://evil.example/pack.toml",
            "pack.toml",
        ] {
            assert_eq!(
                core.pack_add(link(bad)).await.unwrap_err().code,
                ErrorCode::PackLinkInvalid,
                "{bad}"
            );
        }
        // A file is only ever a regular .mrpack file.
        let exe = tmp.path().join("setup.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let folder = tmp.path().join("folder.mrpack");
        std::fs::create_dir_all(&folder).unwrap();
        for bad in [exe, folder, tmp.path().join("missing.mrpack")] {
            assert_eq!(
                core.pack_add(PackAddRequest::File { path: bad.clone() })
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::PackInvalid,
                "{}",
                bad.display()
            );
        }
        assert_eq!(
            runner.added.lock().unwrap().len(),
            3,
            "refused input never reaches the core"
        );
    }
}

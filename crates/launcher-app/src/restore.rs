//! Going back to how an instance was before its last update (spec §5.7 and §5.8): the commands
//! the UI calls. Points are taken around changes in `runs.rs` (sync, seal, end of a run),
//! `browse.rs` (installs) and `app.rs` (removals); the work itself is `launcher_core::restore`.

use std::{path::Path, sync::Arc};

use launcher_core::{
    instances::{unix_now, Instance, UpdateHold},
    locks::InstanceLock,
    restore, ErrorCode, Result,
};

use crate::{
    app::{busy, AppCore},
    dto::RestorePointDto,
    events::Host,
    runner::Runner,
};

/// Unix seconds, as the restore point keeps time.
pub(crate) fn now_secs() -> u64 {
    u64::try_from(unix_now()).unwrap_or(0)
}

/// The instance's lock for a change made outside a run. Someone else holding it (a run, the
/// CLI) is the same refusal as a running game.
pub(crate) fn lock_or_busy(perland: &Path, id: &str) -> Result<InstanceLock> {
    InstanceLock::try_acquire(perland).map_err(|e| {
        if e.code == ErrorCode::InstanceLocked {
            busy(id)
        } else {
            e
        }
    })
}

/// The name the confirm dialog warns about, for a server instance: the server's, as Home
/// shows it (its own name when the view gave none).
fn server_name(instance: &Instance) -> Option<String> {
    instance.server.as_ref().map(|link| {
        if link.name.trim().is_empty() {
            instance.name.clone()
        } else {
            link.name.clone()
        }
    })
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The instance's restore point, or `None` when there is none, it is older than 30 days or
    /// it is damaged.
    pub async fn restore_point(&self, id: &str) -> Result<Option<RestorePointDto>> {
        let instance = self.instances.load(id).await?;
        let Some(meta) = restore::read(&self.instances, &instance.id, now_secs()).await else {
            return Ok(None);
        };
        Ok(Some(RestorePointDto {
            kind: meta.kind,
            created_at: meta.created_at,
            changes: meta.changes,
            size_bytes: restore::held_size(&self.instances, &instance.id).await,
            server: server_name(&instance),
        }))
    }

    /// Put the instance back to its restore point. Refused while it runs, installs or another
    /// process holds it. An instance that checks for updates is held at the restored version
    /// when going back undoes a pack update, and a hold it has already stays (spec §5.8). Returns the new point (how it was before going back), or `None` when that
    /// one couldn't be kept.
    pub async fn restore_go_back(&self, id: &str) -> Result<Option<RestorePointDto>> {
        let instance = self.instances.load(id).await?;
        if self.is_busy(id) {
            return Err(busy(id));
        }
        if self.is_installing(id) {
            return Err(crate::browse::content_installing());
        }
        let lock = lock_or_busy(&self.instances.perland_dir(id), id)?;
        let now = now_secs();
        // An existing hold stays (while held, the instance doesn't check for updates). A new
        // one only when going back undoes a pack update: Browse installs alone leave the
        // instance on the server's pack.
        let hold = match instance.update_hold.clone() {
            Some(hold) => Some(hold),
            None if self.checks_for_updates(&instance).await => self.new_hold(id, now).await,
            None => None,
        };
        let went = restore::go_back(&self.instances, id, hold, now).await?;
        drop(lock);
        tracing::info!(
            instance = id,
            kept_current = went.kept_current,
            kept_undo = went.kept_undo,
            "went back"
        );
        if !went.kept_undo {
            return Ok(None);
        }
        self.restore_point(id).await
    }

    /// The hold for going back to the point, when it undoes a pack update: at the point's pack
    /// version, or the installed pack's when the point has none.
    async fn new_hold(&self, id: &str, now: u64) -> Option<UpdateHold> {
        let point = restore::read(&self.instances, id, now).await?;
        if !point
            .changes
            .iter()
            .any(|c| matches!(c, restore::Change::PackUpdate { .. }))
        {
            return None;
        }
        let version = match point.pack_version {
            Some(version) => version,
            None => {
                launcher_core::sync::lockfile::load_pack(&self.instances.perland_dir(id))
                    .await?
                    .pack_version
            }
        };
        Some(UpdateHold {
            since: now,
            version,
        })
    }

    /// "Update again": take the pack's update like the banner's Update, lifting the hold.
    pub async fn pack_update_again(self: &Arc<Self>, id: &str) -> Result<()> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        // The run lifts the hold once it holds the lock; refused here, the hold stays.
        let mut instance = self.pack_instance(id).await?;
        instance.update_hold = None;
        let checks = self.checks_for_updates(&instance).await;
        self.start_run_lifting(id, instance, None, checks, true)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::Ordering, Arc};

    use launcher_core::{
        accounts::BuildKind,
        instances::{unix_now, UpdateHold, UpdatePolicy},
        locks::InstanceLock,
        restore::{self, Change, ChangeCtx, PointKind},
        ErrorCode,
    };

    use crate::{
        app::{
            busy,
            tests::{core, TestCore},
        },
        dto::NewInstance,
        events::RunStateKind::*,
        testing::{desired, exit, managed_for, pack_instance, pending, FakeRunner, RecordingHost},
    };

    fn now() -> u64 {
        u64::try_from(unix_now()).unwrap()
    }

    async fn ready() -> (
        Arc<TestCore>,
        Arc<FakeRunner>,
        Arc<RecordingHost>,
        tempfile::TempDir,
    ) {
        let (core, runner, host, tmp) = core(BuildKind::Dev);
        crate::testing::sign_in_alex(&core).await;
        (core, runner, host, tmp)
    }

    async fn plain(core: &TestCore, name: &str) -> String {
        core.instance_create(NewInstance {
            name: name.into(),
            minecraft: "1.20.1".into(),
            loader: None,
        })
        .await
        .unwrap()
        .id
    }

    /// A pack instance "Sky" that synced 1.0.3, with `policy`.
    async fn pack(core: &TestCore, policy: UpdatePolicy) -> String {
        let mut instance = pack_instance(core, "Sky", Some(&desired("1.0.3", vec![]))).await;
        instance.update_policy = policy;
        core.instances.save(&instance).await.unwrap();
        instance.id
    }

    /// As if a sync from 1.0.3 to 1.0.4 took a point of `mods/a.jar` and then added `b.jar`.
    async fn updated(core: &TestCore, id: &str) {
        let game = core.instances.game_dir(id);
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/a.jar"), b"a").unwrap();
        managed_for(&["mods/a.jar"])
            .save(&core.instances.perland_dir(id))
            .await
            .unwrap();
        restore::before_change(
            &core.instances,
            id,
            &["mods".to_string()],
            Change::PackUpdate {
                from: "1.0.3".into(),
                to: "1.0.4".into(),
            },
            ChangeCtx {
                during_run: false,
                now: now(),
            },
        )
        .await;
        std::fs::write(game.join("mods/b.jar"), b"b").unwrap();
    }

    #[tokio::test]
    async fn restore_point_is_none_without_point() {
        let (core, _, _, _t) = ready().await;
        let id = plain(&core, "Alpha").await;
        assert_eq!(core.restore_point(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn restore_point_lists_the_changes() {
        let (core, _, _, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        updated(&core, &id).await;
        let point = core.restore_point(&id).await.unwrap().unwrap();
        assert_eq!(point.kind, PointKind::BeforeChange);
        assert_eq!(
            point.changes,
            vec![Change::PackUpdate {
                from: "1.0.3".into(),
                to: "1.0.4".into()
            }]
        );
        assert_eq!(point.server, None, "not a server instance");
    }

    #[tokio::test]
    async fn go_back_on_pack_instance_sets_hold_and_play_skips_check() {
        let (core, runner, host, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        updated(&core, &id).await;
        let undo = core.restore_go_back(&id).await.unwrap().unwrap();
        assert_eq!(undo.kind, PointKind::BeforeGoingBack);
        let game = core.instances.game_dir(&id);
        assert!(game.join("mods/a.jar").is_file());
        assert!(!game.join("mods/b.jar").exists());

        let instance = core.instances.load(&id).await.unwrap();
        let hold = instance.update_hold.clone().expect("held");
        assert_eq!(hold.version, "1.0.3");
        assert!(!core.checks_for_updates(&instance).await);

        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.3", "1.0.4"))));
        let checks = runner.checks.load(Ordering::SeqCst);
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(host.states(&id)[0], Queued, "no update check");
        assert_eq!(*runner.syncs.lock().unwrap(), vec![(id.clone(), None)]);
        assert_eq!(runner.checks.load(Ordering::SeqCst), checks);
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn a_stale_instance_update_keeps_the_hold() {
        let (core, _, _, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        let stale = core.instances.load(&id).await.unwrap();
        updated(&core, &id).await;
        core.restore_go_back(&id).await.unwrap();
        let mut edit = stale;
        edit.java.memory_mb += 512;
        let saved = core.instance_update(edit).await.unwrap();
        assert!(saved.update_hold.is_some());
    }

    /// A point of Browse installs only, on a pack instance.
    async fn browsed(core: &TestCore, id: &str) {
        let game = core.instances.game_dir(id);
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/a.jar"), b"a").unwrap();
        managed_for(&["mods/a.jar"])
            .save(&core.instances.perland_dir(id))
            .await
            .unwrap();
        restore::before_change(
            &core.instances,
            id,
            &["mods".to_string()],
            Change::Installed {
                name: "Sodium".into(),
            },
            ChangeCtx {
                during_run: false,
                now: now(),
            },
        )
        .await;
        std::fs::write(game.join("mods/sodium.jar"), b"s").unwrap();
    }

    #[tokio::test]
    async fn going_back_keeps_an_existing_hold() {
        let (core, _, _, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        let held = UpdateHold {
            since: 7,
            version: "1.0.3".into(),
        };
        let mut instance = core.instances.load(&id).await.unwrap();
        instance.update_hold = Some(held.clone());
        core.instances.save(&instance).await.unwrap();
        browsed(&core, &id).await;

        core.restore_go_back(&id).await.unwrap();
        assert!(!core
            .instances
            .game_dir(&id)
            .join("mods/sodium.jar")
            .exists());
        assert_eq!(
            core.instances.load(&id).await.unwrap().update_hold,
            Some(held)
        );
    }

    #[tokio::test]
    async fn going_back_over_browse_installs_only_sets_no_hold() {
        let (core, _, _, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        browsed(&core, &id).await;
        core.restore_go_back(&id).await.unwrap();
        assert!(!core
            .instances
            .game_dir(&id)
            .join("mods/sodium.jar")
            .exists());
        assert_eq!(core.instances.load(&id).await.unwrap().update_hold, None);
    }

    #[tokio::test]
    async fn a_point_without_a_pack_version_holds_at_the_installed_pack() {
        let (core, _, _, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        updated(&core, &id).await;
        // As if the point had been taken with no `managed.json`.
        let meta_path = core
            .instances
            .perland_dir(&id)
            .join("restore/point/point.json");
        let mut meta: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        meta["packVersion"] = serde_json::Value::Null;
        std::fs::write(&meta_path, serde_json::to_vec(&meta).unwrap()).unwrap();

        core.restore_go_back(&id).await.unwrap();
        let hold = core.instances.load(&id).await.unwrap().update_hold;
        assert_eq!(hold.expect("held").version, "1.0.3");
    }

    #[tokio::test]
    async fn go_back_on_plain_instance_sets_no_hold() {
        let (core, _, _, _t) = ready().await;
        let id = plain(&core, "Alpha").await;
        updated(&core, &id).await;
        core.restore_go_back(&id).await.unwrap().unwrap();
        assert!(!core.instances.game_dir(&id).join("mods/b.jar").exists());
        assert_eq!(core.instances.load(&id).await.unwrap().update_hold, None);
    }

    #[tokio::test]
    async fn go_back_on_pinned_instance_sets_no_hold() {
        let (core, _, _, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Pinned).await;
        updated(&core, &id).await;
        core.restore_go_back(&id).await.unwrap().unwrap();
        assert_eq!(core.instances.load(&id).await.unwrap().update_hold, None);
    }

    #[tokio::test]
    async fn go_back_refused_while_running() {
        let (core, runner, host, _t) = ready().await;
        let id = plain(&core, "Alpha").await;
        updated(&core, &id).await;
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(core.restore_go_back(&id).await.unwrap_err(), busy(&id));
        assert!(core.instances.game_dir(&id).join("mods/b.jar").is_file());
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;

        // Another process (the CLI) holding the instance is busy too.
        let _cli = InstanceLock::try_acquire(&core.instances.perland_dir(&id)).unwrap();
        assert_eq!(core.restore_go_back(&id).await.unwrap_err(), busy(&id));
    }

    #[tokio::test]
    async fn pack_update_again_clears_hold_and_runs_update() {
        let (core, runner, host, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        updated(&core, &id).await;
        core.restore_go_back(&id).await.unwrap();
        assert!(core
            .instances
            .load(&id)
            .await
            .unwrap()
            .update_hold
            .is_some());
        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.3", "1.0.4"))));
        core.pack_update_again(&id).await.unwrap();
        host.wait_for(&id, Idle).await;
        assert_eq!(core.instances.load(&id).await.unwrap().update_hold, None);
        assert_eq!(host.states(&id)[0], Checking);
        assert_eq!(
            *runner.syncs.lock().unwrap(),
            vec![(id.clone(), Some("1.0.4".to_string()))]
        );
        assert!(host.last_exit(&id).unwrap().update_only);
    }

    #[tokio::test]
    async fn pack_update_again_on_a_busy_instance_keeps_the_hold() {
        let (core, runner, host, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        updated(&core, &id).await;
        core.restore_go_back(&id).await.unwrap();
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(core.pack_update_again(&id).await.unwrap_err(), busy(&id));
        assert!(core
            .instances
            .load(&id)
            .await
            .unwrap()
            .update_hold
            .is_some());
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;

        // Refused by start_run itself (an install writes into it): still held.
        core.installing.lock().unwrap().insert(id.clone());
        assert_eq!(
            core.pack_update_again(&id).await.unwrap_err().code,
            ErrorCode::ContentInstalling
        );
        core.installing.lock().unwrap().clear();
        assert!(core
            .instances
            .load(&id)
            .await
            .unwrap()
            .update_hold
            .is_some());
    }

    #[tokio::test]
    async fn a_lost_game_does_not_mark_the_point_clean() {
        let (core, runner, host, _t) = ready().await;
        let id = plain(&core, "Alpha").await;
        updated(&core, &id).await;
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        runner.lose(&id);
        host.wait_for(&id, Idle).await;
        assert!(host.last_exit(&id).unwrap().error.is_some());
        // The next crash still offers going back.
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for_nth(&id, Running, 2).await;
        runner.finish(&id, exit(Some(1)));
        host.wait_for_nth(&id, Idle, 2).await;
        assert!(host.last_exit(&id).unwrap().restore);
    }

    #[tokio::test]
    async fn pack_status_clears_hold_when_source_matches() {
        let (core, runner, _, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        let hold = |version: &str| {
            let core = core.clone();
            let (id, version) = (id.clone(), version.to_string());
            async move {
                let mut instance = core.instances.load(&id).await.unwrap();
                instance.update_hold = Some(UpdateHold { since: 1, version });
                core.instances.save(&instance).await.unwrap();
            }
        };
        // The source still offers the newer version: the hold stays.
        hold("1.0.3").await;
        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.3", "1.0.4"))));
        assert!(!core.pack_status(&id).await.unwrap().hold_lifted);
        assert!(core
            .instances
            .load(&id)
            .await
            .unwrap()
            .update_hold
            .is_some());
        // The source went back to 1.0.3 too: nothing left to differ.
        runner.updates.lock().unwrap().remove(&id);
        assert!(
            core.pack_status(&id).await.unwrap().hold_lifted,
            "the UI learns it"
        );
        assert_eq!(core.instances.load(&id).await.unwrap().update_hold, None);
        assert!(
            !core.pack_status(&id).await.unwrap().hold_lifted,
            "nothing to lift"
        );
        // Installed differs from the held version: the hold stays.
        hold("1.0.2").await;
        core.pack_status(&id).await.unwrap();
        assert!(core
            .instances
            .load(&id)
            .await
            .unwrap()
            .update_hold
            .is_some());
    }

    #[tokio::test]
    async fn mod_remove_takes_point_and_lock() {
        let (core, _, _, tmp) = ready().await;
        let id = plain(&core, "Alpha").await;
        let jar = |name: &str| {
            let path = tmp.path().join(name);
            std::fs::write(&path, b"jar").unwrap();
            path
        };
        core.mods_add(&id, vec![jar("x.jar"), jar("y.jar")])
            .await
            .unwrap();
        core.mod_remove(&id, "x.jar").await.unwrap();
        let point = core.restore_point(&id).await.unwrap().unwrap();
        assert_eq!(
            point.changes,
            vec![Change::Removed {
                name: "x.jar".into()
            }]
        );

        let _cli = InstanceLock::try_acquire(&core.instances.perland_dir(&id)).unwrap();
        assert_eq!(core.mod_remove(&id, "y.jar").await.unwrap_err(), busy(&id));
        assert!(core.instances.game_dir(&id).join("mods/y.jar").is_file());
    }

    #[tokio::test]
    async fn content_remove_takes_point_and_lock() {
        use launcher_core::browse::compat::ContentKind;
        let (core, _, _, _t) = ready().await;
        let id = plain(&core, "Alpha").await;
        let packs = core.instances.game_dir(&id).join("resourcepacks");
        std::fs::create_dir_all(&packs).unwrap();
        std::fs::write(packs.join("A.zip"), b"zip").unwrap();
        std::fs::write(packs.join("B.zip"), b"zip").unwrap();
        core.content_remove(&id, ContentKind::ResourcePack, "A.zip")
            .await
            .unwrap();
        let point = core.restore_point(&id).await.unwrap().unwrap();
        assert_eq!(
            point.changes,
            vec![Change::Removed {
                name: "A.zip".into()
            }]
        );
        let _cli = InstanceLock::try_acquire(&core.instances.perland_dir(&id)).unwrap();
        assert_eq!(
            core.content_remove(&id, ContentKind::ResourcePack, "B.zip")
                .await
                .unwrap_err(),
            busy(&id)
        );
    }

    #[tokio::test]
    async fn crash_after_change_offers_restore() {
        let (core, runner, host, _t) = ready().await;
        let id = plain(&core, "Alpha").await;
        updated(&core, &id).await;
        let play = |n: usize, code: i32| {
            let (core, runner, host, id) = (core.clone(), runner.clone(), host.clone(), id.clone());
            async move {
                core.play(&id).await.unwrap();
                runner.open_gate(1);
                host.wait_for_nth(&id, Running, n).await;
                runner.finish(&id, exit(Some(code)));
                host.wait_for_nth(&id, Idle, n).await;
                host.last_exit(&id).unwrap()
            }
        };
        assert!(play(1, 1).await.restore, "a crash right after the update");
        assert!(!play(2, 0).await.restore, "a clean exit");
        let exit = play(3, 1).await;
        assert!(exit.is_crash());
        assert!(!exit.restore, "it ran fine once since the update");
    }

    #[tokio::test]
    async fn a_run_seals_the_point() {
        let (core, runner, host, _t) = ready().await;
        let id = plain(&core, "Alpha").await;
        updated(&core, &id).await;
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
        // A removal after the game ran starts a new point instead of joining the old one.
        core.mod_remove(&id, "b.jar").await.unwrap();
        let point = core.restore_point(&id).await.unwrap().unwrap();
        assert_eq!(
            point.changes,
            vec![Change::Removed {
                name: "b.jar".into()
            }]
        );
    }

    #[tokio::test]
    async fn a_server_point_names_the_server() {
        use launcher_core::instances::{ServerLink, SourceRef};
        let (core, _, _, _t) = ready().await;
        let id = pack(&core, UpdatePolicy::Auto).await;
        let mut instance = core.instances.load(&id).await.unwrap();
        instance.source = SourceRef::Perland {
            invite: crate::testing::invite('a'),
            engine: "https://api.example.com/".into(),
        };
        instance.server = Some(ServerLink {
            address: None,
            name: "Survival".into(),
            accent: None,
            whitelisted: false,
            last_status: None,
            last_seen: None,
            gone: false,
        });
        core.instances.save(&instance).await.unwrap();
        updated(&core, &id).await;
        let point = core.restore_point(&id).await.unwrap().unwrap();
        assert_eq!(point.server.as_deref(), Some("Survival"));
    }

    #[tokio::test]
    async fn a_play_finishes_an_interrupted_go_back_first() {
        let (core, runner, host, _t) = ready().await;
        let id = plain(&core, "Alpha").await;
        let game = core.instances.game_dir(&id);
        // A leftover staged folder of a go-back that died before its journal.
        std::fs::create_dir_all(game.join(".mods.perland-restore")).unwrap();
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert!(!game.join(".mods.perland-restore").exists());
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
        assert_eq!(host.last_exit(&id).unwrap().error, None);
    }
}

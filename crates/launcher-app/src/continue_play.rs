//! Continue where you left off: the place a run ended at is saved when the game exits, and Home
//! offers it while it still holds (spec 2026-10-21-continue-where-you-left-off-design).
use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use launcher_core::{
    instances::{Instance, ServerAddress, SourceRef},
    last_place::{self, LastPlace},
    Result,
};

use crate::{
    app::AppCore,
    dto::{ContinueKind, ContinueTarget},
    events::Host,
    runner::Runner,
    runs::Destination,
};

/// The last `Connecting to` line a run's game printed, and when the launcher read it.
pub(crate) type Connected = Arc<Mutex<Option<(ServerAddress, SystemTime)>>>;

/// How far before the game's start a world save still counts for this run: file times can be
/// coarser than the clock.
const CLOCK_SLACK: Duration = Duration::from_secs(2);

impl<R: Runner, H: Host> AppCore<R, H> {
    /// Save where the run that started at `started` ended. `own` is the instance's own server.
    /// A run that only saw the menus keeps the old place, and a failed write is only logged.
    pub(crate) async fn record_place(
        &self,
        id: &str,
        own: Option<&ServerAddress>,
        connected: &Connected,
        started: SystemTime,
    ) {
        let game_dir = self.instances.game_dir(id);
        let mut server = connected.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some((address, _)) = &server {
            // A LAN world's port changes every time; only one the player listed is worth it.
            if last_place::is_local(address)
                && last_place::listed_name(&game_dir, address).await.is_none()
            {
                server = None;
            }
        }
        let since = started.checked_sub(CLOCK_SLACK).unwrap_or(started);
        let world = last_place::latest_world(&game_dir, since).await;
        let Some(place) = last_place::decide(server, world, own) else {
            return;
        };
        if let Err(error) = last_place::write(&self.instances.perland_dir(id), &place).await {
            tracing::warn!(instance = id, %error, "couldn't save where the game was");
        }
    }

    /// Where Continue would take the player in instance `id`, or `None` (no key on Home).
    pub async fn continue_target(&self, id: &str) -> Result<Option<ContinueTarget>> {
        let instance = self.instances.load(id).await?;
        Ok(self
            .resolve_place(&instance)
            .await
            .map(|(target, _)| target))
    }

    /// Play straight into the place Continue shows. When it no longer holds (the world went
    /// away since Home asked), this is a plain Play: the player still asked to play.
    pub async fn play_continue(self: &Arc<Self>, id: &str) -> Result<()> {
        let instance = self.instances.load(id).await?;
        let destination = match self.resolve_place(&instance).await {
            Some((_, destination)) => destination,
            None => Destination::Title,
        };
        self.play_to(id, destination).await
    }

    /// The saved place, checked against the files as they are now (spec §5).
    async fn resolve_place(&self, instance: &Instance) -> Option<(ContinueTarget, Destination)> {
        let place = last_place::read(&self.instances.perland_dir(&instance.id)).await?;
        let game_dir = self.instances.game_dir(&instance.id);
        match place {
            LastPlace::World { folder, .. } => {
                let opens = last_place::opens_worlds(&instance.game.minecraft)
                    && last_place::world_exists(&game_dir, &folder).await;
                opens.then(|| {
                    let target = ContinueTarget {
                        kind: ContinueKind::World,
                        name: folder.clone(),
                    };
                    (target, Destination::World(folder))
                })
            }
            LastPlace::Server {
                host, port, own, ..
            } => {
                let address = ServerAddress::new(&host, i64::from(port))?;
                let link = instance
                    .server
                    .as_ref()
                    .filter(|_| matches!(instance.source, SourceRef::Perland { .. }));
                let is_own = own || link.is_some_and(|l| l.address.as_ref() == Some(&address));
                if is_own {
                    // The server may have moved: Play and join follows it. Gone hides it, and so
                    // does an unknown address, which would open the title screen.
                    let link = link.filter(|l| !l.gone && l.address.is_some())?;
                    let name = match link.name.trim() {
                        "" => instance.name.clone(),
                        name => name.to_string(),
                    };
                    let target = ContinueTarget {
                        kind: ContinueKind::Server,
                        name,
                    };
                    return Some((target, Destination::JoinOwn));
                }
                let name = last_place::listed_name(&game_dir, &address).await?;
                let target = ContinueTarget {
                    kind: ContinueKind::Server,
                    name,
                };
                Some((target, Destination::Join(address)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use launcher_core::{
        accounts::BuildKind,
        instances::ServerAddress,
        last_place::{self, LastPlace},
        server_list,
        sync::lockfile,
    };

    use crate::{
        app::tests::{core, TestCore},
        dto::{ContinueKind, ContinueTarget, NewInstance},
        events::RunStateKind::{Idle, Running},
        testing::{desired, exit, invite, serve, server_view, FakeRunner, RecordingHost},
    };

    type Ready = (
        Arc<TestCore>,
        Arc<FakeRunner>,
        Arc<RecordingHost>,
        tempfile::TempDir,
    );

    /// An account and the 1.20.1 instance "alpha" (and `minecraft` for "old" when given).
    async fn ready() -> Ready {
        let (core, runner, host, tmp) = core(BuildKind::Dev);
        crate::testing::sign_in_alex(&core).await;
        for (name, minecraft) in [("Alpha", "1.20.1"), ("Old", "1.19.4")] {
            core.instance_create(NewInstance {
                name: name.into(),
                minecraft: minecraft.into(),
                loader: None,
            })
            .await
            .unwrap();
        }
        (core, runner, host, tmp)
    }

    fn hub() -> ServerAddress {
        ServerAddress::new("hub.example.org", 25565).unwrap()
    }

    fn connecting(address: &ServerAddress) -> String {
        format!(
            "[12:00:01] [Render thread/INFO]: Connecting to {}, {}",
            address.host, address.port
        )
    }

    /// Put `address` in instance `id`'s Multiplayer list as `name`.
    fn list(core: &TestCore, id: &str, name: &str, address: &ServerAddress) {
        let (_, bytes) = server_list::with_server(None, name, address, None, &[]).unwrap();
        let dir = core.instances.game_dir(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(server_list::FILE_NAME), bytes.unwrap()).unwrap();
    }

    fn save_world(core: &TestCore, id: &str, folder: &str) {
        let dir = core.instances.game_dir(id).join("saves").join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("level.dat"), b"nbt").unwrap();
    }

    /// Run `id` until the game is up, call `during`, then end the game.
    async fn run(
        core: &Arc<TestCore>,
        runner: &FakeRunner,
        host: &RecordingHost,
        id: &str,
        during: impl FnOnce(),
    ) {
        core.play(id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(id, Running).await;
        during();
        runner.finish(id, exit(Some(0)));
        host.wait_for(id, Idle).await;
    }

    async fn record(core: &TestCore, id: &str, place: LastPlace) {
        last_place::write(&core.instances.perland_dir(id), &place)
            .await
            .unwrap();
    }

    fn world_place(folder: &str) -> LastPlace {
        LastPlace::World {
            folder: folder.into(),
            at: 1,
        }
    }

    fn server_place(address: &ServerAddress, own: bool) -> LastPlace {
        LastPlace::Server {
            host: address.host.clone(),
            port: address.port,
            own,
            at: 1,
        }
    }

    fn server_target(name: &str) -> Option<ContinueTarget> {
        Some(ContinueTarget {
            kind: ContinueKind::Server,
            name: name.into(),
        })
    }

    fn world_target(name: &str) -> Option<ContinueTarget> {
        Some(ContinueTarget {
            kind: ContinueKind::World,
            name: name.into(),
        })
    }

    #[tokio::test]
    async fn a_server_joined_in_game_becomes_continue() {
        let (core, runner, host, _t) = ready().await;
        list(&core, "alpha", "Hub", &hub());
        *runner.start_lines.lock().unwrap() = vec![connecting(&hub())];
        assert_eq!(core.continue_target("alpha").await.unwrap(), None);
        run(&core, &runner, &host, "alpha", || {}).await;
        assert_eq!(
            core.continue_target("alpha").await.unwrap(),
            server_target("Hub")
        );
    }

    #[tokio::test]
    async fn a_world_played_becomes_continue() {
        let (core, runner, host, _t) = ready().await;
        run(&core, &runner, &host, "alpha", || {
            save_world(&core, "alpha", "Mundo de Ana")
        })
        .await;
        assert_eq!(
            core.continue_target("alpha").await.unwrap(),
            world_target("Mundo de Ana")
        );
    }

    #[tokio::test]
    async fn a_menu_only_run_keeps_the_last_place() {
        let (core, runner, host, _t) = ready().await;
        save_world(&core, "alpha", "Mundo");
        record(&core, "alpha", world_place("Mundo")).await;
        // The world's save is older than this run's start.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(
                core.instances
                    .game_dir("alpha")
                    .join("saves/Mundo/level.dat"),
            )
            .unwrap()
            .set_modified(old)
            .unwrap();
        run(&core, &runner, &host, "alpha", || {}).await;
        assert_eq!(
            core.continue_target("alpha").await.unwrap(),
            world_target("Mundo")
        );
    }

    #[tokio::test]
    async fn an_unlisted_lan_world_is_not_remembered() {
        let (core, runner, host, _t) = ready().await;
        let lan = ServerAddress::new("192.168.1.20", 51234).unwrap();
        *runner.start_lines.lock().unwrap() = vec![connecting(&lan)];
        run(&core, &runner, &host, "alpha", || {}).await;
        assert_eq!(
            last_place::read(&core.instances.perland_dir("alpha")).await,
            None
        );
    }

    #[tokio::test]
    async fn a_deleted_world_hides_continue() {
        let (core, _, _, _t) = ready().await;
        record(&core, "alpha", world_place("Gone")).await;
        assert_eq!(core.continue_target("alpha").await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_old_version_never_offers_a_world() {
        let (core, _, _, _t) = ready().await;
        save_world(&core, "old", "Mundo");
        record(&core, "old", world_place("Mundo")).await;
        assert_eq!(core.continue_target("old").await.unwrap(), None);
        // A server still works there: the launch falls back to --server/--port.
        list(&core, "old", "Hub", &hub());
        record(&core, "old", server_place(&hub(), false)).await;
        assert_eq!(
            core.continue_target("old").await.unwrap(),
            server_target("Hub")
        );
    }

    #[tokio::test]
    async fn a_server_removed_from_the_list_hides_continue() {
        let (core, _, _, _t) = ready().await;
        record(&core, "alpha", server_place(&hub(), false)).await;
        assert_eq!(core.continue_target("alpha").await.unwrap(), None);
    }

    /// A server instance from invite A ("survival" at play.example.com:25566) that synced before.
    async fn server_instance(core: &TestCore, runner: &FakeRunner) -> String {
        serve(runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        lockfile::save_pack(&core.instances.perland_dir(&id), &desired("1.0.4", vec![]))
            .await
            .unwrap();
        id
    }

    fn survival() -> ServerAddress {
        ServerAddress::new("play.example.com", 25566).unwrap()
    }

    #[tokio::test]
    async fn continue_on_the_own_server_plays_and_joins() {
        let (core, runner, host, _t) = ready().await;
        let id = server_instance(&core, &runner).await;
        *runner.start_lines.lock().unwrap() = vec![connecting(&survival())];
        run(&core, &runner, &host, &id, || {}).await;
        assert!(matches!(
            last_place::read(&core.instances.perland_dir(&id)).await,
            Some(LastPlace::Server {
                own: true,
                port: 25566,
                ..
            })
        ));
        assert_eq!(
            core.continue_target(&id).await.unwrap(),
            server_target("survival")
        );
        core.play_continue(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for_nth(&id, Running, 2).await;
        assert_eq!(
            *runner.started_joins.lock().unwrap(),
            vec![None, Some(survival())]
        );
        core.stop(&id).unwrap();
        host.wait_for_nth(&id, Idle, 2).await;
    }

    #[tokio::test]
    async fn a_gone_perland_server_hides_continue() {
        let (core, runner, _, _t) = ready().await;
        let id = server_instance(&core, &runner).await;
        record(&core, &id, server_place(&survival(), true)).await;
        assert!(core.continue_target(&id).await.unwrap().is_some());
        let mut instance = core.instances.load(&id).await.unwrap();
        instance.server.as_mut().unwrap().mark_gone();
        core.instances.save(&instance).await.unwrap();
        assert_eq!(core.continue_target(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn continue_on_another_server_joins_it() {
        let (core, runner, host, _t) = ready().await;
        list(&core, "alpha", "Hub", &hub());
        record(&core, "alpha", server_place(&hub(), false)).await;
        core.play_continue("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        assert_eq!(*runner.started_joins.lock().unwrap(), vec![Some(hub())]);
        core.stop("alpha").unwrap();
        host.wait_for("alpha", Idle).await;
    }

    #[tokio::test]
    async fn continue_on_a_world_starts_it() {
        let (core, runner, host, _t) = ready().await;
        save_world(&core, "alpha", "Mundo");
        record(&core, "alpha", world_place("Mundo")).await;
        core.play_continue("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        assert_eq!(
            *runner.started_worlds.lock().unwrap(),
            vec![Some("Mundo".to_string())]
        );
        core.stop("alpha").unwrap();
        host.wait_for("alpha", Idle).await;
    }

    #[tokio::test]
    async fn continue_with_nothing_plays_normally() {
        let (core, runner, host, _t) = ready().await;
        core.play_continue("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        assert_eq!(*runner.started_joins.lock().unwrap(), vec![None]);
        assert_eq!(*runner.started_worlds.lock().unwrap(), vec![None]);
        core.stop("alpha").unwrap();
        host.wait_for("alpha", Idle).await;
    }
}

//! The seam between the app's run state machine and actually installing and starting a game.
use std::{future::Future, path::PathBuf, pin::Pin, sync::Arc};

use launcher_core::{
    accounts::Session,
    events::EventSink,
    install::Installer,
    instances::{Instance, InstanceStore, ServerAddress},
    invite::{EngineView, InviteLink},
    launch::{GameExit, GameKiller, GameProcess},
    locks::InstanceLock,
    play::{self, Prepared, StartSpec},
    sources::{PackInput, UpdateCheck},
    sync::{Cancel, Ensured, ManualFile, PendingUpdate, Syncer},
    Result,
};

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

pub trait Kill: Send + Sync {
    fn kill(&self);
}

impl Kill for GameKiller {
    fn kill(&self) {
        GameKiller::kill(self)
    }
}

pub trait RunningGame: Send + 'static {
    fn killer(&self) -> Arc<dyn Kill>;
    fn wait(self: Box<Self>) -> BoxFuture<Result<GameExit>>;
}

impl RunningGame for GameProcess {
    fn killer(&self) -> Arc<dyn Kill> {
        Arc::new(GameProcess::killer(self))
    }
    fn wait(self: Box<Self>) -> BoxFuture<Result<GameExit>> {
        Box::pin(GameProcess::wait(*self))
    }
}

/// Installs and starts games. The real one uses `launcher_core::play`; tests use `FakeRunner`.
pub trait Runner: Send + Sync + 'static {
    type Prepared: Send + 'static;
    fn prepare(&self, instance: Instance, sink: EventSink) -> BoxFuture<Result<Self::Prepared>>;
    fn start(
        &self,
        instance: Instance,
        prepared: Self::Prepared,
        session: Session,
        join: Option<ServerAddress>,
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>>;
    /// A new instance for a pack: named after it, with its game version, policy and art.
    /// Its files arrive with the first sync.
    fn add_pack(&self, input: PackInput) -> BoxFuture<Result<Instance>>;
    /// The update this pack instance could take now. `Err` is a warning: play what's installed.
    fn find_update(
        &self,
        instance: Instance,
        sink: EventSink,
    ) -> BoxFuture<Result<Option<PendingUpdate>>>;
    /// The cheap update check only (no pack download): a server's banner.
    fn check_update(&self, instance: Instance) -> BoxFuture<UpdateCheck>;
    /// Bring a pack instance to `accept` (or what's installed). The caller holds `lock`.
    fn sync(
        &self,
        lock: Arc<InstanceLock>,
        id: String,
        accept: Option<PendingUpdate>,
        cancel: Cancel,
        sink: EventSink,
    ) -> BoxFuture<Result<Ensured>>;
    /// Put files the player downloaded by hand into the cache, for instance `id`.
    fn import_manual(&self, id: String, found: Vec<(ManualFile, PathBuf)>)
        -> BoxFuture<Result<()>>;
    /// Browse: Modrinth search and content installs.
    fn browser(&self) -> Arc<dyn crate::browser::Browser>;
    /// The invite's view and its Engine, shared for 15 s by every caller. `engine` seeds the
    /// Engine an instance saved, so no `config.json` is needed.
    fn invite_view(
        &self,
        link: InviteLink,
        engine: Option<String>,
    ) -> BoxFuture<Result<EngineView>>;
    /// The invite's icon PNG, or `None`.
    fn invite_icon(&self, link: InviteLink) -> BoxFuture<Option<Vec<u8>>>;
}

pub struct CoreRunner {
    installer: Arc<Installer>,
    syncer: Arc<Syncer>,
    launcher_version: String,
    browser: Arc<dyn crate::browser::Browser>,
}

impl CoreRunner {
    pub fn new(
        installer: Arc<Installer>,
        syncer: Syncer,
        launcher_version: String,
        browse: launcher_core::browse::Browse,
    ) -> Self {
        let browser = Arc::new(crate::browser::CoreBrowser::new(
            browse,
            installer.meta.clone(),
        ));
        Self {
            installer,
            syncer: Arc::new(syncer),
            launcher_version,
            browser,
        }
    }
    pub fn installer(&self) -> &Arc<Installer> {
        &self.installer
    }
    fn store(&self) -> InstanceStore {
        InstanceStore::new(self.installer.dirs.instances())
    }
}

impl Runner for CoreRunner {
    type Prepared = Prepared;

    fn prepare(&self, instance: Instance, sink: EventSink) -> BoxFuture<Result<Prepared>> {
        let installer = self.installer.clone();
        Box::pin(async move { play::prepare(&installer, &instance, &sink).await })
    }

    fn start(
        &self,
        instance: Instance,
        prepared: Prepared,
        session: Session,
        join: Option<ServerAddress>,
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>> {
        let installer = self.installer.clone();
        let version = self.launcher_version.clone();
        Box::pin(async move {
            let game_dir = InstanceStore::new(installer.dirs.instances()).game_dir(&instance.id);
            let spec = StartSpec {
                dirs: &installer.dirs,
                game_dir: &game_dir,
                instance: &instance,
                prepared: &prepared,
                session: &session,
                platform: installer.platform,
                launcher_version: &version,
                join: join.as_ref(),
            };
            let game = play::start(&spec, sink).await?;
            Ok(Box::new(game) as Box<dyn RunningGame>)
        })
    }

    fn add_pack(&self, input: PackInput) -> BoxFuture<Result<Instance>> {
        let (syncer, store) = (self.syncer.clone(), self.store());
        Box::pin(async move { Ok(syncer.add_pack(&store, &input).await?.0) })
    }

    fn find_update(
        &self,
        instance: Instance,
        sink: EventSink,
    ) -> BoxFuture<Result<Option<PendingUpdate>>> {
        let (syncer, store) = (self.syncer.clone(), self.store());
        Box::pin(async move { syncer.find_update_with(&store, &instance, &sink).await })
    }

    fn check_update(&self, instance: Instance) -> BoxFuture<UpdateCheck> {
        let (syncer, store) = (self.syncer.clone(), self.store());
        Box::pin(async move { syncer.check(&store, &instance).await })
    }

    fn sync(
        &self,
        lock: Arc<InstanceLock>,
        id: String,
        accept: Option<PendingUpdate>,
        cancel: Cancel,
        sink: EventSink,
    ) -> BoxFuture<Result<Ensured>> {
        let (syncer, store) = (self.syncer.clone(), self.store());
        Box::pin(async move {
            syncer
                .ensure_locked(&lock, &store, &id, accept, &sink, &cancel)
                .await
        })
    }

    fn import_manual(
        &self,
        id: String,
        found: Vec<(ManualFile, PathBuf)>,
    ) -> BoxFuture<Result<()>> {
        let syncer = self.syncer.clone();
        Box::pin(async move { syncer.import_found(&id, &found).await })
    }

    fn browser(&self) -> Arc<dyn crate::browser::Browser> {
        self.browser.clone()
    }

    fn invite_view(
        &self,
        link: InviteLink,
        engine: Option<String>,
    ) -> BoxFuture<Result<EngineView>> {
        let invites = self.syncer.sources.invites.clone();
        Box::pin(async move {
            if let Some(engine) = engine {
                invites.remember_engine(&link, &engine);
            }
            invites.view(&link).await
        })
    }

    fn invite_icon(&self, link: InviteLink) -> BoxFuture<Option<Vec<u8>>> {
        let invites = self.syncer.sources.invites.clone();
        Box::pin(async move { invites.icon(&link).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher_core::launch::{spawn, LaunchCommand, Redactor};

    #[tokio::test]
    async fn game_process_is_a_running_game() {
        let tmp = tempfile::tempdir().unwrap();
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("ping", &["-n", "30", "127.0.0.1"])
        } else {
            ("sleep", &["30"])
        };
        let cmd = LaunchCommand {
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: tmp.path().into(),
            classpath: vec![],
        };
        let game = spawn(
            &cmd,
            Redactor::new(),
            launcher_core::events::noop_sink(),
            &tmp.path().join("l.log"),
        )
        .await
        .unwrap();
        let game: Box<dyn RunningGame> = Box::new(game);
        game.killer().kill();
        let exit = tokio::time::timeout(std::time::Duration::from_secs(15), game.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(exit.killed);
    }
}

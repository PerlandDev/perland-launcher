//! The seam between the app's run state machine and actually installing and starting a game.
use std::{future::Future, pin::Pin, sync::Arc};

use launcher_core::{
    accounts::Session,
    events::EventSink,
    install::Installer,
    instances::{Instance, InstanceStore},
    launch::{GameExit, GameKiller, GameProcess},
    play::{self, Prepared, StartSpec},
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
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>>;
}

pub struct CoreRunner {
    installer: Arc<Installer>,
    launcher_version: String,
}

impl CoreRunner {
    pub fn new(installer: Arc<Installer>, launcher_version: String) -> Self {
        Self {
            installer,
            launcher_version,
        }
    }
    pub fn installer(&self) -> &Arc<Installer> {
        &self.installer
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
            };
            let game = play::start(&spec, sink).await?;
            Ok(Box::new(game) as Box<dyn RunningGame>)
        })
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

//! The seam between the app's run state machine and actually installing and starting a game.
use std::{future::Future, path::PathBuf, pin::Pin, sync::Arc, time::Duration};

use launcher_core::{
    accounts::Session,
    events::EventSink,
    install::Installer,
    instances::{Instance, InstanceStore, ServerAddress},
    invite::{EngineView, InviteLink, Url},
    launch::{GameExit, GameKiller, GameProcess},
    locks::InstanceLock,
    news::{NewsClient, NewsPage},
    play::{self, Prepared, StartSpec},
    public_list::{self, FetchedPage, ListQuery, PublicListClient, PublicServer},
    server_wake::{self, Knock, Ping, WaitEnd},
    sources::{PackInput, UpdateCheck},
    sync::{Cancel, Ensured, ManualFile, PendingUpdate, Syncer},
    Result,
};

use crate::{
    manage::{CoreStack, StackApi},
    msa::MsaApi,
    perland::{CorePlayer, PlayerApi},
};

/// How often a woken server is pinged while the run waits for it.
pub const WAKE_EVERY: Duration = Duration::from_secs(2);
/// How long a run waits for a woken server before the game connects anyway.
pub const WAKE_LIMIT: Duration = Duration::from_secs(5 * 60);

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
    /// Install what `instance` needs and find its Java. `repair` checks every file by its hash
    /// and rebuilds a loader's patched files (the crash card's Repair).
    fn prepare(
        &self,
        instance: Instance,
        repair: bool,
        sink: EventSink,
    ) -> BoxFuture<Result<Self::Prepared>>;
    fn start(
        &self,
        instance: Instance,
        prepared: Self::Prepared,
        session: Session,
        join: Option<ServerAddress>,
        world: Option<String>,
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
    /// Put files the player downloaded by hand into the cache.
    fn import_manual(&self, found: Vec<(ManualFile, PathBuf)>) -> BoxFuture<Result<()>>;
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
    /// One page of Perland's public server list, and the Engine it came from.
    fn public_page(&self, query: ListQuery) -> BoxFuture<Result<FetchedPage>>;
    /// A listed server's icon PNG, or `None`.
    fn public_icon(&self, token: String, icon_version: String) -> BoxFuture<Option<Vec<u8>>>;
    /// The invite Play joins for a listed server.
    fn public_invite(&self, server: &PublicServer) -> Option<InviteLink>;
    /// One page of a server's news, or `None` ("no news here"). See `NewsClient::page`.
    fn news_page(
        &self,
        link: InviteLink,
        limit: u8,
        before: Option<String>,
    ) -> BoxFuture<Result<Option<NewsPage>>>;
    /// A news post's picture (PNG or JPEG bytes), or `None`.
    fn news_image(
        &self,
        link: InviteLink,
        post_id: String,
        image_version: String,
    ) -> BoxFuture<Option<Vec<u8>>>;
    /// Who answers a status ping at `address`. A ping never wakes a server.
    fn server_ping(&self, address: ServerAddress) -> BoxFuture<Ping>;
    /// Join as `name` so a sleeping server's waker starts the server; its kick says how it went.
    fn server_knock(&self, address: ServerAddress, name: String) -> BoxFuture<Result<Knock>>;
    /// Ping every [`WAKE_EVERY`] until the real server answers, for at most [`WAKE_LIMIT`].
    fn server_wait(&self, address: ServerAddress, cancel: Cancel) -> BoxFuture<WaitEnd>;
    /// The Engine's player routes (Sign in with Perland, Your servers).
    fn player(&self) -> Arc<dyn PlayerApi>;
    /// The Engine's auth and server routes (Manage your server).
    fn stack(&self) -> Arc<dyn StackApi>;
    /// Microsoft, Xbox and Minecraft services (Sign in with Microsoft).
    fn msa(&self) -> Arc<dyn MsaApi>;
}

pub struct CoreRunner {
    installer: Arc<Installer>,
    syncer: Arc<Syncer>,
    launcher_version: String,
    browser: Arc<dyn crate::browser::Browser>,
    public: PublicListClient,
    news: NewsClient,
    player: Arc<CorePlayer>,
    stack: Arc<CoreStack>,
    msa: Arc<dyn MsaApi>,
}

impl CoreRunner {
    pub fn new(
        installer: Arc<Installer>,
        syncer: Syncer,
        launcher_version: String,
        browse: launcher_core::browse::Browse,
        msa: Arc<dyn MsaApi>,
    ) -> Self {
        let browser = Arc::new(crate::browser::CoreBrowser::new(
            browse,
            installer.meta.clone(),
        ));
        let news = NewsClient::new(syncer.sources.invites.clone());
        let site = public_list::site_url(public_list::DEFAULT_SITE)
            .expect("the default site is a valid Perland site");
        let public = PublicListClient::new(syncer.sources.invites.clone(), site.clone());
        let player = Self::player_for(&syncer, &launcher_version, site);
        // Never follows a redirect: one would forward the device token or a JWT.
        let stack = Arc::new(CoreStack::new(
            launcher_core::http::client_without_redirects(&launcher_version),
        ));
        Self {
            installer,
            syncer: Arc::new(syncer),
            launcher_version,
            browser,
            public,
            news,
            player,
            stack,
            msa,
        }
    }
    /// The player routes go through a client that never follows a redirect (one would replay a
    /// code or verifier body to another host), to the Engine `site` names.
    fn player_for(syncer: &Syncer, launcher_version: &str, site: Url) -> Arc<CorePlayer> {
        Arc::new(CorePlayer::new(
            launcher_core::http::client_without_redirects(launcher_version),
            syncer.sources.invites.clone(),
            site,
        ))
    }
    /// Read the public server list from another Perland site (a staging build, or a developer's
    /// own). `site` should come from [`public_list::site_url`].
    pub fn with_site(mut self, site: Url) -> Self {
        self.public = PublicListClient::new(self.syncer.sources.invites.clone(), site.clone());
        self.player = Self::player_for(&self.syncer, &self.launcher_version, site);
        self
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

    fn prepare(
        &self,
        instance: Instance,
        repair: bool,
        sink: EventSink,
    ) -> BoxFuture<Result<Prepared>> {
        let installer = self.installer.clone();
        Box::pin(async move { play::prepare(&installer, &instance, repair, &sink).await })
    }

    fn start(
        &self,
        instance: Instance,
        prepared: Prepared,
        session: Session,
        join: Option<ServerAddress>,
        world: Option<String>,
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
                world: world.as_deref(),
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

    fn import_manual(&self, found: Vec<(ManualFile, PathBuf)>) -> BoxFuture<Result<()>> {
        let syncer = self.syncer.clone();
        Box::pin(async move { syncer.import_found(&found).await })
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

    fn public_page(&self, query: ListQuery) -> BoxFuture<Result<FetchedPage>> {
        let public = self.public.clone();
        Box::pin(async move { public.page(&query).await })
    }

    fn public_icon(&self, token: String, icon_version: String) -> BoxFuture<Option<Vec<u8>>> {
        let public = self.public.clone();
        Box::pin(async move { public.icon(&token, &icon_version).await })
    }

    fn public_invite(&self, server: &PublicServer) -> Option<InviteLink> {
        self.public.invite_for(server)
    }

    fn news_page(
        &self,
        link: InviteLink,
        limit: u8,
        before: Option<String>,
    ) -> BoxFuture<Result<Option<NewsPage>>> {
        let news = self.news.clone();
        Box::pin(async move { news.page(&link, limit, before.as_deref()).await })
    }

    fn news_image(
        &self,
        link: InviteLink,
        post_id: String,
        image_version: String,
    ) -> BoxFuture<Option<Vec<u8>>> {
        let news = self.news.clone();
        Box::pin(async move { news.image(&link, &post_id, &image_version).await })
    }

    fn server_ping(&self, address: ServerAddress) -> BoxFuture<Ping> {
        Box::pin(async move { server_wake::ping(&address).await })
    }

    fn server_knock(&self, address: ServerAddress, name: String) -> BoxFuture<Result<Knock>> {
        Box::pin(async move { server_wake::knock(&address, &name).await })
    }

    fn server_wait(&self, address: ServerAddress, cancel: Cancel) -> BoxFuture<WaitEnd> {
        Box::pin(async move {
            server_wake::wait_until_up(&address, &cancel, WAKE_EVERY, WAKE_LIMIT).await
        })
    }

    fn player(&self) -> Arc<dyn PlayerApi> {
        self.player.clone()
    }

    fn stack(&self) -> Arc<dyn StackApi> {
        self.stack.clone()
    }

    fn msa(&self) -> Arc<dyn MsaApi> {
        self.msa.clone()
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

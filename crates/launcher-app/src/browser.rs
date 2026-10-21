//! The seam between the app and Browse. The real one is `launcher_core::browse::Browse`; tests
//! use `FakeBrowser`.
use std::{collections::HashMap, path::PathBuf, sync::Arc};

use launcher_core::{
    browse::{
        compat::{loader_tag, ContentKind, Target},
        identify::Installed,
        images::{Image, ImageKind},
        plan::{InstallPlan, PlanItem, PlanRequest},
        project::{ProjectDetail, VersionInfo},
        search::{SearchPage, SearchQuery},
        Browse,
    },
    events::{EventSink, TaskId},
    meta::{catalog, loaders::LoaderKind, MetaClient},
    share::Linked,
    sync::lockfile::Managed,
    ErrorCode, LauncherError, Result,
};

use crate::runner::BoxFuture;

pub trait Browser: Send + Sync + 'static {
    fn search(&self, query: SearchQuery) -> BoxFuture<Result<SearchPage>>;
    fn categories(&self, kind: ContentKind) -> BoxFuture<Result<Vec<String>>>;
    fn project(&self, id: String) -> BoxFuture<Result<ProjectDetail>>;
    fn versions(&self, id: String, target: Option<Target>) -> BoxFuture<Result<Vec<VersionInfo>>>;
    fn image(&self, url: String, kind: ImageKind) -> BoxFuture<Result<Image>>;
    fn identify(&self, game_dir: PathBuf, managed: Option<Managed>)
        -> BoxFuture<Result<Installed>>;
    fn plan(
        &self,
        request: PlanRequest,
        target: Option<Target>,
        installed: Installed,
    ) -> BoxFuture<Result<InstallPlan>>;
    fn install(
        &self,
        game_dir: PathBuf,
        items: Vec<PlanItem>,
        managed: Option<Managed>,
        task: TaskId,
        sink: EventSink,
    ) -> BoxFuture<Result<Vec<String>>>;
    /// The loader version a new instance gets: the newest stable one, else the newest.
    fn loader_version(&self, kind: LoaderKind, minecraft: String) -> BoxFuture<Result<String>>;
    /// The icon URL of the Modrinth project a file with this sha1 belongs to.
    fn project_icon_url(&self, sha1: String) -> BoxFuture<Result<Option<String>>>;
    /// The files Modrinth serves among these sha1s (see `launcher_core::share::lookup`).
    fn share_lookup(&self, sha1s: Vec<String>) -> BoxFuture<Result<HashMap<String, Linked>>>;
}

pub struct CoreBrowser {
    browse: Arc<Browse>,
    meta: MetaClient,
}

impl CoreBrowser {
    pub fn new(browse: Browse, meta: MetaClient) -> Self {
        Self {
            browse: Arc::new(browse),
            meta,
        }
    }
}

impl Browser for CoreBrowser {
    fn search(&self, query: SearchQuery) -> BoxFuture<Result<SearchPage>> {
        let b = self.browse.clone();
        Box::pin(async move { b.search(&query).await })
    }
    fn categories(&self, kind: ContentKind) -> BoxFuture<Result<Vec<String>>> {
        let b = self.browse.clone();
        Box::pin(async move { b.categories(kind).await })
    }
    fn project(&self, id: String) -> BoxFuture<Result<ProjectDetail>> {
        let b = self.browse.clone();
        Box::pin(async move { b.project(&id).await })
    }
    fn versions(&self, id: String, target: Option<Target>) -> BoxFuture<Result<Vec<VersionInfo>>> {
        let b = self.browse.clone();
        Box::pin(async move { b.versions(&id, target.as_ref()).await })
    }
    fn image(&self, url: String, kind: ImageKind) -> BoxFuture<Result<Image>> {
        let b = self.browse.clone();
        Box::pin(async move { b.image(&url, kind).await })
    }
    fn identify(
        &self,
        game_dir: PathBuf,
        managed: Option<Managed>,
    ) -> BoxFuture<Result<Installed>> {
        let b = self.browse.clone();
        Box::pin(async move { b.identify(&game_dir, managed.as_ref()).await })
    }
    fn plan(
        &self,
        request: PlanRequest,
        target: Option<Target>,
        installed: Installed,
    ) -> BoxFuture<Result<InstallPlan>> {
        let b = self.browse.clone();
        Box::pin(async move { b.plan(&request, target.as_ref(), &installed).await })
    }
    fn install(
        &self,
        game_dir: PathBuf,
        items: Vec<PlanItem>,
        managed: Option<Managed>,
        task: TaskId,
        sink: EventSink,
    ) -> BoxFuture<Result<Vec<String>>> {
        let b = self.browse.clone();
        Box::pin(async move {
            b.install(&game_dir, &items, managed.as_ref(), &task, &sink)
                .await
        })
    }
    fn loader_version(&self, kind: LoaderKind, minecraft: String) -> BoxFuture<Result<String>> {
        let meta = self.meta.clone();
        Box::pin(async move {
            let list = catalog::loader_versions(&meta, kind, &minecraft).await?;
            list.iter()
                .find(|v| v.stable)
                .or(list.first())
                .map(|v| v.version.clone())
                .ok_or_else(|| {
                    LauncherError::loader(
                        ErrorCode::LoaderVersionNotFound,
                        format!("No {} version for Minecraft {minecraft}", loader_tag(kind)),
                    )
                    .with_param("loader", loader_tag(kind))
                    .with_param("loader_version", "latest")
                    .with_param("version", &minecraft)
                })
        })
    }
    fn project_icon_url(&self, sha1: String) -> BoxFuture<Result<Option<String>>> {
        let b = self.browse.clone();
        Box::pin(async move { b.project_icon_url(&sha1).await })
    }
    fn share_lookup(&self, sha1s: Vec<String>) -> BoxFuture<Result<HashMap<String, Linked>>> {
        let b = self.browse.clone();
        Box::pin(async move { launcher_core::share::lookup(&b.api, &sha1s).await })
    }
}

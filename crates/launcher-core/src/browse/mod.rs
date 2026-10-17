//! Browse (Plan 3c): Modrinth search, projects, and installing mods, resource packs and shaders
//! into an instance with their required dependencies. Modpacks go through `sources` instead.
pub mod api;
pub mod compat;
pub mod identify;
pub mod images;
pub mod install;
pub mod plan;
pub mod project;
pub mod search;

use std::{collections::HashMap, path::Path};

use tokio::sync::Mutex;

use self::{
    api::Api,
    compat::{ContentKind, Target},
    identify::{FileHashes, Installed},
    images::{Image, ImageCache, ImageKind},
    plan::{InstallPlan, PlanItem, PlanRequest},
    project::{ProjectDetail, VersionInfo},
    search::{SearchPage, SearchQuery},
};
use crate::{
    download::{cache::ObjectCache, Downloader},
    error::Result,
    events::{EventSink, TaskId},
    paths::LauncherDirs,
    sync::lockfile::Managed,
};

/// Upstream addresses, overridable for tests.
#[derive(Debug, Clone)]
pub struct BrowseConfig {
    pub api: String,
    pub cdn: String,
}

impl Default for BrowseConfig {
    fn default() -> Self {
        Self {
            api: "https://api.modrinth.com/v2".into(),
            cdn: "https://cdn.modrinth.com".into(),
        }
    }
}

/// Everything Browse does, for the app. One per process: it remembers file hashes and
/// categories between calls.
pub struct Browse {
    pub api: Api,
    images: ImageCache,
    cache: ObjectCache,
    downloader: Downloader,
    hashes: Mutex<FileHashes>,
    categories: Mutex<HashMap<ContentKind, Vec<String>>>,
}

impl Browse {
    /// `http` talks to the Modrinth API. `image_http` fetches CDN images and must come from
    /// `http::client_without_redirects`, so an image URL can never lead off the CDN origin.
    pub fn new(
        http: reqwest::Client,
        image_http: reqwest::Client,
        downloader: Downloader,
        dirs: &LauncherDirs,
        config: BrowseConfig,
    ) -> Self {
        Self {
            api: Api::new(http, config.api),
            images: ImageCache::new(image_http, dirs.images(), &config.cdn),
            cache: ObjectCache::new(dirs.objects()),
            downloader,
            hashes: Mutex::new(FileHashes::default()),
            categories: Mutex::new(HashMap::new()),
        }
    }

    pub async fn search(&self, query: &SearchQuery) -> Result<SearchPage> {
        search::search(&self.api, query).await
    }

    pub async fn categories(&self, kind: ContentKind) -> Result<Vec<String>> {
        if let Some(known) = self.categories.lock().await.get(&kind) {
            return Ok(known.clone());
        }
        let fresh = search::categories(&self.api, kind).await?;
        self.categories.lock().await.insert(kind, fresh.clone());
        Ok(fresh)
    }

    pub async fn project(&self, id: &str) -> Result<ProjectDetail> {
        project::project(&self.api, id).await
    }

    pub async fn versions(&self, id: &str, target: Option<&Target>) -> Result<Vec<VersionInfo>> {
        project::versions(&self.api, id, target).await
    }

    pub async fn identify(&self, game_dir: &Path, managed: Option<&Managed>) -> Result<Installed> {
        let mut hashes = self.hashes.lock().await;
        identify::identify(&self.api, game_dir, managed, &mut hashes).await
    }

    /// The Modrinth project icon for a file with this sha1 (see [`identify::project_icon_url`]).
    pub async fn project_icon_url(&self, sha1: &str) -> Result<Option<String>> {
        identify::project_icon_url(&self.api, sha1).await
    }

    pub async fn plan(
        &self,
        request: &PlanRequest,
        target: Option<&Target>,
        installed: &Installed,
    ) -> Result<InstallPlan> {
        plan::plan(&self.api, request, target, installed).await
    }

    pub async fn install(
        &self,
        game_dir: &Path,
        items: &[PlanItem],
        managed: Option<&Managed>,
        task: &TaskId,
        sink: &EventSink,
    ) -> Result<Vec<String>> {
        install::install(
            &self.cache,
            &self.downloader,
            game_dir,
            items,
            managed,
            task,
            sink,
        )
        .await
    }

    pub async fn image(&self, url: &str, kind: ImageKind) -> Result<Image> {
        self.images.get(url, kind).await
    }
}

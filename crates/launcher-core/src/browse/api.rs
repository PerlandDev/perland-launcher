//! Modrinth's public API v2 for Browse. Plain requests with the launcher's User-Agent (the
//! client comes from `http::client`), so a rate limit or a missing project gets its own error
//! instead of the pack wording `sources::modrinth` uses.
use std::{collections::HashMap, fmt::Display};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    download::host_of,
    error::{ErrorCode, LauncherError, Result},
    sources::valid_modrinth_id,
};

/// No answer Browse asks for comes near this; a bigger one is refused rather than buffered.
pub const MAX_JSON_BYTES: usize = 8 << 20;
/// How long a rate-limited player waits when Modrinth doesn't say.
const DEFAULT_WAIT_SECS: u64 = 10;

#[derive(Debug, Clone, Deserialize)]
pub struct ApiProject {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub body: String,
    pub project_type: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub client_side: String,
    #[serde(default)]
    pub server_side: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub license: Option<ApiLicense>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub issues_url: Option<String>,
    #[serde(default)]
    pub wiki_url: Option<String>,
    #[serde(default)]
    pub gallery: Vec<ApiGallery>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiLicense {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiGallery {
    pub url: String,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiUser {
    pub username: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiMember {
    #[serde(default)]
    pub role: String,
    pub user: ApiUser,
    #[serde(default)]
    pub ordering: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ApiHashes {
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub hashes: ApiHashes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyType {
    Required,
    Optional,
    Incompatible,
    Embedded,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiDependency {
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiVersion {
    pub id: String,
    pub project_id: String,
    #[serde(default)]
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    pub date_published: String,
    #[serde(default)]
    pub files: Vec<ApiFile>,
    #[serde(default)]
    pub dependencies: Vec<ApiDependency>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiHit {
    pub project_id: String,
    pub project_type: String,
    pub slug: String,
    #[serde(default)]
    pub author: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub display_categories: Vec<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub follows: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub client_side: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiSearch {
    pub hits: Vec<ApiHit>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub total_hits: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiCategory {
    pub name: String,
    pub project_type: String,
    #[serde(default)]
    pub header: String,
}

pub fn not_found() -> LauncherError {
    LauncherError::pack(ErrorCode::ContentNotFound, "That project wasn't found")
}

fn checked(id: &str) -> Result<&str> {
    if valid_modrinth_id(id) {
        Ok(id)
    } else {
        Err(not_found())
    }
}

fn unreachable(host: &str, e: reqwest::Error) -> LauncherError {
    LauncherError::network(ErrorCode::HostUnreachable, format!("Couldn't reach {host}"))
        .with_host(host)
        .with_fix("Check your internet connection")
        .with_detail(e.without_url())
}

fn dropped(host: &str, e: reqwest::Error) -> LauncherError {
    LauncherError::network(
        ErrorCode::ConnectionDropped,
        format!("Downloading from {host} was interrupted"),
    )
    .with_host(host)
    .with_detail(e.without_url())
}

fn unexpected(host: &str, detail: impl Display) -> LauncherError {
    LauncherError::loader(
        ErrorCode::MetaUnexpected,
        format!("Unexpected data from {host}"),
    )
    .with_host(host)
    .with_detail(detail)
}

/// A non-2xx answer: rate limits say how long to wait, 404/410 mean the project is gone, and
/// anything else is the host having trouble.
fn status_error(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    host: &str,
) -> LauncherError {
    match status.as_u16() {
        429 => {
            let seconds = headers
                .get("x-ratelimit-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(DEFAULT_WAIT_SECS)
                .clamp(1, 60);
            LauncherError::network(
                ErrorCode::BrowseRateLimited,
                format!("Too many searches. Try again in {seconds} s"),
            )
            .with_param("seconds", seconds)
        }
        404 | 410 => not_found(),
        code => LauncherError::network(
            ErrorCode::HostTrouble,
            format!("{host} is having trouble ({status})"),
        )
        .with_host(host)
        .with_param("status", code)
        .with_fix("Try again in a few minutes"),
    }
}

#[derive(Debug, Clone)]
pub struct Api {
    http: reqwest::Client,
    base: String,
    limit: usize,
}

impl Api {
    pub fn new(http: reqwest::Client, base: impl Into<String>) -> Self {
        Self {
            http,
            base: base.into().trim_end_matches('/').to_string(),
            limit: MAX_JSON_BYTES,
        }
    }

    /// The same client with a smaller cap on answers (tests use a few bytes).
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn url(&self, path: &str, query: &[(&str, String)]) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse(&format!("{}/{path}", self.base))
            .map_err(|e| unexpected(&host_of(&self.base), e))?;
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }
        Ok(url)
    }

    pub async fn get<T: DeserializeOwned>(&self, url: reqwest::Url) -> Result<T> {
        let host = host_of(url.as_str());
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| unreachable(&host, e))?;
        self.read(response, &host).await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        url: reqwest::Url,
        body: &B,
    ) -> Result<T> {
        let host = host_of(url.as_str());
        let response = self
            .http
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|e| unreachable(&host, e))?;
        self.read(response, &host).await
    }

    async fn read<T: DeserializeOwned>(
        &self,
        mut response: reqwest::Response,
        host: &str,
    ) -> Result<T> {
        let status = response.status();
        if !status.is_success() {
            return Err(status_error(status, response.headers(), host));
        }
        if response
            .content_length()
            .is_some_and(|n| n > self.limit as u64)
        {
            return Err(unexpected(host, "the answer is too big"));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| dropped(host, e))? {
            body.extend_from_slice(&chunk);
            if body.len() > self.limit {
                return Err(unexpected(host, "the answer is too big"));
            }
        }
        serde_json::from_slice(&body).map_err(|e| unexpected(host, e))
    }

    pub async fn project(&self, id: &str) -> Result<ApiProject> {
        self.get(self.url(&format!("project/{}", checked(id)?), &[])?)
            .await
    }

    pub async fn members(&self, id: &str) -> Result<Vec<ApiMember>> {
        self.get(self.url(&format!("project/{}/members", checked(id)?), &[])?)
            .await
    }

    /// A project's versions, filtered by Modrinth loader tags and one Minecraft version.
    pub async fn versions(
        &self,
        id: &str,
        loaders: &[&str],
        game_version: Option<&str>,
    ) -> Result<Vec<ApiVersion>> {
        let mut query = Vec::new();
        if !loaders.is_empty() {
            query.push(("loaders", serde_json::to_string(loaders).expect("strings")));
        }
        if let Some(mc) = game_version {
            query.push((
                "game_versions",
                serde_json::to_string(&[mc]).expect("strings"),
            ));
        }
        self.get(self.url(&format!("project/{}/version", checked(id)?), &query)?)
            .await
    }

    pub async fn version(&self, id: &str) -> Result<ApiVersion> {
        self.get(self.url(&format!("version/{}", checked(id)?), &[])?)
            .await
    }

    /// The versions files with these sha1s belong to, keyed by sha1. Unknown files are absent.
    pub async fn version_files(&self, sha1s: &[String]) -> Result<HashMap<String, ApiVersion>> {
        #[derive(Serialize)]
        struct Body<'a> {
            hashes: &'a [String],
            algorithm: &'static str,
        }
        self.post(
            self.url("version_files", &[])?,
            &Body {
                hashes: sha1s,
                algorithm: "sha1",
            },
        )
        .await
    }

    pub async fn search(&self, query: &[(&str, String)]) -> Result<ApiSearch> {
        self.get(self.url("search", query)?).await
    }

    pub async fn categories(&self) -> Result<Vec<ApiCategory>> {
        self.get(self.url("tag/category", &[])?).await
    }
}

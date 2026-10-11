//! A project's page and its versions (spec §3.3).
use serde::{Deserialize, Serialize};

use super::{
    api::{not_found, Api, ApiVersion},
    compat::{fits, loader_tags, usable_file, ContentKind, Target, SHADER_TAG},
    search::only_categories,
};
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideSupport {
    Required,
    Optional,
    Unsupported,
    #[serde(other)]
    Unknown,
}

fn side(s: &str) -> SideSupport {
    match s {
        "required" => SideSupport::Required,
        "optional" => SideSupport::Optional,
        "unsupported" => SideSupport::Unsupported,
        _ => SideSupport::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryImage {
    pub url: String,
    pub title: Option<String>,
    pub featured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub author: Option<String>,
    pub summary: String,
    /// Markdown, as the author wrote it. The UI renders it without raw HTML.
    pub body: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub follows: u64,
    pub categories: Vec<String>,
    pub kind: ContentKind,
    pub client_side: SideSupport,
    pub server_side: SideSupport,
    pub license: Option<String>,
    pub source_url: Option<String>,
    pub issues_url: Option<String>,
    pub wiki_url: Option<String>,
    pub gallery: Vec<GalleryImage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: String,
    pub name: String,
    pub number: String,
    pub version_type: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub date: String,
    /// The size of the file that would be installed.
    pub size: Option<u64>,
}

pub async fn project(api: &Api, id: &str) -> Result<ProjectDetail> {
    let p = api.project(id).await?;
    let kind = ContentKind::from_project_type(&p.project_type).ok_or_else(not_found)?;
    // The author is nice to have: a project owned by an organization may list nobody.
    let members = api.members(&p.id).await.unwrap_or_default();
    let author = members
        .iter()
        .find(|m| m.role.eq_ignore_ascii_case("owner"))
        .or_else(|| members.iter().min_by_key(|m| m.ordering))
        .map(|m| m.user.username.clone());
    let mut gallery: Vec<GalleryImage> = p
        .gallery
        .into_iter()
        .map(|g| GalleryImage {
            url: g.url,
            title: g.title,
            featured: g.featured,
        })
        .collect();
    gallery.sort_by_key(|g| !g.featured);
    Ok(ProjectDetail {
        project_id: p.id,
        slug: p.slug,
        title: p.title,
        author,
        summary: p.description,
        body: p.body,
        icon_url: p.icon_url,
        downloads: p.downloads,
        follows: p.followers,
        categories: only_categories(p.categories),
        kind,
        client_side: side(&p.client_side),
        server_side: side(&p.server_side),
        license: p.license.map(|l| l.name.unwrap_or(l.id)),
        source_url: p.source_url,
        issues_url: p.issues_url,
        wiki_url: p.wiki_url,
        gallery,
    })
}

/// Versions of `project_id`, asked for with the filters that fit `target`.
pub async fn fetch_versions(
    api: &Api,
    project_id: &str,
    kind: ContentKind,
    target: Option<&Target>,
) -> Result<Vec<ApiVersion>> {
    let minecraft = target.map(|t| t.minecraft.as_str());
    let tags: Vec<&str> = match (kind, target.and_then(|t| t.loader)) {
        (ContentKind::Mod, Some(loader)) => loader_tags(loader, minecraft),
        (ContentKind::Shader, _) => vec![SHADER_TAG],
        _ => vec![],
    };
    api.versions(project_id, &tags, minecraft).await
}

/// The versions the player can pick: those that fit `target` when there is one, else every
/// version with an installable file. Newest first.
pub async fn versions(api: &Api, id: &str, target: Option<&Target>) -> Result<Vec<VersionInfo>> {
    let p = api.project(id).await?;
    let kind = ContentKind::from_project_type(&p.project_type).ok_or_else(not_found)?;
    let list = fetch_versions(api, &p.id, kind, target).await?;
    let mut keep: Vec<&ApiVersion> = list
        .iter()
        .filter(|v| match target {
            Some(t) if kind != ContentKind::Modpack => fits(v, kind, t),
            _ => usable_file(v, kind).is_some(),
        })
        .collect();
    keep.sort_by(|a, b| b.date_published.cmp(&a.date_published));
    Ok(keep
        .into_iter()
        .map(|v| VersionInfo {
            id: v.id.clone(),
            name: v.name.clone(),
            number: v.version_number.clone(),
            version_type: v.version_type.clone(),
            game_versions: v.game_versions.clone(),
            loaders: v.loaders.clone(),
            date: v.date_published.clone(),
            size: usable_file(v, kind).map(|f| f.size),
        })
        .collect())
}

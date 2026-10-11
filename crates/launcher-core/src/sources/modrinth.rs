//! Modrinth modpacks through the public API v2 (no key; the launcher's descriptive User-Agent).
//! Only modpack projects become instances; Browse (Plan 3c) handles the rest.
use serde::Deserialize;

use super::{
    check_pack_url, invalid, mrpack, pack_not_found, unsupported, valid_modrinth_id, DesiredState,
    PackAbout, PackArt, Sources, UpdateCheck,
};
use crate::{
    download::{host_of, redact_url, Hashes},
    error::{ErrorCode, LauncherError, Result},
    instances::SourceRef,
};

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub project_type: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub gallery: Vec<GalleryImage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GalleryImage {
    pub url: String,
    #[serde(default)]
    pub featured: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    pub date_published: String,
    #[serde(default)]
    pub files: Vec<VersionFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    pub size: u64,
    pub hashes: VersionHashes,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionHashes {
    pub sha1: String,
    #[serde(default)]
    pub sha512: Option<String>,
}

fn api(src: &Sources, path: &str) -> String {
    format!("{}/{path}", src.config.modrinth_api.trim_end_matches('/'))
}

fn checked_id(id: &str) -> Result<&str> {
    if valid_modrinth_id(id) {
        Ok(id)
    } else {
        Err(LauncherError::pack(
            ErrorCode::PackLinkInvalid,
            "That doesn't look like a pack link",
        ))
    }
}

pub async fn project(src: &Sources, id: &str) -> Result<Project> {
    let url = api(src, &format!("project/{}", checked_id(id)?));
    src.meta.get_json(&url).await.map_err(pack_not_found)
}

pub async fn version(src: &Sources, id: &str) -> Result<Version> {
    let url = api(src, &format!("version/{}", checked_id(id)?));
    src.meta.get_json(&url).await.map_err(pack_not_found)
}

/// A project's versions, optionally only those for one Minecraft version. Always fresh.
pub async fn versions(
    src: &Sources,
    project_id: &str,
    minecraft: Option<&str>,
) -> Result<Vec<Version>> {
    let mut url = reqwest::Url::parse(&api(
        src,
        &format!("project/{}/version", checked_id(project_id)?),
    ))
    .map_err(|e| invalid("Modrinth address", e))?;
    if let Some(mc) = minecraft {
        url.query_pairs_mut()
            .append_pair("game_versions", &format!("[\"{mc}\"]"));
    }
    let host = host_of(url.as_str());
    let shown = redact_url(url.as_str());
    let response = src.http.get(url).send().await.map_err(|e| {
        LauncherError::network(ErrorCode::HostUnreachable, format!("Couldn't reach {host}"))
            .with_param("host", &host)
            .with_fix("Check your internet connection")
            .with_detail(e.without_url())
    })?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(pack_not_found(
            LauncherError::loader(ErrorCode::MetaNotFound, "not found")
                .with_param("status", status.as_u16())
                .with_detail(shown),
        ));
    }
    if !status.is_success() {
        return Err(LauncherError::network(
            ErrorCode::HostTrouble,
            format!("{host} is having trouble ({status})"),
        )
        .with_param("host", &host)
        .with_param("status", status.as_u16()));
    }
    response.json().await.map_err(|e| {
        LauncherError::loader(
            ErrorCode::MetaUnexpected,
            format!("Unexpected data from {host}"),
        )
        .with_detail(e.without_url())
    })
}

fn stability(kind: &str) -> u8 {
    match kind {
        "release" => 0,
        "beta" => 1,
        _ => 2,
    }
}

/// The newest version at least as stable as the installed one.
pub fn pick_update<'a>(versions: &'a [Version], installed_type: &str) -> Option<&'a Version> {
    versions
        .iter()
        .filter(|v| stability(&v.version_type) <= stability(installed_type))
        .max_by(|a, b| a.date_published.cmp(&b.date_published))
}

fn mrpack_file(v: &Version) -> Result<&VersionFile> {
    v.files
        .iter()
        .filter(|f| f.filename.to_ascii_lowercase().ends_with(".mrpack"))
        .max_by_key(|f| f.primary)
        .ok_or_else(|| unsupported("a Modrinth version without a .mrpack file"))
}

fn banner(gallery: &[GalleryImage]) -> Option<String> {
    gallery
        .iter()
        .find(|g| g.featured)
        .or_else(|| gallery.first())
        .map(|g| g.url.clone())
}

pub async fn resolve(src: &Sources, project_id: &str, version_id: &str) -> Result<DesiredState> {
    let project = project(src, project_id).await?;
    if project.project_type != "modpack" {
        return Err(unsupported(&format!(
            "Modrinth {} projects",
            project.project_type
        )));
    }
    let version = version(src, version_id).await?;
    if version.project_id != project.id {
        return Err(invalid("Modrinth version", "it belongs to another project"));
    }
    let file = mrpack_file(&version)?;
    if check_pack_url(&file.url).is_err() {
        return Err(unsupported("a .mrpack that isn't served over https"));
    }
    let hashes = Hashes {
        sha1: Hashes::from_format("sha1", &file.hashes.sha1).and_then(|h| h.sha1),
        sha512: file
            .hashes
            .sha512
            .as_deref()
            .and_then(|s| Hashes::from_format("sha512", s))
            .and_then(|h| h.sha512),
        ..Hashes::default()
    };
    if hashes.cache_key().is_none() {
        return Err(invalid("Modrinth version", "no usable checksum"));
    }
    let path = src
        .cache
        .ensure(
            &src.downloader,
            std::slice::from_ref(&file.url),
            &hashes,
            Some(file.size),
        )
        .await?;
    let (stamp, archive) = (version.id.clone(), hashes);
    let mut desired = tokio::task::spawn_blocking(move || {
        mrpack::read_archive(&path, stamp, archive, mrpack::Limits::default())
    })
    .await
    .map_err(|e| LauncherError::from(std::io::Error::other(e)))??;
    desired.name = project.title.trim().chars().take(64).collect();
    desired.pack_version = version.version_number;
    desired.about = PackAbout {
        description: project.description,
        art: PackArt {
            icon: project.icon_url,
            banner: banner(&project.gallery),
        },
        policy: None,
    };
    Ok(desired)
}

/// A modpack by slug or id, at `version` or its newest release (else newest of any kind).
pub async fn open(
    src: &Sources,
    project_ref: &str,
    version: Option<&str>,
) -> Result<(SourceRef, DesiredState)> {
    let p = project(src, project_ref).await?;
    if p.project_type != "modpack" {
        return Err(unsupported(&format!(
            "Modrinth {} projects",
            p.project_type
        )));
    }
    let version_id = match version {
        Some(v) => v.to_string(),
        None => {
            let list = versions(src, &p.id, None).await?;
            pick_update(&list, "release")
                .or_else(|| pick_update(&list, "alpha"))
                .map(|v| v.id.clone())
                .ok_or_else(|| unsupported("a Modrinth modpack with no versions"))?
        }
    };
    let desired = resolve(src, &p.id, &version_id).await?;
    Ok((
        SourceRef::Modrinth {
            project_id: p.id,
            version_id,
        },
        desired,
    ))
}

/// Is there a newer version for this Minecraft, at least as stable as the installed one?
pub async fn check(
    src: &Sources,
    project_id: &str,
    installed_version_id: &str,
    minecraft: &str,
) -> UpdateCheck {
    let failed = |e: LauncherError| {
        UpdateCheck::Unavailable(
            LauncherError::network(ErrorCode::UpdateCheckFailed, "Couldn't check for updates")
                .with_fix("You can still play the version you have")
                .with_detail(e.message),
        )
    };
    // A version deleted upstream must not hide updates forever: compare by id against releases.
    let installed = match version(src, installed_version_id).await {
        Ok(v) => Some(v),
        Err(e) if e.code == ErrorCode::PackNotFound => None,
        Err(e) => return failed(e),
    };
    let mut list = match versions(src, project_id, Some(minecraft)).await {
        Ok(list) => list,
        Err(e) => return failed(e),
    };
    // The query filters too, but updates must never move to another Minecraft version.
    list.retain(|v| v.game_versions.iter().any(|g| g == minecraft));
    let installed_type = installed
        .as_ref()
        .map_or("release", |v| v.version_type.as_str());
    match pick_update(&list, installed_type) {
        Some(v)
            if v.id != installed_version_id
                && installed
                    .as_ref()
                    .is_none_or(|i| v.date_published > i.date_published) =>
        {
            UpdateCheck::Available {
                source: SourceRef::Modrinth {
                    project_id: project_id.to_string(),
                    version_id: v.id.clone(),
                },
                stamp: v.id.clone(),
                version: v.version_number.clone(),
            }
        }
        _ => UpdateCheck::UpToDate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: &str, kind: &str, date: &str) -> Version {
        Version {
            id: id.into(),
            project_id: "p".into(),
            name: id.into(),
            version_number: id.into(),
            version_type: kind.into(),
            game_versions: vec!["1.20.1".into()],
            date_published: date.into(),
            files: vec![],
        }
    }

    #[test]
    fn updates_never_get_less_stable() {
        let list = vec![
            v("alpha-new", "alpha", "2024-03-01T00:00:00Z"),
            v("beta-mid", "beta", "2024-02-01T00:00:00Z"),
            v("release-old", "release", "2024-01-01T00:00:00Z"),
        ];
        assert_eq!(pick_update(&list, "release").unwrap().id, "release-old");
        assert_eq!(pick_update(&list, "beta").unwrap().id, "beta-mid");
        assert_eq!(pick_update(&list, "alpha").unwrap().id, "alpha-new");
        assert!(pick_update(&list[..1], "release").is_none());
    }
}

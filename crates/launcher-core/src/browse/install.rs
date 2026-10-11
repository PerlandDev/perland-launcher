//! Writing planned content into the game folder (spec §3.6). Every file is verified in the
//! object cache first, then created next to its final name and linked in without replacing
//! anything: a player's file is never overwritten.
use std::{io::ErrorKind, path::Path};

use super::plan::PlanItem;
use crate::{
    download::{cache::ObjectCache, hash::sha1_file, Downloader},
    error::{ErrorCode, LauncherError, Result},
    events::{Event, EventSink, TaskId},
    fsutil::{path_key, safe_component},
    sync::lockfile::Managed,
};

fn name_invalid(name: &str) -> LauncherError {
    LauncherError::disk(ErrorCode::ModNameInvalid, "That isn't a mod file name").with_detail(name)
}

fn exists(name: &str) -> LauncherError {
    LauncherError::disk(
        ErrorCode::ModExists,
        format!("{name} is already in this instance"),
    )
    .with_param("file", name)
}

fn managed_path(name: &str) -> LauncherError {
    LauncherError::disk(ErrorCode::ModManaged, format!("{name} comes with the pack"))
        .with_param("file", name)
}

fn join_error(e: tokio::task::JoinError) -> LauncherError {
    std::io::Error::other(e).into()
}

/// The file's path relative to the game folder, if its name is one safe component with the
/// kind's extension.
fn relative_path(item: &PlanItem) -> Result<String> {
    let name = &item.file.file_name;
    let folder = item.kind.folder().ok_or_else(|| name_invalid(name))?;
    let extension = item.kind.extension();
    if !safe_component(name)
        || name.len() <= extension.len()
        || !name.to_ascii_lowercase().ends_with(extension)
    {
        return Err(name_invalid(name));
    }
    Ok(format!("{folder}/{name}"))
}

/// Move `tmp` to `dest` unless something is already at `dest`. A hard link fails atomically
/// when the name is taken; file systems without hard links fall back to check-then-rename.
fn link_no_replace(tmp: &Path, dest: &Path) -> std::io::Result<()> {
    let result = match std::fs::hard_link(tmp, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Err(e),
        Err(_) if dest.exists() => Err(ErrorKind::AlreadyExists.into()),
        Err(_) => return std::fs::rename(tmp, dest),
    };
    let _ = std::fs::remove_file(tmp);
    result
}

/// Where a disabled copy of `dest` would be.
fn disabled_twin(dest: &Path) -> std::path::PathBuf {
    let mut disabled = dest.as_os_str().to_owned();
    disabled.push(".disabled");
    disabled.into()
}

async fn same_file(path: &Path, sha1: Option<&str>) -> Result<bool> {
    let (Some(expected), owned) = (sha1, path.to_path_buf()) else {
        return Ok(false);
    };
    let actual = tokio::task::spawn_blocking(move || sha1_file(&owned))
        .await
        .map_err(join_error)??;
    Ok(actual.eq_ignore_ascii_case(expected))
}

pub async fn install(
    cache: &ObjectCache,
    downloader: &Downloader,
    game_dir: &Path,
    items: &[PlanItem],
    managed: Option<&Managed>,
    task: &TaskId,
    sink: &EventSink,
) -> Result<Vec<String>> {
    // Refuse bad names and pack-owned paths before anything is downloaded.
    let mut paths = Vec::with_capacity(items.len());
    for item in items {
        let rel = relative_path(item)?;
        let key = path_key(&rel);
        if managed.is_some_and(|m| m.files.keys().any(|k| path_key(k) == key)) {
            return Err(managed_path(&item.file.file_name));
        }
        // Conflicts are found now, so a clash never leaves earlier items half-installed. The
        // checks in the loop below stay as the safety net against a race.
        let dest = game_dir.join(&rel);
        if tokio::fs::try_exists(&dest).await?
            && !same_file(&dest, item.file.hashes.sha1.as_deref()).await?
        {
            return Err(exists(&item.file.file_name));
        }
        if tokio::fs::try_exists(disabled_twin(&dest)).await? {
            return Err(exists(&item.file.file_name));
        }
        paths.push(rel);
    }
    let total = items.len() as u64;
    let (mut done, mut bytes) = (0u64, 0u64);
    let progress = |done, bytes| {
        sink(Event::Progress {
            task: task.clone(),
            done,
            total,
            bytes,
        })
    };
    progress(done, bytes);
    let mut added = Vec::new();
    for (item, rel) in items.iter().zip(&paths) {
        let name = &item.file.file_name;
        let dest = game_dir.join(rel);
        let disabled = disabled_twin(&dest);
        if tokio::fs::try_exists(&dest).await? {
            if !same_file(&dest, item.file.hashes.sha1.as_deref()).await? {
                return Err(exists(name));
            }
        } else if tokio::fs::try_exists(&disabled).await? {
            return Err(exists(name));
        } else {
            let object = cache
                .ensure(
                    downloader,
                    std::slice::from_ref(&item.file.url),
                    &item.file.hashes,
                    Some(item.file.size),
                )
                .await?;
            let dir = dest.parent().expect("a folder and a name");
            tokio::fs::create_dir_all(dir).await?;
            let tmp = dir.join(format!(".{name}.{}.perland-tmp", std::process::id()));
            if let Err(e) = tokio::fs::copy(&object, &tmp).await {
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(e.into());
            }
            let (from, to) = (tmp.clone(), dest.clone());
            match tokio::task::spawn_blocking(move || link_no_replace(&from, &to))
                .await
                .map_err(join_error)?
            {
                Ok(()) => added.push(name.clone()),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => return Err(exists(name)),
                Err(e) => {
                    let _ = tokio::fs::remove_file(&tmp).await;
                    return Err(e.into());
                }
            }
        }
        done += 1;
        bytes += item.file.size;
        progress(done, bytes);
    }
    Ok(added)
}

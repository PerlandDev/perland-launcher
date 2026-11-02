//! OS file locks. One per instance, held while its files change (sync, and from Plan 3b a
//! running game); one for installing shared game files, so the app and the CLI never write them
//! at the same time. The OS releases both when the process exits, so a crash never leaves a
//! stale lock behind.
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
};

use crate::error::{ErrorCategory, ErrorCode, LauncherError, Result};

fn open_lock_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
}

/// Unlock explicitly instead of relying on closing the file. A child process spawned by any
/// thread holds a copy of every open fd from its fork until its exec, and the OS lock belongs to
/// the open file, so closing only our fd could leave the lock held a moment longer.
fn release(file: &File) {
    // Nothing useful to do on failure: closing the file still releases the lock eventually.
    let _ = file.unlock();
}

/// Held while something changes an instance's files. Dropping it releases the lock.
#[derive(Debug)]
pub struct InstanceLock {
    file: File,
}

impl InstanceLock {
    /// Lock `<perland_dir>/lock`, or fail at once with `instance_locked` while anyone else
    /// (this process or another) holds it.
    pub fn try_acquire(perland_dir: &Path) -> Result<Self> {
        let file = open_lock_file(&perland_dir.join("lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { file }),
            Err(TryLockError::WouldBlock) => Err(LauncherError::game(
                ErrorCode::InstanceLocked,
                "Another Perland Launcher window is using this instance",
            )
            .with_fix("Close the other window, or wait for it to finish")),
            Err(TryLockError::Error(e)) => Err(e.into()),
        }
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        release(&self.file);
    }
}

/// Held while shared game files (versions, libraries, assets) are installed.
#[derive(Debug)]
pub struct InstallLock {
    file: File,
}

impl InstallLock {
    /// Wait until the lock at `path` is free, then hold it.
    pub async fn acquire(path: &Path) -> Result<Self> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<Self> {
            let file = open_lock_file(&path)?;
            file.lock()?;
            Ok(Self { file })
        })
        .await
        .map_err(|e| {
            LauncherError::new(
                ErrorCategory::Disk,
                ErrorCode::Internal,
                "Couldn't wait for another install to finish",
            )
            .with_detail(e)
        })?
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        release(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;
    use std::time::Duration;

    #[test]
    fn a_second_instance_lock_is_refused_until_the_first_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let first = InstanceLock::try_acquire(dir.path()).unwrap();
        assert_eq!(
            InstanceLock::try_acquire(dir.path()).unwrap_err().code,
            ErrorCode::InstanceLocked
        );
        drop(first);
        InstanceLock::try_acquire(dir.path()).unwrap();
    }

    // A child process spawned by another thread holds a copy of every open fd between its fork
    // and exec, and a flock belongs to the open file, not the fd. A duplicate handle stands in
    // for that child: dropping the lock must still release it at once.
    #[test]
    fn dropping_an_instance_lock_releases_it_while_a_duplicate_handle_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let first = InstanceLock::try_acquire(dir.path()).unwrap();
        let _inherited = first.file.try_clone().unwrap();
        drop(first);
        InstanceLock::try_acquire(dir.path()).unwrap();
    }

    #[tokio::test]
    async fn dropping_an_install_lock_releases_it_while_a_duplicate_handle_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".install.lock");
        let first = InstallLock::acquire(&path).await.unwrap();
        let _inherited = first.file.try_clone().unwrap();
        drop(first);
        let file = open_lock_file(&path).unwrap();
        file.try_lock().expect("released on drop");
    }

    #[tokio::test]
    async fn install_lock_waits_for_the_holder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/.install.lock");
        let first = InstallLock::acquire(&path).await.unwrap();
        let p2 = path.clone();
        let waiter = tokio::spawn(async move { InstallLock::acquire(&p2).await });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!waiter.is_finished(), "must wait while the lock is held");
        drop(first);
        tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .expect("acquired once released")
            .unwrap()
            .unwrap();
    }
}

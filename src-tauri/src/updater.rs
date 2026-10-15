//! The real `UpdateSource`: Tauri's updater against the channel manifests on GitHub (spec §4.1,
//! §4.7). Every behaviour decision lives in `launcher_app::updates`; this only adapts the plugin.
use std::sync::Arc;

use launcher_app::{
    runner::BoxFuture,
    updates::{Progress, UpdateSource},
};
use launcher_core::{ErrorCode, LauncherError, Result};
use tauri::AppHandle;
use tauri_plugin_updater::{Error, Update, UpdaterExt};

pub struct TauriUpdateSource {
    app: AppHandle,
}

impl TauriUpdateSource {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

/// The plugin's update, as found by a check.
pub struct Found {
    update: Update,
}

fn failed(error: Error) -> LauncherError {
    match error {
        // A bad signature, a signature for another version, or a manifest that isn't ours:
        // never installed.
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::InvalidUpdaterFormat
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => LauncherError::integrity(
            ErrorCode::LauncherUpdateInvalid,
            "The launcher update didn't pass its safety check, so it wasn't installed",
        )
        .with_detail(error),
        other => LauncherError::network(
            ErrorCode::LauncherUpdateFailed,
            "Couldn't check for launcher updates",
        )
        .with_fix("You can keep playing. It tries again next time you open the launcher")
        .with_detail(other),
    }
}

/// Check `endpoint`. The updater is built with `restart_after_install(false)`: the plugin passes
/// the NSIS installer `/R` (relaunch) only when it is on, and `install` turns it on for an
/// update that should reopen the launcher (spec §4.7). `on_before_exit` is never set, so the
/// plugin's default cleanup still runs before an install.
async fn check_at(app: &AppHandle, endpoint: &'static str) -> Result<Option<Update>> {
    let url = endpoint.parse().map_err(|e| {
        LauncherError::network(
            ErrorCode::LauncherUpdateFailed,
            "Couldn't check for launcher updates",
        )
        .with_detail(e)
    })?;
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(failed)?
        .restart_after_install(false)
        .build()
        .map_err(failed)?;
    match updater.check().await {
        Ok(found) => Ok(found),
        // No release on this channel yet (stable before Plan 4): nothing newer (Review Focus 1).
        Err(Error::ReleaseNotFound) => Ok(None),
        Err(e) => Err(failed(e)),
    }
}

impl UpdateSource for TauriUpdateSource {
    type Update = Found;

    fn check(&self, endpoint: &'static str) -> BoxFuture<Result<Option<Found>>> {
        let app = self.app.clone();
        Box::pin(async move {
            Ok(check_at(&app, endpoint)
                .await?
                .map(|update| Found { update }))
        })
    }

    fn version(&self, found: &Found) -> String {
        found.update.version.clone()
    }

    fn download(&self, found: Arc<Found>, progress: Progress) -> BoxFuture<Result<Vec<u8>>> {
        Box::pin(async move {
            found
                .update
                .download(move |chunk, total| progress(chunk as u64, total), || {})
                .await
                .map_err(failed)
        })
    }

    fn install(
        &self,
        found: Arc<Found>,
        bytes: Arc<Vec<u8>>,
        relaunch: bool,
    ) -> BoxFuture<Result<()>> {
        let app = self.app.clone();
        Box::pin(async move {
            // Windows: starts the installer and ends the process; `/R` relaunches only when asked.
            // Linux: replaces the AppImage or installs the .deb (pkexec asks for the password);
            // Linux never relaunches by itself.
            found
                .update
                .clone()
                .restart_after_install(relaunch)
                .install(bytes.as_slice())
                .map_err(failed)?;
            if relaunch && !cfg!(windows) {
                app.restart();
            }
            Ok(())
        })
    }
}

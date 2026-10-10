//! The launch steps both front ends share: get everything ready, then start the game.
use std::path::{Path, PathBuf};

use crate::{
    accounts::Session,
    error::{ErrorCode, LauncherError, Result},
    events::EventSink,
    install::{Installer, ResolvedVersion},
    instances::Instance,
    java,
    launch::{self, GameProcess, LaunchCommand, LaunchSpec, Redactor},
    paths::LauncherDirs,
    platform::Platform,
};

/// Everything a launch needs that `prepare` downloaded or found.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub resolved: ResolvedVersion,
    pub java: PathBuf,
}

/// Resolve the version, install its files, and find Java (the instance's custom one, or Mojang's).
pub async fn prepare(
    installer: &Installer,
    instance: &Instance,
    sink: &EventSink,
) -> Result<Prepared> {
    let loader = instance
        .game
        .loader
        .as_ref()
        .map(|l| (l.kind, l.version.as_str()));
    let resolved = installer.resolve(&instance.game.minecraft, loader).await?;
    installer.install(&resolved, sink).await?;
    let java = match &instance.java.path {
        Some(path) => custom_java(path)?,
        None => {
            java::ensure_runtime(
                &installer.meta,
                &installer.downloader,
                &installer.dirs.java(),
                installer.platform,
                &java::component_for(&resolved.json),
                sink,
            )
            .await?
        }
    };
    Ok(Prepared { resolved, java })
}

/// A custom Java path must point at an existing file; checked before anything is spawned.
pub fn custom_java(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        Ok(path.to_path_buf())
    } else {
        Err(
            LauncherError::java(ErrorCode::JavaPathMissing, "The chosen Java wasn't found")
                .with_fix("Pick Java again in the instance settings, or switch back to automatic")
                .with_detail(path.display()),
        )
    }
}

pub struct StartSpec<'a> {
    pub dirs: &'a LauncherDirs,
    pub game_dir: &'a Path,
    pub instance: &'a Instance,
    pub prepared: &'a Prepared,
    pub session: &'a Session,
    pub platform: Platform,
    pub launcher_version: &'a str,
}

pub fn command(spec: &StartSpec) -> Result<LaunchCommand> {
    launch::build_command(&LaunchSpec {
        dirs: spec.dirs,
        resolved: &spec.prepared.resolved,
        game_dir: spec.game_dir,
        java: &spec.prepared.java,
        session: spec.session,
        memory_mb: spec.instance.java.memory_mb,
        platform: spec.platform,
        launcher_version: spec.launcher_version,
    })
}

/// Fail before spawning when Java or a classpath entry is missing.
pub fn check_files(cmd: &LaunchCommand) -> Result<()> {
    let missing = launch::missing_files(cmd);
    if missing.is_empty() {
        return Ok(());
    }
    let list = missing
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    Err(LauncherError::game(
        ErrorCode::GameFilesMissing,
        format!("{} game files are missing", missing.len()),
    )
    .with_param("count", missing.len())
    .with_fix("Press Play again to repair the instance")
    .with_detail(list))
}

/// Build the command, check its files, and start the game with a redacted log.
pub async fn start(spec: &StartSpec<'_>, sink: EventSink) -> Result<GameProcess> {
    let cmd = command(spec)?;
    check_files(&cmd)?;
    let redactor = Redactor::for_current_user().with_secret(spec.session.access_token.clone());
    let log = spec
        .dirs
        .logs()
        .join(format!("{}-latest.log", spec.instance.id));
    launch::spawn(&cmd, redactor, sink, &log).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounts::offline_account,
        instances::{GameSpec, InstanceStore},
        test_support::version,
        ErrorCode,
    };

    #[test]
    fn custom_java_must_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope/java.exe");
        assert_eq!(
            custom_java(&missing).unwrap_err().code,
            ErrorCode::JavaPathMissing
        );
        let present = tmp.path().join("java.exe");
        std::fs::write(&present, "").unwrap();
        assert_eq!(custom_java(&present).unwrap(), present);
    }

    #[tokio::test]
    async fn start_refuses_when_files_are_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path());
        let store = InstanceStore::new(dirs.instances());
        let instance = store
            .create(
                "Smoke",
                GameSpec {
                    minecraft: "1.20.1".into(),
                    loader: None,
                },
            )
            .await
            .unwrap();
        let java = tmp.path().join("java.exe");
        std::fs::write(&java, "").unwrap();
        let prepared = Prepared {
            resolved: ResolvedVersion {
                vanilla_id: "1.20.1".into(),
                json: version("1.20.1.json"),
            },
            java,
        };
        let session = offline_account("Steve").unwrap().offline_session();
        let game_dir = store.game_dir(&instance.id);
        let spec = StartSpec {
            dirs: &dirs,
            game_dir: &game_dir,
            instance: &instance,
            prepared: &prepared,
            session: &session,
            platform: Platform::current(),
            launcher_version: "0.1.0",
        };
        let err = start(&spec, crate::events::noop_sink())
            .await
            .err()
            .expect("libraries are not on disk");
        assert_eq!(err.code, ErrorCode::GameFilesMissing);
        let count: usize = err.params["count"].parse().unwrap();
        assert!(count > 10, "{count}");
    }
}

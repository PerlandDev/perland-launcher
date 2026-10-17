//! The launcher's own log (spec §5.1): `logs/launcher/launcher.YYYY-MM-DD.log`, a new file each
//! day, the newest 14 kept, the home path masked on every line. Its own folder, so pruning never
//! touches a game's `<id>-latest.log`. Writes are synchronous (no background worker): nothing is
//! lost when an update install ends the process, and the volume is small.
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

use tracing_appender::rolling::{RollingFileAppender, RollingWriter, Rotation};
use tracing_subscriber::{fmt::MakeWriter, EnvFilter};

use crate::{error::LauncherError, launch::Redactor, paths::LauncherDirs};

pub const KEEP_FILES: usize = 14;

pub fn dir(dirs: &LauncherDirs) -> PathBuf {
    dirs.logs().join("launcher")
}

/// The rolling log file; every line written through it is redacted first.
pub struct RedactingFile {
    appender: RollingFileAppender,
    redactor: Redactor,
}

impl RedactingFile {
    pub fn new(dir: &Path, redactor: Redactor) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("launcher")
            .filename_suffix("log")
            .max_log_files(KEEP_FILES)
            .build(dir)
            .map_err(io::Error::other)?;
        Ok(Self { appender, redactor })
    }
}

pub struct RedactingWriter<'a> {
    inner: RollingWriter<'a>,
    redactor: &'a Redactor,
}

impl Write for RedactingWriter<'_> {
    // The formatter writes one whole event per call, so a home path is never split.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        self.inner
            .write_all(self.redactor.redact(&text).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<'a> MakeWriter<'a> for RedactingFile {
    type Writer = RedactingWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter {
            inner: self.appender.make_writer(),
            redactor: &self.redactor,
        }
    }
}

/// The launcher's crates; `PERLAND_LOG` sets their level and nothing else's.
const OUR_CRATES: [&str; 3] = ["launcher_core", "launcher_app", "perland_launcher"];

/// The filter for a `PERLAND_LOG` value: a plain level name (`debug`, ...) applies to our crates
/// only, and everything else stays at `warn`. Dependencies at debug log full request URLs (an
/// invite's path carries its token), so anything but a plain level falls back to `info`.
pub fn directives(perland_log: Option<&str>) -> String {
    let wanted = perland_log.unwrap_or("").trim().to_ascii_lowercase();
    let level = match wanted.as_str() {
        "trace" | "debug" | "info" | "warn" | "error" => wanted.as_str(),
        _ => "info",
    };
    let mut out = String::from("warn");
    for name in OUR_CRATES {
        out.push_str(&format!(",{name}={level}"));
    }
    out
}

/// Start logging. Never fails the launcher: without a log file it simply runs without one.
/// `PERLAND_LOG` (e.g. `debug`) sets our crates' level, `info` by default (see `directives`).
pub fn init(dirs: &LauncherDirs) {
    let Ok(file) = RedactingFile::new(&dir(dirs), Redactor::for_current_user()) else {
        return;
    };
    let wanted = std::env::var("PERLAND_LOG").ok();
    let filter = EnvFilter::new(directives(wanted.as_deref()));
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .with_env_filter(filter)
        .with_writer(file)
        .try_init();
}

/// A command's error, as the player saw it plus its technical detail. Never a token: errors
/// never carry them (spec §5.1), and the home path is masked by the writer.
pub fn log_error(command: &str, error: &LauncherError) {
    // The player pressing Cancel on an import is an answer, not a failure. A server list that
    // isn't open yet is the Servers screen's normal state, and it asks again every 30 s.
    if matches!(
        error.code,
        crate::ErrorCode::ImportCancelled | crate::ErrorCode::ServerListClosed
    ) {
        return;
    }
    tracing::warn!(
        command,
        code = ?error.code,
        detail = error.detail.as_deref().unwrap_or(""),
        "{}",
        error.message
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    fn only_file(dir: &Path) -> PathBuf {
        let files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        files[0].clone()
    }

    #[test]
    fn perland_log_sets_the_level_of_our_crates_only() {
        let ours = |lvl: &str| {
            format!("warn,launcher_core={lvl},launcher_app={lvl},perland_launcher={lvl}")
        };
        assert_eq!(directives(None), ours("info"));
        assert_eq!(directives(Some("debug")), ours("debug"));
        assert_eq!(directives(Some(" TRACE ")), ours("trace"));
        assert_eq!(directives(Some("error")), ours("error"));
        // Anything but a plain level is ignored, so no one can raise reqwest or hyper with it.
        for bad in ["", "reqwest=debug", "debug,hyper=trace", "loud", "info,"] {
            assert_eq!(directives(Some(bad)), ours("info"), "{bad:?}");
        }
        // And the result parses.
        EnvFilter::try_new(directives(Some("debug"))).unwrap();
    }

    #[test]
    fn the_launcher_log_has_its_own_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path());
        assert_eq!(dir(&dirs), dirs.logs().join("launcher"));
    }

    #[test]
    fn lines_reach_a_daily_file_with_the_home_path_masked() {
        let tmp = tempfile::tempdir().unwrap();
        let file =
            RedactingFile::new(tmp.path(), Redactor::new().with_home("/home/steve")).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(file)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(path = "/home/steve/.minecraft", "opened");
        });
        let log = only_file(tmp.path());
        let name = log.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("launcher.") && name.ends_with(".log"),
            "{name}"
        );
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("~/.minecraft"), "{text}");
        assert!(!text.contains("/home/steve"), "{text}");
    }

    #[test]
    fn a_windows_home_path_is_masked_when_debug_formatting_doubles_the_backslashes() {
        let tmp = tempfile::tempdir().unwrap();
        let file =
            RedactingFile::new(tmp.path(), Redactor::new().with_home(r"C:\Users\steve")).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(file)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(path = r"C:\Users\steve\x", "opened");
        });
        let text = std::fs::read_to_string(only_file(tmp.path())).unwrap();
        assert!(text.contains('~'), "{text}");
        assert!(!text.contains(r"Users\steve"), "{text}");
        assert!(!text.contains(r"Users\\steve"), "{text}");
    }

    #[test]
    fn errors_log_their_code_and_detail() {
        let tmp = tempfile::tempdir().unwrap();
        let file = RedactingFile::new(tmp.path(), Redactor::new()).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(file)
            .finish();
        let error =
            LauncherError::network(ErrorCode::HostUnreachable, "Couldn't reach example.com")
                .with_detail("dns failed");
        tracing::subscriber::with_default(subscriber, || log_error("play", &error));
        let text = std::fs::read_to_string(only_file(tmp.path())).unwrap();
        assert!(text.contains("WARN"), "{text}");
        assert!(text.contains("command=\"play\""), "{text}");
        assert!(text.contains("HostUnreachable"), "{text}");
        assert!(text.contains("dns failed"), "{text}");
    }

    #[test]
    fn a_players_import_cancel_stays_out_of_the_log() {
        let tmp = tempfile::tempdir().unwrap();
        let file = RedactingFile::new(tmp.path(), Redactor::new()).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(file)
            .finish();
        let cancelled =
            LauncherError::disk(ErrorCode::ImportCancelled, "The import was cancelled.");
        let failed = LauncherError::disk(ErrorCode::ImportNotInstance, "Not an instance");
        tracing::subscriber::with_default(subscriber, || {
            log_error("import_instance", &cancelled);
            log_error("import_instance", &failed);
        });
        let text = std::fs::read_to_string(only_file(tmp.path())).unwrap();
        assert!(!text.contains("ImportCancelled"), "{text}");
        assert!(text.contains("ImportNotInstance"), "{text}");
    }

    #[test]
    fn a_closed_server_list_stays_out_of_the_log() {
        let tmp = tempfile::tempdir().unwrap();
        let file = RedactingFile::new(tmp.path(), Redactor::new()).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(file)
            .finish();
        let closed = LauncherError::network(ErrorCode::ServerListClosed, "The list isn't open");
        let failed = LauncherError::network(ErrorCode::ServerListFailed, "Couldn't load it");
        tracing::subscriber::with_default(subscriber, || {
            log_error("servers_list", &closed);
            log_error("servers_list", &failed);
        });
        let text = std::fs::read_to_string(only_file(tmp.path())).unwrap();
        assert!(!text.contains("ServerListClosed"), "{text}");
        assert!(text.contains("ServerListFailed"), "{text}");
    }
}

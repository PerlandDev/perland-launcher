//! Why a game that exited on its own with an error crashed: what the game left behind is read
//! (redacted first), a small rule table names the cause, and the mods it names are found on disk.
use std::{cell::OnceCell, path::Path, time::SystemTime};

use serde::Serialize;

use crate::{
    error::{ErrorCategory, ErrorCode, LauncherError},
    launch::Redactor,
};

mod evidence;
mod mods;
mod rules;

pub use mods::JarMod;

pub use rules::{classify, rule_codes, MixinOwner};

/// Where a piece of evidence came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The last output lines the launcher kept.
    Tail,
    /// The newest `crash-reports/crash-*.txt` the run wrote.
    CrashReport,
    /// `logs/latest.log`.
    LatestLog,
    /// The JVM's fatal error log, `hs_err_pid<pid>.log`.
    HsErr,
}

/// One redacted text a rule may read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub source: Source,
    pub text: String,
}

/// A mod a cause names: its id from the log, its name (from its jar, else the log) and, when it
/// was found among the enabled jars, its file in `mods/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModRef {
    pub id: String,
    pub name: Option<String>,
    pub file: Option<String>,
}

impl ModRef {
    /// What the player reads: the name, else the id.
    pub fn shown(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Vendor {
    Amd,
    Nvidia,
    Intel,
}

impl Vendor {
    fn as_str(self) -> &'static str {
        match self {
            Vendor::Amd => "amd",
            Vendor::Nvidia => "nvidia",
            Vendor::Intel => "intel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrashCause {
    OutOfMemory,
    GraphicsDriver {
        vendor: Option<Vendor>,
    },
    JavaTooOld {
        required: u32,
        running: Option<u32>,
    },
    JavaTooNew,
    ModMissingDependency {
        module: ModRef,
        missing: String,
    },
    /// `module` declares it breaks with `other`.
    ModsClash {
        module: ModRef,
        other: ModRef,
    },
    ModWrongVersion {
        module: ModRef,
    },
    ModFailed {
        module: ModRef,
    },
    /// A jar in `mods/` that can't be read: its id is the file name the log gives, and it has a
    /// `file` only when that is an enabled jar on disk.
    ModFileDamaged {
        module: ModRef,
    },
    GameFilesDamaged,
}

/// A recognised crash: its cause, and the (redacted) log lines that show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crash {
    pub cause: CrashCause,
    pub detail: String,
}

impl Crash {
    pub fn code(&self) -> ErrorCode {
        match &self.cause {
            CrashCause::OutOfMemory => ErrorCode::CrashOutOfMemory,
            CrashCause::GraphicsDriver { .. } => ErrorCode::CrashGraphicsDriver,
            CrashCause::JavaTooOld { .. } => ErrorCode::CrashJavaTooOld,
            CrashCause::JavaTooNew => ErrorCode::CrashJavaTooNew,
            CrashCause::ModMissingDependency { .. } => ErrorCode::CrashModMissingDependency,
            CrashCause::ModsClash { .. } => ErrorCode::CrashModsClash,
            CrashCause::ModWrongVersion { .. } => ErrorCode::CrashModWrongVersion,
            CrashCause::ModFailed { .. } => ErrorCode::CrashModFailed,
            CrashCause::ModFileDamaged { .. } => ErrorCode::CrashModFileDamaged,
            CrashCause::GameFilesDamaged => ErrorCode::CrashGameFilesDamaged,
        }
    }

    /// The cause as the crash card shows it. The UI translates by code; this is the fallback.
    pub fn error(&self) -> LauncherError {
        use ErrorCategory as C;
        let (category, message, fix): (_, String, &str) = match &self.cause {
            CrashCause::OutOfMemory => (
                C::Game,
                "Minecraft ran out of memory".into(),
                "Give it more memory and press Play again",
            ),
            CrashCause::GraphicsDriver { .. } => (
                C::Game,
                "Your graphics driver crashed the game".into(),
                "Update your graphics driver, restart your computer and press Play again",
            ),
            CrashCause::JavaTooOld { required, .. } => (
                C::Java,
                format!("This game needs Java {required} or newer"),
                "Switch to a newer Java and press Play again",
            ),
            CrashCause::JavaTooNew => (
                C::Java,
                "This version of Minecraft needs an older Java".into(),
                "Use the recommended Java and press Play again",
            ),
            CrashCause::ModMissingDependency { module, missing } => (
                C::Loader,
                format!("{} needs {missing}, which isn't installed", module.shown()),
                "Add it and press Play again",
            ),
            CrashCause::ModsClash { module, other } => (
                C::Loader,
                format!(
                    "{} and {} don't work together",
                    module.shown(),
                    other.shown()
                ),
                "Turn one of them off and press Play again",
            ),
            CrashCause::ModWrongVersion { module } => (
                C::Loader,
                format!(
                    "{} is made for a different version of Minecraft",
                    module.shown()
                ),
                "Turn it off, or get the version made for this game",
            ),
            CrashCause::ModFailed { module } => (
                C::Loader,
                format!("{} broke while the game was loading", module.shown()),
                "Turn it off and press Play again",
            ),
            CrashCause::ModFileDamaged { module } => (
                C::Integrity,
                format!("The file for {} is damaged", module.shown()),
                "Turn it off and download it again",
            ),
            CrashCause::GameFilesDamaged => (
                C::Integrity,
                "Some game files are damaged".into(),
                "Repair them and play again",
            ),
        };
        let mut error = LauncherError::new(category, self.code(), message)
            .with_fix(fix)
            .with_detail(&self.detail);
        for (key, value) in self.params() {
            error = error.with_param(key, value);
        }
        error
    }

    fn params(&self) -> Vec<(&'static str, String)> {
        match &self.cause {
            CrashCause::OutOfMemory | CrashCause::JavaTooNew | CrashCause::GameFilesDamaged => {
                vec![]
            }
            CrashCause::GraphicsDriver { vendor } => vendor
                .iter()
                .map(|v| ("vendor", v.as_str().to_string()))
                .collect(),
            CrashCause::JavaTooOld { required, running } => {
                let mut params = vec![("required", required.to_string())];
                params.extend(running.map(|r| ("running", r.to_string())));
                params
            }
            CrashCause::ModMissingDependency { module, missing } => vec![
                ("mod", module.shown().to_string()),
                ("missing", missing.clone()),
            ],
            CrashCause::ModsClash { module, other } => vec![
                ("mod", module.shown().to_string()),
                ("other", other.shown().to_string()),
            ],
            CrashCause::ModWrongVersion { module }
            | CrashCause::ModFailed { module }
            | CrashCause::ModFileDamaged { module } => {
                vec![("mod", module.shown().to_string())]
            }
        }
    }
}

/// Why the game in `game_dir` (process `pid`, started at `started`) crashed, if a rule knows.
/// `tail` is the output the launcher kept, already redacted; the files are redacted here.
pub async fn diagnose(
    game_dir: &Path,
    pid: Option<u32>,
    started: SystemTime,
    tail: &[String],
    redactor: &Redactor,
) -> Option<Crash> {
    let evidence = evidence::gather(game_dir, pid, started, tail, redactor).await;
    let dir = game_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        // Each jar is opened once at most, however many mixin lines the logs hold.
        let jars = OnceCell::new();
        let jars = || jars.get_or_init(|| mods::scan(&dir));
        let configs = OnceCell::new();
        let owner = |config: &str| {
            let file = configs
                .get_or_init(|| mods::mixin_configs(&dir))
                .get(config)?;
            jars()
                .iter()
                .find(|j| &j.file == file)
                .map(|j| j.id.clone())
        };
        let mut crash = classify(&evidence, &owner)?;
        mods::resolve(&mut crash.cause, jars(), &mods::enabled_files(&dir));
        Some(crash)
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn diagnose_names_the_mod_from_its_jar() {
        let tmp = tempfile::tempdir().unwrap();
        mods::tests::jar(
            tmp.path(),
            "create-1.20.1-0.5.1.jar",
            &[
                (
                    "META-INF/mods.toml",
                    "[[mods]]\nmodId=\"create\"\ndisplayName=\"Create\"\n",
                ),
                ("create.mixins.json", "{}"),
            ],
        );
        let tail = [
            "[main/FATAL] [mixin/]: Mixin apply failed create.mixins.json:a.B -> c.D".to_string(),
        ];
        let crash = diagnose(tmp.path(), None, SystemTime::now(), &tail, &Redactor::new())
            .await
            .unwrap();
        assert_eq!(
            crash.cause,
            CrashCause::ModFailed {
                module: ModRef {
                    id: "create".into(),
                    name: Some("Create".into()),
                    file: Some("create-1.20.1-0.5.1.jar".into()),
                }
            }
        );
        assert_eq!(crash.error().params["mod"], "Create");
    }

    #[tokio::test]
    async fn diagnose_reads_the_crash_report() {
        let tmp = tempfile::tempdir().unwrap();
        let reports = tmp.path().join("crash-reports");
        std::fs::create_dir_all(&reports).unwrap();
        std::fs::write(
            reports.join("crash-2026-10-21_12.00.00-client.txt"),
            "Description: Unexpected error\n\njava.lang.OutOfMemoryError: Java heap space\n",
        )
        .unwrap();
        let crash = diagnose(tmp.path(), None, SystemTime::now(), &[], &Redactor::new())
            .await
            .unwrap();
        assert_eq!(crash.cause, CrashCause::OutOfMemory);
    }

    #[test]
    fn every_rule_code_is_a_crash_code() {
        for code in rule_codes() {
            assert!(ErrorCode::ALL.contains(&code));
        }
        assert_eq!(rule_codes().count(), 10);
    }

    #[test]
    fn the_error_names_both_mods() {
        let crash = Crash {
            cause: CrashCause::ModsClash {
                module: ModRef {
                    id: "sodium".into(),
                    name: Some("Sodium".into()),
                    file: Some("sodium.jar".into()),
                },
                other: ModRef {
                    id: "optifabric".into(),
                    name: None,
                    file: None,
                },
            },
            detail: "- Mod 'Sodium' …".into(),
        };
        let error = crash.error();
        assert_eq!(error.code, ErrorCode::CrashModsClash);
        assert_eq!(error.message, "Sodium and optifabric don't work together");
        assert_eq!(error.params["mod"], "Sodium");
        assert_eq!(error.params["other"], "optifabric");
        assert_eq!(error.detail.as_deref(), Some("- Mod 'Sodium' …"));
        assert!(!error.params.values().any(|v| v.ends_with(".jar")));
    }
}

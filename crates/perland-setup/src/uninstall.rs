//! The branded uninstaller (spec §4b, §6.7): how it was started, what it shows, what it deletes.
//! Pure apart from `delete_paths` and `dir_size`, which touch the disk.

use crate::model::{Action, Screen};
use std::path::{Path, PathBuf};

/// Where the launcher keeps credentials in Windows Credential Manager (keyring's service).
pub const CREDENTIAL_SERVICE: &str = "online.perland.launcher";

/// How the uninstaller was started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Tauri's own wizard runs the registry's UninstallString with `_?=<dir>`: pass it through.
    Handoff { dir: PathBuf, args: Vec<String> },
    /// `/S`: remove the app silently, never the data.
    Silent { dir: PathBuf },
    /// Started from the install folder: copy to temp and restart from there.
    Relaunch { dir: PathBuf },
    /// Started from temp with `--from-temp <dir>`: open the window.
    Window { dir: PathBuf },
}

pub fn mode(args: &[String], exe_dir: &Path) -> Mode {
    if let Some(dir) = args.iter().find_map(|a| a.strip_prefix("_?=")) {
        return Mode::Handoff {
            dir: PathBuf::from(dir),
            args: args.to_vec(),
        };
    }
    if args.iter().any(|a| a.eq_ignore_ascii_case("/S")) {
        return Mode::Silent {
            dir: exe_dir.to_path_buf(),
        };
    }
    if let Some(i) = args.iter().position(|a| a == "--from-temp") {
        if let Some(dir) = args.get(i + 1) {
            return Mode::Window {
                dir: PathBuf::from(dir),
            };
        }
    }
    Mode::Relaunch {
        dir: exe_dir.to_path_buf(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UStep {
    RemovingApp,
    DeletingData,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UScreen {
    Confirm,
    Removing {
        step: UStep,
        progress: f32,
    },
    Done {
        kept_data: bool,
    },
    /// `code`: the NSIS uninstaller's exit code (0 when only data failed); `not_deleted`: what
    /// could not be deleted, as shown to the player.
    Failed {
        code: i32,
        not_deleted: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum UEvent {
    Pressed(Action),
    RunningChecked(bool),
    UninstallerExited(i32),
    DataDeleted(Vec<String>),
    DataSize(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UEffect {
    CheckRunning,
    RunUninstaller,
    DeleteData,
    CopyDetails,
    Exit,
}

pub struct UMachine {
    pub screen: Screen,
    pub delete_data: bool,
    pub data_bytes: Option<u64>,
    /// A running-launcher check is out: a second press starts nothing.
    checking: bool,
}

impl UMachine {
    pub fn new() -> UMachine {
        UMachine {
            screen: Screen::Uninstall(UScreen::Confirm),
            delete_data: false,
            data_bytes: None,
            checking: false,
        }
    }

    fn check(&mut self) -> Vec<UEffect> {
        if self.checking {
            return vec![];
        }
        self.checking = true;
        vec![UEffect::CheckRunning]
    }

    fn set(&mut self, screen: UScreen) {
        self.screen = Screen::Uninstall(screen);
    }

    pub fn on(&mut self, event: UEvent) -> Vec<UEffect> {
        use Action::*;
        use UScreen::*;
        match (&self.screen, event) {
            (_, UEvent::DataSize(bytes)) => {
                self.data_bytes = Some(bytes);
                vec![]
            }
            (Screen::Uninstall(Confirm), UEvent::Pressed(ToggleDeleteData)) => {
                self.delete_data = !self.delete_data;
                vec![]
            }
            (Screen::Uninstall(Confirm) | Screen::LauncherRunning, UEvent::Pressed(Primary)) => {
                self.check()
            }
            (_, UEvent::RunningChecked(running)) if self.checking => {
                self.checking = false;
                if running {
                    self.screen = Screen::LauncherRunning;
                    vec![]
                } else {
                    self.set(Removing {
                        step: UStep::RemovingApp,
                        progress: 0.0,
                    });
                    vec![UEffect::RunUninstaller]
                }
            }
            (
                Screen::Uninstall(Removing {
                    step: UStep::RemovingApp,
                    ..
                }),
                UEvent::UninstallerExited(code),
            ) => {
                if code != 0 {
                    self.set(Failed {
                        code,
                        not_deleted: vec![],
                    });
                    vec![]
                } else if self.delete_data {
                    self.set(Removing {
                        step: UStep::DeletingData,
                        progress: 0.6,
                    });
                    vec![UEffect::DeleteData]
                } else {
                    self.set(Done { kept_data: true });
                    vec![]
                }
            }
            (
                Screen::Uninstall(Removing {
                    step: UStep::DeletingData,
                    ..
                }),
                UEvent::DataDeleted(not_deleted),
            ) => {
                if not_deleted.is_empty() {
                    self.set(Done { kept_data: false });
                } else {
                    self.set(Failed {
                        code: 0,
                        not_deleted,
                    });
                }
                vec![]
            }
            (Screen::Uninstall(Failed { .. }), UEvent::Pressed(CopyDetails)) => {
                vec![UEffect::CopyDetails]
            }
            (Screen::Uninstall(Done { .. }), UEvent::Pressed(Primary))
            | (Screen::Uninstall(Confirm), UEvent::Pressed(Secondary)) => vec![UEffect::Exit],
            (
                Screen::Uninstall(Confirm | Done { .. } | Failed { .. }) | Screen::LauncherRunning,
                UEvent::Pressed(Close),
            ) => vec![UEffect::Exit],
            _ => vec![],
        }
    }
}

impl Default for UMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// The player's data: games, worlds and settings, then the webview's two folders.
pub fn data_targets(appdata: &Path, local_appdata: &Path) -> Vec<PathBuf> {
    vec![
        appdata.join("PerlandLauncher"),
        appdata.join("dev.perland.launcher"),
        local_appdata.join("dev.perland.launcher"),
    ]
}

/// Deletes each path (a missing one is fine) and returns those that could not be deleted.
pub fn delete_paths(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter(|p| match std::fs::remove_dir_all(p) {
            Ok(()) => false,
            Err(e) => e.kind() != std::io::ErrorKind::NotFound,
        })
        .map(|p| p.display().to_string())
        .collect()
}

/// Bytes in the files under `path`; links are not followed; missing is 0.
pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// `CredEnumerateW` filter for the launcher's credentials (keyring names them `<user>.<service>`).
pub fn credential_filter(service: &str) -> String {
    format!("*.{service}")
}

/// Arguments for NSIS's `uninstall.exe`, run in place so it waits instead of relaunching.
pub fn uninstaller_args(dir: &Path) -> Vec<String> {
    vec!["/S".into(), format!("_?={}", dir.display())]
}

/// A hidden command that removes the uninstaller's own temp folder a few seconds after it exits.
pub fn cleanup_command(temp_dir: &Path) -> (String, Vec<String>) {
    let script = format!(
        "ping -n 4 127.0.0.1 >nul & rmdir /s /q \"{}\"",
        temp_dir.display()
    );
    ("cmd".into(), vec!["/c".into(), script])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use UEffect::*;
    use UEvent::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn mode_handoff() {
        let exe = Path::new("C:\\PL");
        assert_eq!(
            mode(&s(&["_?=C:\\X"]), exe),
            Mode::Handoff {
                dir: PathBuf::from("C:\\X"),
                args: s(&["_?=C:\\X"])
            }
        );
        assert_eq!(
            mode(&s(&["/P", "_?=C:\\My Folder"]), exe),
            Mode::Handoff {
                dir: PathBuf::from("C:\\My Folder"),
                args: s(&["/P", "_?=C:\\My Folder"])
            }
        );
    }

    #[test]
    fn mode_silent() {
        assert_eq!(
            mode(&s(&["/s"]), Path::new("C:\\PL")),
            Mode::Silent {
                dir: PathBuf::from("C:\\PL")
            }
        );
    }

    #[test]
    fn mode_from_temp_opens_window() {
        assert_eq!(
            mode(
                &s(&["--from-temp", "D:\\Games\\Perland Launcher"]),
                Path::new("C:\\Temp\\x")
            ),
            Mode::Window {
                dir: PathBuf::from("D:\\Games\\Perland Launcher")
            }
        );
    }

    #[test]
    fn mode_default_relaunches() {
        assert_eq!(
            mode(&[], Path::new("C:\\PL")),
            Mode::Relaunch {
                dir: PathBuf::from("C:\\PL")
            }
        );
        // `--from-temp` without a folder is not a window run.
        assert_eq!(
            mode(&s(&["--from-temp"]), Path::new("C:\\PL")),
            Mode::Relaunch {
                dir: PathBuf::from("C:\\PL")
            }
        );
    }

    fn removing(m: &UMachine) -> Option<UStep> {
        match &m.screen {
            Screen::Uninstall(UScreen::Removing { step, .. }) => Some(*step),
            _ => None,
        }
    }

    #[test]
    fn keep_data_path() {
        let mut m = UMachine::new();
        assert_eq!(m.screen, Screen::Uninstall(UScreen::Confirm));
        assert!(!m.delete_data);
        assert_eq!(m.on(Pressed(Action::Primary)), vec![CheckRunning]);
        assert_eq!(m.on(Pressed(Action::Primary)), vec![]);
        assert_eq!(m.on(RunningChecked(false)), vec![RunUninstaller]);
        assert_eq!(removing(&m), Some(UStep::RemovingApp));
        assert_eq!(m.on(UninstallerExited(0)), vec![]);
        assert_eq!(
            m.screen,
            Screen::Uninstall(UScreen::Done { kept_data: true })
        );
        assert_eq!(m.on(Pressed(Action::Primary)), vec![Exit]);
    }

    #[test]
    fn delete_data_path() {
        let mut m = UMachine::new();
        m.on(Pressed(Action::ToggleDeleteData));
        assert!(m.delete_data);
        m.on(Pressed(Action::Primary));
        m.on(RunningChecked(false));
        assert_eq!(m.on(UninstallerExited(0)), vec![DeleteData]);
        assert_eq!(removing(&m), Some(UStep::DeletingData));
        assert_eq!(m.on(DataDeleted(vec![])), vec![]);
        assert_eq!(
            m.screen,
            Screen::Uninstall(UScreen::Done { kept_data: false })
        );
    }

    #[test]
    fn toggle_twice_keeps_data() {
        let mut m = UMachine::new();
        m.on(Pressed(Action::ToggleDeleteData));
        m.on(Pressed(Action::ToggleDeleteData));
        assert!(!m.delete_data);
    }

    #[test]
    fn uninstaller_failure() {
        let mut m = UMachine::new();
        m.on(Pressed(Action::ToggleDeleteData));
        m.on(Pressed(Action::Primary));
        m.on(RunningChecked(false));
        assert_eq!(m.on(UninstallerExited(2)), vec![]);
        assert_eq!(
            m.screen,
            Screen::Uninstall(UScreen::Failed {
                code: 2,
                not_deleted: vec![]
            })
        );
        assert_eq!(m.on(Pressed(Action::CopyDetails)), vec![CopyDetails]);
        assert_eq!(m.on(Pressed(Action::Close)), vec![Exit]);
    }

    #[test]
    fn data_not_deleted_is_reported() {
        let mut m = UMachine::new();
        m.on(Pressed(Action::ToggleDeleteData));
        m.on(Pressed(Action::Primary));
        m.on(RunningChecked(false));
        m.on(UninstallerExited(0));
        m.on(DataDeleted(s(&[
            "C:\\Users\\a\\AppData\\Roaming\\PerlandLauncher",
        ])));
        assert_eq!(
            m.screen,
            Screen::Uninstall(UScreen::Failed {
                code: 0,
                not_deleted: s(&["C:\\Users\\a\\AppData\\Roaming\\PerlandLauncher"])
            })
        );
    }

    #[test]
    fn running_launcher_blocks_uninstall() {
        let mut m = UMachine::new();
        m.on(Pressed(Action::Primary));
        assert_eq!(m.on(RunningChecked(true)), vec![]);
        assert_eq!(m.screen, Screen::LauncherRunning);
        assert_eq!(m.on(Pressed(Action::Primary)), vec![CheckRunning]);
        assert_eq!(m.on(RunningChecked(false)), vec![RunUninstaller]);
    }

    #[test]
    fn close_while_removing_does_nothing() {
        let mut m = UMachine::new();
        m.on(Pressed(Action::Primary));
        m.on(RunningChecked(false));
        assert_eq!(m.on(Pressed(Action::Close)), vec![]);
        assert_eq!(removing(&m), Some(UStep::RemovingApp));
    }

    #[test]
    fn cancel_on_confirm_exits() {
        let mut m = UMachine::new();
        assert_eq!(m.on(Pressed(Action::Secondary)), vec![Exit]);
        let mut m = UMachine::new();
        assert_eq!(m.on(Pressed(Action::Close)), vec![Exit]);
    }

    #[test]
    fn data_size_is_kept() {
        let mut m = UMachine::new();
        m.on(DataSize(3_200_000_000));
        assert_eq!(m.data_bytes, Some(3_200_000_000));
    }

    #[test]
    fn data_targets_names_three_folders() {
        let (a, l) = (Path::new("R"), Path::new("L"));
        assert_eq!(
            data_targets(a, l),
            vec![
                a.join("PerlandLauncher"),
                a.join("dev.perland.launcher"),
                l.join("dev.perland.launcher")
            ]
        );
    }

    #[test]
    fn delete_paths_missing_is_fine() {
        let t = tempfile::tempdir().unwrap();
        let there = t.path().join("PerlandLauncher");
        fs::create_dir_all(there.join("instances")).unwrap();
        fs::write(there.join("instances").join("world.dat"), b"x").unwrap();
        let missing = t.path().join("dev.perland.launcher");
        assert_eq!(
            delete_paths(&[there.clone(), missing]),
            Vec::<String>::new()
        );
        assert!(!there.exists());
    }

    #[test]
    fn dir_size_counts_files() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("a"), b"abc").unwrap();
        fs::create_dir(t.path().join("sub")).unwrap();
        fs::write(t.path().join("sub").join("b"), b"defg").unwrap();
        assert_eq!(dir_size(t.path()), 7);
        assert_eq!(dir_size(&t.path().join("missing")), 0);
    }

    #[test]
    fn credential_filter_is_the_service() {
        assert_eq!(
            credential_filter(CREDENTIAL_SERVICE),
            "*.online.perland.launcher"
        );
    }

    #[test]
    fn uninstaller_args_in_place() {
        assert_eq!(
            uninstaller_args(Path::new("D:\\My Games\\PL")),
            s(&["/S", "_?=D:\\My Games\\PL"])
        );
    }

    #[test]
    fn cleanup_command_quotes_the_folder() {
        let (program, args) = cleanup_command(Path::new("C:\\Temp\\perland uninstall"));
        assert_eq!(program, "cmd");
        assert_eq!(args[0], "/c");
        assert!(
            args[1].contains("rmdir /s /q \"C:\\Temp\\perland uninstall\""),
            "{}",
            args[1]
        );
        assert!(args[1].starts_with("ping -n 4 127.0.0.1"));
    }
}

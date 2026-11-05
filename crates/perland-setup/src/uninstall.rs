//! The branded uninstaller (spec §4b, §6.7): how it was started, what it shows, what it deletes.
//! Pure apart from `delete_paths` and `dir_size`, which touch the disk.

use crate::model::{Action, Screen};
use std::path::{Path, PathBuf};

/// Where the launcher keeps credentials in Windows Credential Manager (keyring's service).
pub const CREDENTIAL_SERVICE: &str = "online.perland.launcher";

/// How the uninstaller was started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Tauri's own wizard runs the registry's UninstallString with `_?=<dir>`: run the
    /// `uninstall.exe` in `dir` (our own folder) with `args`, the raw command line as it came.
    Handoff { dir: PathBuf, args: String },
    /// `/S`: remove the app silently, never the data.
    Silent { dir: PathBuf },
    /// Started from the install folder: copy to temp and restart from there.
    Relaunch { dir: PathBuf },
    /// Started from temp with `--from-temp <dir>`: open the window.
    Window { dir: PathBuf },
}

/// `args` are the parsed arguments, `tail` the raw command line after the program name
/// ([`command_line_tail`]): Tauri's wizard passes `_?=<dir>` unquoted, so a folder with a space
/// arrives split across arguments and only the raw text keeps it whole.
pub fn mode(args: &[String], tail: &str, exe_dir: &Path) -> Mode {
    let starts_handoff = tail.starts_with("_?=") || tail.contains(" _?=") || tail.contains("\t_?=");
    if starts_handoff && !args.iter().any(|a| a == "--from-temp") {
        return Mode::Handoff {
            dir: exe_dir.to_path_buf(),
            args: tail.to_string(),
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
                        progress: 0.35,
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
                        progress: 0.75,
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
            (Screen::Uninstall(Done { .. } | Failed { .. }), UEvent::Pressed(Primary))
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
    // A root Windows could not name would make these relative to the working directory.
    let mut targets = Vec::new();
    if appdata.is_absolute() {
        targets.push(appdata.join("PerlandLauncher"));
        targets.push(appdata.join("dev.perland.launcher"));
    }
    if local_appdata.is_absolute() {
        targets.push(local_appdata.join("dev.perland.launcher"));
    }
    targets
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
/// Where Tauri's NSIS installer remembers the install folder: `Software\<manufacturer>\<product>`
/// under HKCU, the manufacturer being the identifier's second part (`dev.perland.launcher`).
/// Deleted with the player's data, as Tauri's own "delete app data" does.
pub const REMEMBERED_FOLDER_KEY: &str = "Software\\perland\\Perland Launcher";

/// Whether a Credential Manager entry is one of the launcher's. The keyring crate names a
/// Windows credential `<user>.<service>`, so ours end in `.online.perland.launcher`.
/// `CredEnumerateW` only takes a prefix filter, so the uninstaller lists every entry and keeps
/// these.
pub fn is_our_credential(target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    t == CREDENTIAL_SERVICE || t.ends_with(&format!(".{CREDENTIAL_SERVICE}"))
}

/// The raw command line after the program name, as Windows hands it over: the program is a
/// quoted string or runs to the first space or tab.
pub fn command_line_tail(full: &str) -> &str {
    let rest = if let Some(after) = full.strip_prefix('"') {
        match after.find('"') {
            Some(i) => &after[i + 1..],
            None => "",
        }
    } else {
        match full.find([' ', '\t']) {
            Some(i) => &full[i..],
            None => "",
        }
    };
    rest.trim_start_matches([' ', '\t'])
}

/// NSIS's `uninstall.exe` command line, run in place so it waits instead of relaunching.
/// Passed as is (`raw_arg`): NSIS only reads `_?=` after a space, and runs to the end of the
/// line, so it must not be quoted.
pub fn uninstaller_command_line(dir: &Path) -> String {
    format!("/S _?={}", dir.display())
}

/// Whether `dir` is a folder the setup or the uninstaller made for itself: directly in `temp`
/// and named `perland-setup-<16 lowercase hex>`. The only folder the cleanup may delete.
pub fn is_own_temp(dir: &Path, temp: &Path) -> bool {
    if dir.parent() != Some(temp) {
        return false;
    }
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.strip_prefix("perland-setup-").is_some_and(|hex| {
        hex.len() == 16
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// `cmd`'s command line (passed as is, `raw_arg`) that removes the uninstaller's temp folder a
/// few seconds after it exits. Only for a folder [`is_own_temp`] accepts.
pub fn cleanup_command_line(temp_dir: &Path) -> String {
    format!(
        "/c ping -n 4 127.0.0.1 >nul & rmdir /s /q \"{}\"",
        temp_dir.display()
    )
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
        let exe = Path::new("C:\\Users\\ana\\AppData\\Local\\Perland Launcher");
        // Tauri's wizard passes `_?=` unquoted, so a folder with a space splits in two.
        let tail = "/UPDATE /P _?=C:\\Users\\ana\\AppData\\Local\\Perland Launcher";
        assert_eq!(
            mode(
                &s(&[
                    "/UPDATE",
                    "/P",
                    "_?=C:\\Users\\ana\\AppData\\Local\\Perland",
                    "Launcher"
                ]),
                tail,
                exe
            ),
            Mode::Handoff {
                dir: exe.to_path_buf(),
                args: tail.to_string()
            }
        );
        assert_eq!(
            mode(&s(&["_?=C:\\X"]), "_?=C:\\X", exe),
            Mode::Handoff {
                dir: exe.to_path_buf(),
                args: "_?=C:\\X".to_string()
            }
        );
    }

    #[test]
    fn handoff_needs_the_marker_at_an_argument_start() {
        assert_eq!(
            mode(
                &s(&["--from-temp", "D:\\a_?=b"]),
                "--from-temp \"D:\\a_?=b\"",
                Path::new("C:\\Temp\\x")
            ),
            Mode::Window {
                dir: PathBuf::from("D:\\a_?=b")
            }
        );
    }

    #[test]
    fn tail_skips_the_program() {
        assert_eq!(
            command_line_tail(
                "\"C:\\Perland Launcher\\perland-uninstall.exe\" /P _?=C:\\Perland Launcher"
            ),
            "/P _?=C:\\Perland Launcher"
        );
        assert_eq!(
            command_line_tail("C:\\PL\\perland-uninstall.exe   /S"),
            "/S"
        );
        assert_eq!(command_line_tail("perland-uninstall.exe"), "");
        assert_eq!(command_line_tail("\"C:\\a b\\x.exe\""), "");
        assert_eq!(command_line_tail(""), "");
    }

    #[test]
    fn mode_silent() {
        assert_eq!(
            mode(&s(&["/s"]), "/s", Path::new("C:\\PL")),
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
                "--from-temp \"D:\\Games\\Perland Launcher\"",
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
            mode(&[], "", Path::new("C:\\PL")),
            Mode::Relaunch {
                dir: PathBuf::from("C:\\PL")
            }
        );
        // `--from-temp` without a folder is not a window run.
        assert_eq!(
            mode(&s(&["--from-temp"]), "--from-temp", Path::new("C:\\PL")),
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
    fn failed_close_button_exits() {
        let mut m = UMachine::new();
        m.on(Pressed(Action::Primary));
        m.on(RunningChecked(false));
        m.on(UninstallerExited(5));
        assert_eq!(m.on(Pressed(Action::Primary)), vec![Exit]);
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
        let base = std::env::temp_dir();
        let (a, l) = (base.join("R"), base.join("L"));
        assert_eq!(
            data_targets(&a, &l),
            vec![
                a.join("PerlandLauncher"),
                a.join("dev.perland.launcher"),
                l.join("dev.perland.launcher")
            ]
        );
    }

    #[test]
    fn data_targets_never_relative() {
        let a = std::env::temp_dir().join("R");
        assert_eq!(
            data_targets(&a, Path::new("")),
            vec![a.join("PerlandLauncher"), a.join("dev.perland.launcher")]
        );
        assert!(data_targets(Path::new("R"), Path::new("L")).is_empty());
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
    fn our_credentials_only() {
        assert!(is_our_credential("ana@example.com.online.perland.launcher"));
        assert!(is_our_credential("SESSION.Online.Perland.Launcher"));
        assert!(is_our_credential("online.perland.launcher"));
        assert!(!is_our_credential("notonline.perland.launcher"));
        assert!(!is_our_credential("ana.online.perland.launcher.evil"));
        assert!(!is_our_credential("git:https://github.com"));
        assert!(!is_our_credential(""));
    }

    #[test]
    fn uninstaller_runs_in_place_unquoted() {
        assert_eq!(
            uninstaller_command_line(Path::new("D:\\My Games\\PL")),
            "/S _?=D:\\My Games\\PL"
        );
    }

    #[test]
    fn own_temp_is_a_setup_folder_directly_in_temp() {
        let temp = Path::new("/t");
        assert!(is_own_temp(
            &temp.join("perland-setup-0123456789abcdef"),
            temp
        ));
        assert!(!is_own_temp(
            &temp.join("perland-setup-0123456789ABCDEF"),
            temp
        ));
        assert!(!is_own_temp(&temp.join("perland-setup-0123"), temp));
        assert!(!is_own_temp(&temp.join("other-0123456789abcdef"), temp));
        assert!(!is_own_temp(
            &temp.join("x").join("perland-setup-0123456789abcdef"),
            temp
        ));
        assert!(!is_own_temp(temp, temp));
        assert!(!is_own_temp(Path::new("/"), temp));
    }

    #[test]
    fn cleanup_line_quotes_the_folder_for_cmd() {
        let line = cleanup_command_line(Path::new("C:\\Temp\\perland-setup-0123456789abcdef"));
        assert_eq!(
            line,
            "/c ping -n 4 127.0.0.1 >nul & rmdir /s /q \"C:\\Temp\\perland-setup-0123456789abcdef\""
        );
    }
}

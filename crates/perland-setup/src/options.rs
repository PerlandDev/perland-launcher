//! Install options: which folder, which shortcuts, and the arguments that tell Tauri's NSIS
//! installer about them (spec §4a, §6.4a). Pure apart from the folder checks, which touch the disk.

use std::path::{Path, PathBuf};

/// The launcher's exe; a folder holding it already holds Perland Launcher.
pub const LAUNCHER_EXE: &str = "perland-launcher.exe";
/// The folder added inside a picked folder that holds other things.
pub const SUBFOLDER: &str = "Perland Launcher";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// `None`: the default folder (`%LOCALAPPDATA%\Perland Launcher`).
    pub folder: Option<PathBuf>,
    pub start_menu: bool,
    pub desktop: bool,
}

impl Default for Choice {
    fn default() -> Self {
        Choice {
            folder: None,
            start_menu: true,
            desktop: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderProblem {
    NotWritable,
    NeedsAdmin,
    NotEmpty,
}

impl FolderProblem {
    pub fn code(&self) -> &'static str {
        match self {
            FolderProblem::NotWritable => "not_writable",
            FolderProblem::NeedsAdmin => "needs_admin",
            FolderProblem::NotEmpty => "not_empty",
        }
    }
}

/// The folder the install goes to for a picked folder, or why it can't go there.
pub fn resolve_folder(picked: &Path, protected: &[PathBuf]) -> Result<PathBuf, FolderProblem> {
    if protected
        .iter()
        .any(|root| starts_with_ignore_case(picked, root))
    {
        return Err(FolderProblem::NeedsAdmin);
    }
    let target = if usable_as_is(picked) {
        picked.to_path_buf()
    } else {
        let sub = picked.join(SUBFOLDER);
        if !usable_as_is(&sub) {
            return Err(FolderProblem::NotEmpty);
        }
        sub
    };
    if !writable(&target) {
        return Err(FolderProblem::NotWritable);
    }
    Ok(target)
}

/// Missing, empty, or already holding the launcher.
fn usable_as_is(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Err(_) => !dir.exists(),
        Ok(mut entries) => entries.next().is_none() || dir.join(LAUNCHER_EXE).is_file(),
    }
}

fn starts_with_ignore_case(path: &Path, root: &Path) -> bool {
    let lower = |p: &Path| -> Vec<String> {
        p.components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect()
    };
    let (path, root) = (lower(path), lower(root));
    !root.is_empty() && path.len() >= root.len() && path[..root.len()] == root[..]
}

/// Whether this user can write where the folder is (or will be): a test file in its nearest
/// existing folder, removed again.
fn writable(target: &Path) -> bool {
    let Some(existing) = target.ancestors().find(|a| a.is_dir()) else {
        return false;
    };
    let probe = existing.join(".perland-write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Arguments for the NSIS installer. `/D=` must be last and unquoted (NSIS's rule).
pub fn installer_args(choice: &Choice, fresh: bool) -> Vec<String> {
    let mut args = vec!["/S".to_string()];
    if !fresh {
        args.push("/UPDATE".into());
        return args;
    }
    if !choice.start_menu && !choice.desktop {
        args.push("/NS".into());
    }
    if let Some(folder) = &choice.folder {
        args.push(format!("/D={}", folder.display()));
    }
    args
}

/// Shortcuts to delete after a fresh install where exactly one of the two was unticked
/// (both unticked is `/NS`, which creates neither).
pub fn shortcuts_to_remove(
    choice: &Choice,
    fresh: bool,
    start_menu: &Path,
    desktop: &Path,
) -> Vec<PathBuf> {
    match (fresh, choice.start_menu, choice.desktop) {
        (true, true, false) => vec![desktop.to_path_buf()],
        (true, false, true) => vec![start_menu.to_path_buf()],
        _ => vec![],
    }
}

/// The launcher's installed size, measured by the release build.
pub fn installed_bytes() -> Option<u64> {
    option_env!("PERLAND_SETUP_INSTALLED_BYTES").and_then(|v| v.trim().parse().ok())
}

/// "25 MB": whole megabytes (1 000 000 bytes), rounded up, at least 1.
pub fn size_text(bytes: u64) -> String {
    format!("{} MB", bytes.div_ceil(1_000_000).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn empty_folder_used_as_is() {
        let t = tmp();
        let dir = t.path().join("Empty");
        fs::create_dir(&dir).unwrap();
        assert_eq!(resolve_folder(&dir, &[]), Ok(dir));
    }

    #[test]
    fn new_folder_used_as_is() {
        let t = tmp();
        let dir = t.path().join("New").join("Deeper");
        assert_eq!(resolve_folder(&dir, &[]), Ok(dir));
    }

    #[test]
    fn folder_with_launcher_used_as_is() {
        let t = tmp();
        fs::write(t.path().join(LAUNCHER_EXE), b"MZ").unwrap();
        fs::write(t.path().join("other.dll"), b"x").unwrap();
        assert_eq!(resolve_folder(t.path(), &[]), Ok(t.path().to_path_buf()));
    }

    #[test]
    fn other_folder_gets_subfolder() {
        let t = tmp();
        let games = t.path().join("Games");
        fs::create_dir(&games).unwrap();
        fs::write(games.join("notes.txt"), b"x").unwrap();
        assert_eq!(resolve_folder(&games, &[]), Ok(games.join(SUBFOLDER)));
    }

    #[test]
    fn drive_like_root_gets_subfolder() {
        let t = tmp();
        fs::create_dir(t.path().join("Users")).unwrap();
        assert_eq!(resolve_folder(t.path(), &[]), Ok(t.path().join(SUBFOLDER)));
    }

    #[test]
    fn subfolder_with_other_files_is_refused() {
        let t = tmp();
        fs::write(t.path().join("a.txt"), b"x").unwrap();
        fs::create_dir(t.path().join(SUBFOLDER)).unwrap();
        fs::write(t.path().join(SUBFOLDER).join("photo.png"), b"x").unwrap();
        assert_eq!(resolve_folder(t.path(), &[]), Err(FolderProblem::NotEmpty));
    }

    #[test]
    fn subfolder_with_launcher_is_fine() {
        let t = tmp();
        fs::write(t.path().join("a.txt"), b"x").unwrap();
        fs::create_dir(t.path().join(SUBFOLDER)).unwrap();
        fs::write(t.path().join(SUBFOLDER).join(LAUNCHER_EXE), b"MZ").unwrap();
        assert_eq!(resolve_folder(t.path(), &[]), Ok(t.path().join(SUBFOLDER)));
    }

    #[test]
    fn protected_root_needs_admin() {
        let t = tmp();
        let protected = vec![t.path().join("ProgramFiles")];
        let picked = t.path().join("programfiles").join("X");
        assert_eq!(
            resolve_folder(&picked, &protected),
            Err(FolderProblem::NeedsAdmin)
        );
        assert_eq!(
            resolve_folder(&t.path().join("ProgramFiles"), &protected),
            Err(FolderProblem::NeedsAdmin)
        );
        let beside = t.path().join("ProgramFilesExtra");
        assert_eq!(resolve_folder(&beside, &protected), Ok(beside));
    }

    #[cfg(unix)]
    #[test]
    fn unwritable_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let t = tmp();
        let locked = t.path().join("Locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        // Root writes anywhere; the check is meaningful only for a normal user.
        if fs::write(locked.join("probe"), b"x").is_ok() {
            let _ = fs::remove_file(locked.join("probe"));
            return;
        }
        assert_eq!(
            resolve_folder(&locked.join("New"), &[]),
            Err(FolderProblem::NotWritable)
        );
    }

    #[test]
    fn no_test_file_left_behind() {
        let t = tmp();
        let dir = t.path().join("Empty");
        fs::create_dir(&dir).unwrap();
        resolve_folder(&dir, &[]).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn args_default_install() {
        assert_eq!(installer_args(&Choice::default(), true), vec!["/S"]);
    }

    #[test]
    fn args_custom_folder_last_unquoted() {
        let choice = Choice {
            folder: Some(PathBuf::from("D:\\My Games\\Perland Launcher")),
            ..Choice::default()
        };
        assert_eq!(
            installer_args(&choice, true),
            vec!["/S", "/D=D:\\My Games\\Perland Launcher"]
        );
    }

    #[test]
    fn args_no_shortcuts() {
        let none = Choice {
            start_menu: false,
            desktop: false,
            ..Choice::default()
        };
        assert_eq!(installer_args(&none, true), vec!["/S", "/NS"]);
        let both = Choice {
            folder: Some(PathBuf::from("E:\\PL")),
            ..none
        };
        assert_eq!(installer_args(&both, true), vec!["/S", "/NS", "/D=E:\\PL"]);
        let one = Choice {
            desktop: false,
            ..Choice::default()
        };
        assert_eq!(installer_args(&one, true), vec!["/S"]);
    }

    #[test]
    fn args_update() {
        let choice = Choice {
            folder: Some(PathBuf::from("E:\\PL")),
            start_menu: false,
            desktop: false,
        };
        assert_eq!(installer_args(&choice, false), vec!["/S", "/UPDATE"]);
    }

    #[test]
    fn remove_one_shortcut() {
        let (sm, dt) = (Path::new("sm.lnk"), Path::new("dt.lnk"));
        let c = |start_menu, desktop| Choice {
            start_menu,
            desktop,
            ..Choice::default()
        };
        assert_eq!(
            shortcuts_to_remove(&c(true, false), true, sm, dt),
            vec![dt.to_path_buf()]
        );
        assert_eq!(
            shortcuts_to_remove(&c(false, true), true, sm, dt),
            vec![sm.to_path_buf()]
        );
        assert!(shortcuts_to_remove(&c(false, false), true, sm, dt).is_empty());
        assert!(shortcuts_to_remove(&c(true, true), true, sm, dt).is_empty());
        assert!(shortcuts_to_remove(&c(true, false), false, sm, dt).is_empty());
    }

    #[test]
    fn size_text_rounds_up() {
        assert_eq!(size_text(25_000_001), "26 MB");
        assert_eq!(size_text(25_000_000), "25 MB");
        assert_eq!(size_text(100), "1 MB");
        assert_eq!(size_text(0), "1 MB");
    }

    #[test]
    fn problem_codes_are_stable() {
        assert_eq!(FolderProblem::NotWritable.code(), "not_writable");
        assert_eq!(FolderProblem::NeedsAdmin.code(), "needs_admin");
        assert_eq!(FolderProblem::NotEmpty.code(), "not_empty");
    }
}

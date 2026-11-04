//! The NSIS installer inside the setup app, unpacked to a fresh temp folder for one run.

use std::io;
use std::path::{Path, PathBuf};

/// The embedded NSIS installer; `None` in builds made without `PERLAND_SETUP_PAYLOAD`.
pub fn embedded() -> Option<&'static [u8]> {
    #[cfg(has_payload)]
    {
        Some(include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin")))
    }
    #[cfg(not(has_payload))]
    {
        None
    }
}

/// A temp folder holding the unpacked installer. Dropping it removes the folder.
pub struct PayloadDir {
    dir: PathBuf,
    exe: PathBuf,
}

impl PayloadDir {
    pub fn write(bytes: &[u8], temp_root: &Path) -> io::Result<PayloadDir> {
        Self::write_named(bytes, temp_root, "PerlandLauncherSetup.exe")
    }

    pub fn write_named(bytes: &[u8], temp_root: &Path, name: &str) -> io::Result<PayloadDir> {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
        let hex: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let dir = temp_root.join(format!("perland-setup-{hex}"));
        std::fs::create_dir(&dir)?;
        // From here on, dropping `payload` removes the folder, also when the write fails.
        let payload = PayloadDir {
            exe: dir.join(name),
            dir,
        };
        std::fs::write(&payload.exe, bytes)?;
        Ok(payload)
    }

    pub fn exe(&self) -> &Path {
        &self.exe
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Removes the folder now, retrying for about a second (an antivirus may still hold the
    /// just-run installer), and says whether it worked.
    pub fn remove(self) -> io::Result<()> {
        let mut last = Ok(());
        for attempt in 0..5 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            last = match std::fs::remove_dir_all(&self.dir) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            };
            if last.is_ok() {
                break;
            }
        }
        last
    }
}

/// The installed launcher from its registry values, only while its exe is on disk: a folder
/// deleted by hand leaves the uninstall entry behind, and that is not an install.
pub fn installed_entry(
    version: Option<String>,
    location: Option<String>,
    binary: Option<String>,
    exists: impl Fn(&Path) -> bool,
) -> Option<(String, PathBuf)> {
    let (version, location) = (version?, location?);
    let exe = installed_exe(&location, binary.as_deref());
    exists(&exe).then_some((version, exe))
}

/// The launcher's exe from the registry's `InstallLocation` (Tauri writes it in quotes) and
/// `MainBinaryName`.
pub fn installed_exe(install_location: &str, main_binary: Option<&str>) -> PathBuf {
    let folder = install_location
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/']);
    let binary = main_binary
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .unwrap_or("perland-launcher.exe");
    Path::new(folder).join(binary)
}

impl Drop for PayloadDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_creates_exe_and_drop_removes_dir() {
        let root = tempfile::tempdir().unwrap();
        let payload = PayloadDir::write(b"MZ fake installer", root.path()).unwrap();
        assert_eq!(std::fs::read(payload.exe()).unwrap(), b"MZ fake installer");
        assert!(payload.exe().starts_with(root.path()));
        let name = payload
            .dir()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            name.starts_with("perland-setup-") && name.len() == "perland-setup-".len() + 16,
            "{name}"
        );
        assert_eq!(
            payload.exe().file_name().unwrap(),
            "PerlandLauncherSetup.exe"
        );
        let dir = payload.dir().to_path_buf();
        drop(payload);
        assert!(!dir.exists());
    }

    #[test]
    fn write_named_uses_the_name() {
        let root = tempfile::tempdir().unwrap();
        let copy = PayloadDir::write_named(b"MZ", root.path(), "perland-uninstall.exe").unwrap();
        assert_eq!(copy.exe().file_name().unwrap(), "perland-uninstall.exe");
    }

    #[test]
    fn two_writes_get_different_dirs() {
        let root = tempfile::tempdir().unwrap();
        let a = PayloadDir::write(b"a", root.path()).unwrap();
        let b = PayloadDir::write(b"b", root.path()).unwrap();
        assert_ne!(a.dir(), b.dir());
    }

    #[test]
    fn write_into_missing_root_fails() {
        let root = tempfile::tempdir().unwrap();
        assert!(PayloadDir::write(b"a", &root.path().join("nope/deeper")).is_err());
    }

    #[test]
    fn installed_exe_cases() {
        let want = Path::new("C:\\Users\\a\\AppData\\Local\\Perland Launcher")
            .join("perland-launcher.exe");
        assert_eq!(
            installed_exe("\"C:\\Users\\a\\AppData\\Local\\Perland Launcher\"", None),
            want
        );
        assert_eq!(
            installed_exe("C:\\Users\\a\\AppData\\Local\\Perland Launcher", None),
            want
        );
        assert_eq!(
            installed_exe("\"C:\\Users\\a\\AppData\\Local\\Perland Launcher\\\"", None),
            want
        );
        assert_eq!(
            installed_exe(
                "  \"C:\\Users\\a\\AppData\\Local\\Perland Launcher\"  ",
                Some("")
            ),
            want
        );
        assert_eq!(
            installed_exe("C:\\x", Some("other.exe")),
            Path::new("C:\\x").join("other.exe")
        );
    }

    #[test]
    fn remove_deletes_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let payload = PayloadDir::write(b"a", root.path()).unwrap();
        let dir = payload.dir().to_path_buf();
        payload.remove().unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn installed_entry_needs_the_exe() {
        let entry = |exists: bool| {
            installed_entry(Some("0.2.0".into()), Some("\"C:\\x\"".into()), None, |_| {
                exists
            })
        };
        assert_eq!(
            entry(true),
            Some((
                "0.2.0".into(),
                Path::new("C:\\x").join("perland-launcher.exe")
            ))
        );
        assert_eq!(entry(false), None);
        assert_eq!(
            installed_entry(None, Some("C:\\x".into()), None, |_| true),
            None
        );
        assert_eq!(
            installed_entry(Some("0.2.0".into()), None, None, |_| true),
            None
        );
    }

    #[cfg(not(has_payload))]
    #[test]
    fn no_payload_without_the_build_variable() {
        assert!(embedded().is_none());
    }

    #[cfg(has_payload)]
    #[test]
    fn payload_is_embedded_with_the_build_variable() {
        assert!(!embedded().unwrap().is_empty());
    }
}

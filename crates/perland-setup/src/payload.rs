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
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
        let name: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let dir = temp_root.join(format!("perland-setup-{name}"));
        std::fs::create_dir(&dir)?;
        // From here on, dropping `payload` removes the folder, also when the write fails.
        let payload = PayloadDir {
            exe: dir.join("PerlandLauncherSetup.exe"),
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

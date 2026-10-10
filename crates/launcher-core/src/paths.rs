use std::path::{Path, PathBuf};

/// The launcher's data folder and the shared game-file folders inside it.
#[derive(Debug, Clone)]
pub struct LauncherDirs {
    root: PathBuf,
}

impl LauncherDirs {
    /// A relative `root` is made absolute against the current directory: the game runs with its
    /// instance folder as working directory, so every path handed to Java must be absolute.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let root = std::path::absolute(&root).unwrap_or(root);
        Self { root }
    }

    /// `%APPDATA%\PerlandLauncher` on Windows; `$XDG_DATA_HOME/PerlandLauncher` or `~/.local/share/PerlandLauncher` elsewhere.
    pub fn default_root() -> Option<PathBuf> {
        if cfg!(windows) {
            std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("PerlandLauncher"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
                .map(|p| p.join("PerlandLauncher"))
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn java(&self) -> PathBuf {
        self.root.join("java")
    }
    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }
    pub fn meta(&self) -> PathBuf {
        self.root.join("meta")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(id)
    }
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }
    pub fn client_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }
    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("natives")
    }
    pub fn log_config(&self, file_id: &str) -> PathBuf {
        self.assets().join("log_configs").join(file_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_under_root() {
        let d = LauncherDirs::new("/data/PerlandLauncher");
        let root = d.root().to_path_buf();
        assert_eq!(
            d.client_jar("1.20.1"),
            root.join("versions/1.20.1/1.20.1.jar")
        );
        assert_eq!(
            d.version_json("fabric-loader-0.16.14-1.20.1"),
            root.join("versions/fabric-loader-0.16.14-1.20.1/fabric-loader-0.16.14-1.20.1.json")
        );
        assert_eq!(
            d.natives_dir("1.12.2"),
            root.join("versions/1.12.2/natives")
        );
        assert_eq!(
            d.log_config("client-1.12.xml"),
            root.join("assets/log_configs/client-1.12.xml")
        );
        assert!(d.instances().starts_with(d.root()));
    }

    #[test]
    fn relative_root_becomes_absolute() {
        // The game runs with its instance folder as the working directory, so every path the
        // launcher hands to Java must be absolute; a relative `--root` broke Fabric launches.
        let d = LauncherDirs::new("target/pl-smoke");
        assert!(d.root().is_absolute(), "{}", d.root().display());
        assert!(d.root().ends_with("target/pl-smoke"));
        assert!(d.libraries().is_absolute());
    }
}

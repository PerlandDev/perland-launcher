//! Opening a folder in the file manager on Linux.
//!
//! Handing a folder to `xdg-open` lets the desktop's MIME defaults pick the program. On some
//! desktops (Linux Mint, from an AppImage or a `.deb`) that picks the web browser, which shows
//! the folder as a `file://` page. Asking the file manager itself over D-Bus
//! (`org.freedesktop.FileManager1`, which Nemo, Nautilus, Dolphin, Thunar and Caja provide)
//! never goes through MIME types. `xdg-open` stays the fallback when no file manager answers.

use std::path::Path;

/// The D-Bus service, object, interface and method that open folders in the file manager.
pub const FILE_MANAGER_SERVICE: &str = "org.freedesktop.FileManager1";
pub const FILE_MANAGER_PATH: &str = "/org/freedesktop/FileManager1";
pub const FILE_MANAGER_INTERFACE: &str = "org.freedesktop.FileManager1";
pub const SHOW_FOLDERS: &str = "ShowFolders";

/// The `file://` URI `ShowFolders` takes for `path`, or `None` for a relative path.
pub fn folder_uri(path: &Path) -> Option<String> {
    url::Url::from_file_path(path).ok().map(String::from)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn uri_of_the_data_folder() {
        assert_eq!(
            folder_uri(Path::new("/home/test/.local/share/PerlandLauncher")).as_deref(),
            Some("file:///home/test/.local/share/PerlandLauncher")
        );
    }

    #[test]
    fn uri_escapes_spaces_and_accents() {
        assert_eq!(
            folder_uri(Path::new("/home/test/Mis juegos/año #1")).as_deref(),
            Some("file:///home/test/Mis%20juegos/a%C3%B1o%20%231")
        );
    }

    #[test]
    fn uri_of_the_root_and_relative_paths() {
        assert_eq!(folder_uri(Path::new("/")).as_deref(), Some("file:///"));
        assert_eq!(folder_uri(Path::new("instances/x")), None);
    }
}

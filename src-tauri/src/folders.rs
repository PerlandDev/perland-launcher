//! Opens a folder in the file manager over D-Bus (see `launcher_app::folders`).

use launcher_app::folders::{
    folder_uri, FILE_MANAGER_INTERFACE, FILE_MANAGER_PATH, FILE_MANAGER_SERVICE, SHOW_FOLDERS,
};
use std::path::Path;
use std::time::Duration;

/// Asks the desktop's file manager to show `path`. An error means no file manager answered.
pub fn show_folder(path: &Path) -> Result<(), String> {
    let uri = folder_uri(path).ok_or("not an absolute path")?;
    let connection = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.method_timeout(Duration::from_secs(10)).build())
        .map_err(|e| e.to_string())?;
    connection
        .call_method(
            Some(FILE_MANAGER_SERVICE),
            FILE_MANAGER_PATH,
            Some(FILE_MANAGER_INTERFACE),
            SHOW_FOLDERS,
            &(vec![uri.as_str()], ""),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#![cfg_attr(windows, windows_subsystem = "windows")]

//! Perland Launcher's branded uninstaller (design §4b, §6.7).

fn main() {
    #[cfg(windows)]
    std::process::exit(perland_setup::win::run_uninstall());
    #[cfg(not(windows))]
    {
        eprintln!("Perland Launcher's uninstaller runs on Windows.");
        std::process::exit(1);
    }
}

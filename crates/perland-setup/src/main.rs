#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    std::process::exit(perland_setup::win::run());
    #[cfg(not(windows))]
    {
        eprintln!("Perland Launcher Setup runs on Windows.");
        std::process::exit(1);
    }
}

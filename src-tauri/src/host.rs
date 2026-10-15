//! The real window behind `launcher_app::Host`.
use launcher_app::{AppEvent, Host};
use tauri::{AppHandle, Emitter, Manager};

pub const APP_EVENT: &str = "launcher://event";

pub struct TauriHost {
    app: AppHandle,
}

impl TauriHost {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Host for TauriHost {
    fn emit(&self, event: AppEvent) {
        let _ = self.app.emit(APP_EVENT, event);
    }

    fn minimize(&self) {
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.minimize();
        }
    }

    fn restore(&self) {
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

/// Total RAM in MB, for the memory slider's soft warning.
pub fn system_memory_mb() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.total_memory() / (1024 * 1024)
}

/// "Windows 11 Pro 26200" or "Linux 24.04 Ubuntu", for diagnostics.
pub fn os_description() -> String {
    sysinfo::System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string())
}

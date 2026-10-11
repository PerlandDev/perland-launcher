// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod host;

use std::{path::PathBuf, sync::Arc};

use launcher_app::{AppConfig, AppCore, CoreRunner};
use launcher_core::{
    accounts::BuildKind, download::Downloader, install::Installer, meta::MetaClient,
    paths::LauncherDirs, platform::Platform,
};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_window_state::StateFlags;

pub type Core = AppCore<CoreRunner, host::TauriHost>;

const CLOSE_REQUESTED: &str = "launcher://close-requested";

fn main() {
    tauri::Builder::default()
        // Must be first: a second launch focuses the running launcher instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let core = build_core(app.handle().clone())?;
            app.manage(core);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing while a game runs asks first (the UI shows the dialog).
            if let WindowEvent::CloseRequested { api, .. } = event {
                if let Some(core) = window.try_state::<Arc<Core>>() {
                    if core.has_active_runs() {
                        api.prevent_close();
                        let _ = window.emit(CLOSE_REQUESTED, ());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::settings_get,
            commands::settings_set,
            commands::instances_list,
            commands::instance_create,
            commands::instance_update,
            commands::instance_rename,
            commands::instance_delete,
            commands::catalog_minecraft,
            commands::catalog_loaders,
            commands::accounts_list,
            commands::account_add_offline,
            commands::account_set_active,
            commands::account_remove,
            commands::play,
            commands::stop,
            commands::run_states,
            commands::log_tail,
            commands::mods_list,
            commands::mod_set_enabled,
            commands::mods_add,
            commands::mod_remove,
            commands::open_folder,
            commands::java_test,
            commands::quit,
        ])
        .run(tauri::generate_context!())
        .expect("Perland Launcher failed to start");
}

/// `PERLAND_LAUNCHER_ROOT` points development runs at a scratch data folder.
fn build_core(app: tauri::AppHandle) -> Result<Arc<Core>, Box<dyn std::error::Error>> {
    let root = std::env::var_os("PERLAND_LAUNCHER_ROOT")
        .map(PathBuf::from)
        .or_else(LauncherDirs::default_root)
        .ok_or("couldn't find a data folder")?;
    let dirs = LauncherDirs::new(root);
    let version = env!("CARGO_PKG_VERSION").to_string();
    let http = launcher_core::http::client(&version)?;
    let meta = MetaClient::new(http.clone(), dirs.meta());
    let installer = Arc::new(Installer {
        dirs: dirs.clone(),
        meta: meta.clone(),
        downloader: Downloader::new(http.clone()),
        platform: Platform::current(),
    });
    let syncer = launcher_core::sync::Syncer::for_installer(installer.clone(), http);
    let runner = Arc::new(CoreRunner::new(installer, syncer, version.clone()));
    let host = Arc::new(host::TauriHost::new(app));
    let config = AppConfig {
        dirs,
        build: BuildKind::current(),
        launcher_version: version,
        system_memory_mb: host::system_memory_mb(),
        downloads_dir: launcher_core::sync::manual::downloads_dir(),
    };
    Ok(AppCore::new(config, meta, runner, host))
}

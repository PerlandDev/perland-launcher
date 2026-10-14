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
use tauri_plugin_deep_link::DeepLinkExt;
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
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // The window is built here rather than from the config so that DevTools exist only
            // while developing (`tauri dev`): tester builds have debug assertions on, which would
            // otherwise turn them on (and "Inspect" in the page menu) for players.
            let window = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .ok_or("the main window is missing from tauri.conf.json")?
                .clone();
            tauri::WebviewWindowBuilder::from_config(app.handle(), &window)?
                .devtools(cfg!(dev))
                .build()?;
            let core = build_core(app.handle().clone())?;
            app.manage(core.clone());
            // Installers register the scheme; a dev run registers itself (per user, no admin).
            #[cfg(all(debug_assertions, any(windows, target_os = "linux")))]
            if let Err(e) = app.deep_link().register_all() {
                eprintln!("couldn't register perland:// links: {e}");
            }
            // Started by a link…
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                for url in urls {
                    core.deep_link(url.as_str());
                }
            }
            // …or handed one while running (a second launch, through single-instance).
            let linked = core.clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    linked.deep_link(url.as_str());
                }
            });
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
            commands::content_list,
            commands::content_remove,
            commands::open_folder,
            commands::java_test,
            commands::pack_add,
            commands::play_answer,
            commands::pack_update,
            commands::pack_status,
            commands::pack_contents,
            commands::pack_set_optional,
            commands::manual_import,
            commands::pack_art,
            commands::deep_link_take,
            commands::browse_search,
            commands::browse_categories,
            commands::browse_project,
            commands::browse_versions,
            commands::browse_image,
            commands::browse_installed,
            commands::browse_plan,
            commands::browse_install,
            commands::open_link,
            commands::join_preview,
            commands::join_add,
            commands::server_status,
            commands::server_relink,
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
    let browse = launcher_core::browse::Browse::new(
        http.clone(),
        launcher_core::http::client_without_redirects(&version)?,
        Downloader::new(http.clone()),
        &dirs,
        launcher_core::browse::BrowseConfig::default(),
    );
    let syncer = launcher_core::sync::Syncer::for_installer(installer.clone(), http);
    let runner = Arc::new(CoreRunner::new(installer, syncer, version.clone(), browse));
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

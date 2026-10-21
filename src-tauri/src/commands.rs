//! One thin wrapper per UI command. All behavior lives in `launcher_app::AppCore`.
use std::{path::PathBuf, sync::Arc, time::Duration};

use launcher_app::deep_link::DeepLink;
use launcher_app::dto::{
    AppInfo, ArtChoice, BrowsePlanRequest, ContentFile, EmailCodeSent, FreeServerCard,
    FreeServerPage, IconChoice, ImportPreview, ImportRequest, InstallPlanView, InstallResult,
    InstanceArtData, JavaFound, JavaOverview, JoinPreview, LinkCodeView, ManualProgress,
    NewInstance, OpenTarget, PackAddRequest, PackContents, PackStatus, PerlandAccount,
    PerlandStats, PerlandStatus, PlaytimeEntry, PublicServerPage, RunSnapshot, SearchRequest,
    ServerNews, ServerStatusView, YourServersPage,
};
use launcher_app::updates::UpdateState;
use launcher_core::{
    accounts::{AccountsFile, BuildKind},
    browse::{
        compat::ContentKind,
        images::ImageKind,
        project::{ProjectDetail, VersionInfo},
        search::SearchPage,
    },
    instances::{Focus, Instance, InstanceListing},
    meta::{
        catalog::{LoaderVersion, McVersion},
        loaders::LoaderKind,
    },
    mods::LocalMod,
    settings::{Channel, Settings},
    ErrorCode, LauncherError,
};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::Core;

type Res<T> = Result<T, LauncherError>;
type CoreState<'a> = State<'a, Arc<Core>>;
type UpdatesState<'a> = State<'a, Arc<crate::Updates>>;

/// Every error a command returns goes to the launcher log.
fn logged<T>(command: &'static str, result: Res<T>) -> Res<T> {
    if let Err(error) = &result {
        launcher_core::logging::log_error(command, error);
    }
    result
}

#[tauri::command]
pub async fn app_info(core: CoreState<'_>) -> Res<AppInfo> {
    logged("app_info", core.app_info().await)
}

#[tauri::command]
pub async fn settings_get(core: CoreState<'_>) -> Res<Settings> {
    Ok(core.settings_get().await)
}

#[tauri::command]
pub async fn settings_set(
    core: CoreState<'_>,
    updates: UpdatesState<'_>,
    settings: Settings,
) -> Res<Settings> {
    let saved = logged("settings_set", core.settings_set(settings).await)?;
    let channel = Channel::effective(saved.update_channel, BuildKind::current());
    let updates = updates.inner().clone();
    tauri::async_runtime::spawn(async move {
        updates.set_channel(channel).await;
    });
    Ok(saved)
}

#[tauri::command]
pub fn update_status(updates: UpdatesState<'_>) -> UpdateState {
    updates.status()
}

#[tauri::command]
pub async fn update_check(updates: UpdatesState<'_>) -> Res<UpdateState> {
    Ok(updates.check().await)
}

#[tauri::command]
pub async fn update_restart(core: CoreState<'_>, updates: UpdatesState<'_>) -> Res<()> {
    logged(
        "update_restart",
        updates.restart_now(core.update_blocked()).await,
    )
}

#[tauri::command]
pub async fn instances_list(core: CoreState<'_>) -> Res<InstanceListing> {
    logged("instances_list", core.instances_list().await)
}

#[tauri::command]
pub async fn playtime_list(core: CoreState<'_>) -> Res<Vec<PlaytimeEntry>> {
    logged("playtime_list", core.playtime_list().await)
}

#[tauri::command]
pub async fn instance_reorder(core: CoreState<'_>, ids: Vec<String>) -> Res<InstanceListing> {
    logged("instance_reorder", core.instance_reorder(ids).await)
}

#[tauri::command]
pub async fn instance_create(core: CoreState<'_>, request: NewInstance) -> Res<Instance> {
    logged("instance_create", core.instance_create(request).await)
}

#[tauri::command]
pub async fn import_inspect(core: CoreState<'_>, path: PathBuf) -> Res<ImportPreview> {
    logged("import_inspect", core.import_inspect(path).await)
}

#[tauri::command]
pub async fn import_instance(core: CoreState<'_>, request: ImportRequest) -> Res<Instance> {
    logged("import_instance", core.import_instance(request).await)
}

#[tauri::command]
pub fn import_cancel(core: CoreState<'_>) {
    core.import_cancel();
}

#[tauri::command]
pub async fn instance_update(core: CoreState<'_>, instance: Instance) -> Res<Instance> {
    logged("instance_update", core.instance_update(instance).await)
}

#[tauri::command]
pub async fn instance_rename(core: CoreState<'_>, id: String, name: String) -> Res<Instance> {
    logged("instance_rename", core.instance_rename(&id, &name).await)
}

#[tauri::command]
pub async fn instance_delete(core: CoreState<'_>, id: String) -> Res<()> {
    logged("instance_delete", core.instance_delete(&id).await)
}

#[tauri::command]
pub async fn catalog_minecraft(core: CoreState<'_>) -> Res<Vec<McVersion>> {
    logged("catalog_minecraft", core.catalog_minecraft().await)
}

#[tauri::command]
pub async fn catalog_loaders(
    core: CoreState<'_>,
    kind: LoaderKind,
    mc: String,
) -> Res<Vec<LoaderVersion>> {
    logged("catalog_loaders", core.catalog_loaders(kind, &mc).await)
}

#[tauri::command]
pub async fn accounts_list(core: CoreState<'_>) -> Res<AccountsFile> {
    logged("accounts_list", core.accounts_list().await)
}

#[tauri::command]
pub async fn account_add_offline(core: CoreState<'_>, name: String) -> Res<AccountsFile> {
    logged("account_add_offline", core.account_add_offline(&name).await)
}

#[tauri::command]
pub async fn account_set_active(core: CoreState<'_>, uuid: String) -> Res<AccountsFile> {
    logged("account_set_active", core.account_set_active(&uuid).await)
}

#[tauri::command]
pub async fn account_remove(core: CoreState<'_>, uuid: String) -> Res<AccountsFile> {
    logged("account_remove", core.account_remove(&uuid).await)
}

#[tauri::command]
pub async fn play(core: CoreState<'_>, id: String, join: Option<bool>) -> Res<()> {
    logged(
        "play",
        core.inner().play_with(&id, join.unwrap_or(false)).await,
    )
}

#[tauri::command]
pub fn stop(core: CoreState<'_>, id: String) -> Res<()> {
    logged("stop", core.stop(&id))
}

#[tauri::command]
pub fn run_states(core: CoreState<'_>) -> Vec<RunSnapshot> {
    core.run_states()
}

#[tauri::command]
pub fn log_tail(core: CoreState<'_>, id: String) -> Vec<String> {
    core.log_tail(&id)
}

#[tauri::command]
pub async fn mods_list(core: CoreState<'_>, id: String) -> Res<Vec<LocalMod>> {
    logged("mods_list", core.mods_list(&id).await)
}

#[tauri::command]
pub async fn mod_icon(core: CoreState<'_>, id: String, file: String) -> Res<Option<String>> {
    logged("mod_icon", core.mod_icon(&id, &file).await)
}

#[tauri::command]
pub async fn mod_set_enabled(
    core: CoreState<'_>,
    id: String,
    file: String,
    enabled: bool,
) -> Res<Vec<LocalMod>> {
    logged(
        "mod_set_enabled",
        core.mod_set_enabled(&id, &file, enabled).await,
    )
}

#[tauri::command]
pub async fn mods_add(core: CoreState<'_>, id: String, paths: Vec<PathBuf>) -> Res<Vec<LocalMod>> {
    logged("mods_add", core.mods_add(&id, paths).await)
}

#[tauri::command]
pub async fn mod_remove(core: CoreState<'_>, id: String, file: String) -> Res<Vec<LocalMod>> {
    logged("mod_remove", core.mod_remove(&id, &file).await)
}

#[tauri::command]
pub async fn content_list(
    core: CoreState<'_>,
    id: String,
    kind: ContentKind,
) -> Res<Vec<ContentFile>> {
    logged("content_list", core.content_list(&id, kind).await)
}

#[tauri::command]
pub async fn content_remove(
    core: CoreState<'_>,
    id: String,
    kind: ContentKind,
    file: String,
) -> Res<Vec<ContentFile>> {
    logged(
        "content_remove",
        core.content_remove(&id, kind, &file).await,
    )
}

#[tauri::command]
pub async fn open_folder(app: AppHandle, core: CoreState<'_>, target: OpenTarget) -> Res<()> {
    let path = logged("open_folder", core.folder(target).await)?;
    logged(
        "open_folder",
        app.opener()
            .open_path(path.to_string_lossy().into_owned(), None::<&str>)
            .map_err(|e| {
                LauncherError::disk(ErrorCode::Internal, "Couldn't open the folder").with_detail(e)
            }),
    )
}

#[tauri::command]
pub async fn java_test(
    core: CoreState<'_>,
    path: PathBuf,
    minecraft: Option<String>,
) -> Res<JavaFound> {
    logged("java_test", core.java_test(path, minecraft).await)
}

#[tauri::command]
pub async fn java_overview(core: CoreState<'_>, minecraft: String) -> Res<JavaOverview> {
    logged("java_overview", core.java_overview(minecraft).await)
}

#[tauri::command]
pub async fn pack_add(core: CoreState<'_>, request: PackAddRequest) -> Res<Instance> {
    logged("pack_add", core.pack_add(request).await)
}

#[tauri::command]
pub fn play_answer(core: CoreState<'_>, id: String, update: bool) -> Res<()> {
    logged("play_answer", core.play_answer(&id, update))
}

#[tauri::command]
pub async fn pack_update(core: CoreState<'_>, id: String) -> Res<()> {
    logged("pack_update", core.inner().pack_update(&id).await)
}

#[tauri::command]
pub async fn pack_status(core: CoreState<'_>, id: String) -> Res<PackStatus> {
    logged("pack_status", core.pack_status(&id).await)
}

#[tauri::command]
pub async fn pack_contents(core: CoreState<'_>, id: String) -> Res<PackContents> {
    logged("pack_contents", core.pack_contents(&id).await)
}

#[tauri::command]
pub async fn pack_set_optional(
    core: CoreState<'_>,
    id: String,
    path: String,
    enabled: bool,
) -> Res<Instance> {
    logged(
        "pack_set_optional",
        core.pack_set_optional(&id, &path, enabled).await,
    )
}

#[tauri::command]
pub async fn manual_import(core: CoreState<'_>, id: String) -> Res<ManualProgress> {
    logged("manual_import", core.manual_import(&id).await)
}

#[tauri::command]
pub async fn instance_art(core: CoreState<'_>, id: String) -> Res<InstanceArtData> {
    logged("instance_art", core.instance_art(&id).await)
}

#[tauri::command]
pub async fn instance_art_set(core: CoreState<'_>, id: String, choice: ArtChoice) -> Res<Instance> {
    logged("instance_art_set", core.instance_art_set(&id, choice).await)
}

#[tauri::command]
pub async fn instance_screenshots(core: CoreState<'_>, id: String) -> Res<Vec<String>> {
    logged("instance_screenshots", core.instance_screenshots(&id).await)
}

#[tauri::command]
pub async fn instance_screenshot(
    core: CoreState<'_>,
    id: String,
    name: String,
) -> Res<Option<String>> {
    logged(
        "instance_screenshot",
        core.instance_screenshot(&id, &name).await,
    )
}

#[tauri::command]
pub async fn instance_icon_set(
    core: CoreState<'_>,
    id: String,
    choice: IconChoice,
) -> Res<Instance> {
    logged(
        "instance_icon_set",
        core.instance_icon_set(&id, choice).await,
    )
}

#[tauri::command]
pub async fn instance_banner_focus_set(
    core: CoreState<'_>,
    id: String,
    focus: Option<Focus>,
) -> Res<Instance> {
    logged(
        "instance_banner_focus_set",
        core.instance_banner_focus_set(&id, focus).await,
    )
}

#[tauri::command]
pub fn deep_link_take(core: CoreState<'_>) -> Option<DeepLink> {
    core.deep_link_take()
}

#[tauri::command]
pub async fn browse_search(core: CoreState<'_>, request: SearchRequest) -> Res<SearchPage> {
    logged("browse_search", core.browse_search(request).await)
}

#[tauri::command]
pub async fn browse_categories(core: CoreState<'_>, kind: ContentKind) -> Res<Vec<String>> {
    logged("browse_categories", core.browse_categories(kind).await)
}

#[tauri::command]
pub async fn browse_project(core: CoreState<'_>, id: String) -> Res<ProjectDetail> {
    logged("browse_project", core.browse_project(&id).await)
}

#[tauri::command]
pub async fn browse_versions(
    core: CoreState<'_>,
    project: String,
    instance: Option<String>,
) -> Res<Vec<VersionInfo>> {
    logged(
        "browse_versions",
        core.browse_versions(&project, instance.as_deref()).await,
    )
}

#[tauri::command]
pub async fn browse_image(core: CoreState<'_>, url: String, kind: ImageKind) -> Res<String> {
    logged("browse_image", core.browse_image(&url, kind).await)
}

#[tauri::command]
pub async fn browse_installed(core: CoreState<'_>, id: String) -> Res<Vec<String>> {
    logged("browse_installed", core.browse_installed(&id).await)
}

#[tauri::command]
pub async fn browse_plan(core: CoreState<'_>, request: BrowsePlanRequest) -> Res<InstallPlanView> {
    logged("browse_plan", core.browse_plan(request).await)
}

#[tauri::command]
pub async fn browse_install(core: CoreState<'_>, plan_id: String) -> Res<InstallResult> {
    logged("browse_install", core.browse_install(&plan_id).await)
}

/// Project and Markdown links: https only (checked in `launcher-app`), opened from Rust, so the
/// webview's opener scope stays as narrow as it was.
#[tauri::command]
pub fn open_link(app: AppHandle, url: String) -> Res<()> {
    let url = logged("open_link", launcher_app::browse::link_to_open(&url))?;
    logged(
        "open_link",
        app.opener().open_url(url, None::<&str>).map_err(|e| {
            LauncherError::pack(ErrorCode::LinkRefused, "That link can't be opened").with_detail(e)
        }),
    )
}

#[tauri::command]
pub async fn join_preview(core: CoreState<'_>, link: String) -> Res<JoinPreview> {
    logged("join_preview", core.join_preview(&link).await)
}

#[tauri::command]
pub async fn servers_list(
    core: CoreState<'_>,
    query: launcher_core::public_list::ListQuery,
) -> Res<PublicServerPage> {
    logged("servers_list", core.servers_list(query).await)
}

#[tauri::command]
pub async fn servers_icon(
    core: CoreState<'_>,
    token: String,
    icon_version: String,
) -> Res<Option<String>> {
    Ok(core.servers_icon(&token, &icon_version).await)
}

#[tauri::command]
pub async fn server_news(
    core: CoreState<'_>,
    invite: String,
    before: Option<String>,
    limit: u8,
) -> Res<Option<ServerNews>> {
    // Show more's errors show in the dialog and are logged. The line's first page polls on every
    // card and renders nothing on an error, so its failures stay at debug.
    let later = before.is_some();
    let result = core.server_news(&invite, before, limit).await;
    if later {
        return logged("server_news", result);
    }
    if let Err(error) = &result {
        launcher_core::logging::log_quiet("server_news", error);
    }
    result
}

#[tauri::command]
pub async fn server_news_opened(core: CoreState<'_>, invite: String, newest: String) -> Res<()> {
    logged(
        "server_news_opened",
        core.server_news_opened(&invite, &newest).await,
    )
}

#[tauri::command]
pub async fn news_image(
    core: CoreState<'_>,
    invite: String,
    post_id: String,
    image_version: String,
) -> Res<Option<String>> {
    Ok(core.news_image(&invite, &post_id, &image_version).await)
}

#[tauri::command]
pub async fn join_add(core: CoreState<'_>, link: String) -> Res<Instance> {
    logged("join_add", core.join_add(&link).await)
}

#[tauri::command]
pub async fn server_status(core: CoreState<'_>, id: String) -> Res<ServerStatusView> {
    logged("server_status", core.server_status(&id).await)
}

#[tauri::command]
pub async fn server_relink(core: CoreState<'_>, id: String, link: String) -> Res<Instance> {
    logged("server_relink", core.server_relink(&id, &link).await)
}

#[tauri::command]
pub async fn server_link(core: CoreState<'_>, id: String, link: String) -> Res<Instance> {
    logged("server_link", core.server_link(&id, &link).await)
}

#[tauri::command]
pub async fn server_unlink(core: CoreState<'_>, id: String) -> Res<Instance> {
    logged("server_unlink", core.server_unlink(&id).await)
}

#[tauri::command]
pub async fn perland_status(core: CoreState<'_>) -> Res<PerlandStatus> {
    logged("perland_status", core.perland_status().await)
}

#[tauri::command]
pub async fn perland_sign_in_discord(
    core: CoreState<'_>,
    language: String,
) -> Res<Option<PerlandAccount>> {
    logged(
        "perland_sign_in_discord",
        core.perland_sign_in_discord(&language).await,
    )
}

#[tauri::command]
pub async fn perland_sign_in_github(
    core: CoreState<'_>,
    language: String,
) -> Res<Option<PerlandAccount>> {
    logged(
        "perland_sign_in_github",
        core.perland_sign_in_github(&language).await,
    )
}

#[tauri::command]
pub async fn perland_sign_in_reopen(core: CoreState<'_>) -> Res<()> {
    logged("perland_sign_in_reopen", core.perland_sign_in_reopen())
}

#[tauri::command]
pub async fn perland_sign_in_cancel(core: CoreState<'_>) -> Res<()> {
    core.perland_sign_in_cancel();
    Ok(())
}

#[tauri::command]
pub async fn perland_email_start(
    core: CoreState<'_>,
    email: String,
    language: String,
) -> Res<EmailCodeSent> {
    logged(
        "perland_email_start",
        core.perland_email_start(&email, &language).await,
    )
}

#[tauri::command]
pub async fn perland_email_verify(
    core: CoreState<'_>,
    email: String,
    code: String,
) -> Res<PerlandAccount> {
    logged(
        "perland_email_verify",
        core.perland_email_verify(&email, &code).await,
    )
}

/// Opened from Rust, like `open_link`, so the webview's opener scope stays as narrow as it was.
#[tauri::command]
pub async fn perland_manage(core: CoreState<'_>, stack: String) -> Res<()> {
    logged("perland_manage", core.perland_manage(&stack))
}

#[tauri::command]
pub async fn perland_sign_out(core: CoreState<'_>) -> Res<()> {
    logged("perland_sign_out", core.perland_sign_out().await)
}

#[tauri::command]
pub async fn perland_link_code(core: CoreState<'_>) -> Res<LinkCodeView> {
    logged("perland_link_code", core.perland_link_code().await)
}

#[tauri::command]
pub async fn perland_link_poll(core: CoreState<'_>) -> Res<Option<String>> {
    logged("perland_link_poll", core.perland_link_poll().await)
}

#[tauri::command]
pub async fn perland_unlink(core: CoreState<'_>) -> Res<()> {
    logged("perland_unlink", core.perland_unlink().await)
}

#[tauri::command]
pub async fn perland_stats(core: CoreState<'_>) -> Res<Option<PerlandStats>> {
    logged("perland_stats", core.perland_stats().await)
}

#[tauri::command]
pub async fn your_servers(core: CoreState<'_>) -> Res<Option<YourServersPage>> {
    logged("your_servers", core.your_servers().await)
}

/// Fire and forget: never fails.
#[tauri::command]
pub async fn your_servers_follow(core: CoreState<'_>, invite: String) -> Res<()> {
    core.your_servers_follow(&invite).await;
    Ok(())
}

#[tauri::command]
pub async fn your_servers_unfollow(core: CoreState<'_>, invite: String) -> Res<()> {
    logged(
        "your_servers_unfollow",
        core.your_servers_unfollow(&invite).await,
    )
}

/// Fire and forget: never fails.
#[tauri::command]
pub async fn your_servers_joined(core: CoreState<'_>, invite: String) -> Res<()> {
    core.your_servers_joined(&invite).await;
    Ok(())
}

#[tauri::command]
pub async fn free_server(core: CoreState<'_>) -> Res<Option<FreeServerPage>> {
    logged("free_server", core.free_server().await)
}

#[tauri::command]
pub async fn free_server_create(
    core: CoreState<'_>,
    name: String,
    kind: String,
    modrinth_version_id: Option<String>,
    language: String,
) -> Res<FreeServerCard> {
    logged(
        "free_server_create",
        core.free_server_create(&name, &kind, modrinth_version_id, &language)
            .await,
    )
}

#[tauri::command]
pub async fn free_server_start(core: CoreState<'_>) -> Res<FreeServerCard> {
    logged("free_server_start", core.free_server_start().await)
}

#[tauri::command]
pub async fn free_server_delete(core: CoreState<'_>) -> Res<bool> {
    logged("free_server_delete", core.free_server_delete().await)
}

#[tauri::command]
pub async fn free_server_play(core: CoreState<'_>) -> Res<String> {
    logged("free_server_play", core.free_server_play().await)
}

/// Called by the close dialog. Stopping waits a few seconds so the kills land before exit;
/// keeping the games playing counts their time so far, as nothing tracks them after exit.
#[tauri::command]
pub async fn quit(app: AppHandle, core: CoreState<'_>, stop_games: bool) -> Res<()> {
    if stop_games {
        core.stop_all(Duration::from_secs(5)).await;
    } else {
        core.record_running_playtime().await;
    }
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn diagnostics_export(core: CoreState<'_>, path: PathBuf) -> Res<String> {
    logged(
        "diagnostics_export",
        core.diagnostics_export(path, crate::host::os_description())
            .await,
    )
    .map(|p| p.display().to_string())
}

/// Reveals the file the last export wrote. The path never comes from the webview.
#[tauri::command]
pub fn diagnostics_reveal(app: AppHandle, core: CoreState<'_>) -> Res<()> {
    if let Some(path) = core.diagnostics_last() {
        let _ = app.opener().reveal_item_in_dir(path);
    }
    Ok(())
}

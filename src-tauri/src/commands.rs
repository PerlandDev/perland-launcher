//! One thin wrapper per UI command. All behavior lives in `launcher_app::AppCore`.
use std::{path::PathBuf, sync::Arc, time::Duration};

use launcher_app::deep_link::DeepLink;
use launcher_app::dto::{
    AppInfo, ArtChoice, BrowsePlanRequest, ContentFile, InstallPlanView, InstallResult,
    InstanceArtData, JavaTest, JoinPreview, ManualProgress, NewInstance, OpenTarget,
    PackAddRequest, PackContents, PackStatus, RunSnapshot, SearchRequest, ServerStatusView,
};
use launcher_core::{
    accounts::AccountsFile,
    browse::{
        compat::ContentKind,
        images::ImageKind,
        project::{ProjectDetail, VersionInfo},
        search::SearchPage,
    },
    instances::{Instance, InstanceListing},
    meta::{
        catalog::{LoaderVersion, McVersion},
        loaders::LoaderKind,
    },
    mods::LocalMod,
    settings::Settings,
    ErrorCode, LauncherError,
};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::Core;

type Res<T> = Result<T, LauncherError>;
type CoreState<'a> = State<'a, Arc<Core>>;

#[tauri::command]
pub async fn app_info(core: CoreState<'_>) -> Res<AppInfo> {
    core.app_info().await
}

#[tauri::command]
pub async fn settings_get(core: CoreState<'_>) -> Res<Settings> {
    Ok(core.settings_get().await)
}

#[tauri::command]
pub async fn settings_set(core: CoreState<'_>, settings: Settings) -> Res<Settings> {
    core.settings_set(settings).await
}

#[tauri::command]
pub async fn instances_list(core: CoreState<'_>) -> Res<InstanceListing> {
    core.instances_list().await
}

#[tauri::command]
pub async fn instance_reorder(core: CoreState<'_>, ids: Vec<String>) -> Res<InstanceListing> {
    core.instance_reorder(ids).await
}

#[tauri::command]
pub async fn instance_create(core: CoreState<'_>, request: NewInstance) -> Res<Instance> {
    core.instance_create(request).await
}

#[tauri::command]
pub async fn instance_update(core: CoreState<'_>, instance: Instance) -> Res<Instance> {
    core.instance_update(instance).await
}

#[tauri::command]
pub async fn instance_rename(core: CoreState<'_>, id: String, name: String) -> Res<Instance> {
    core.instance_rename(&id, &name).await
}

#[tauri::command]
pub async fn instance_delete(core: CoreState<'_>, id: String) -> Res<()> {
    core.instance_delete(&id).await
}

#[tauri::command]
pub async fn catalog_minecraft(core: CoreState<'_>) -> Res<Vec<McVersion>> {
    core.catalog_minecraft().await
}

#[tauri::command]
pub async fn catalog_loaders(
    core: CoreState<'_>,
    kind: LoaderKind,
    mc: String,
) -> Res<Vec<LoaderVersion>> {
    core.catalog_loaders(kind, &mc).await
}

#[tauri::command]
pub async fn accounts_list(core: CoreState<'_>) -> Res<AccountsFile> {
    core.accounts_list().await
}

#[tauri::command]
pub async fn account_add_offline(core: CoreState<'_>, name: String) -> Res<AccountsFile> {
    core.account_add_offline(&name).await
}

#[tauri::command]
pub async fn account_set_active(core: CoreState<'_>, uuid: String) -> Res<AccountsFile> {
    core.account_set_active(&uuid).await
}

#[tauri::command]
pub async fn account_remove(core: CoreState<'_>, uuid: String) -> Res<AccountsFile> {
    core.account_remove(&uuid).await
}

#[tauri::command]
pub async fn play(core: CoreState<'_>, id: String, join: Option<bool>) -> Res<()> {
    core.inner().play_with(&id, join.unwrap_or(true)).await
}

#[tauri::command]
pub fn stop(core: CoreState<'_>, id: String) -> Res<()> {
    core.stop(&id)
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
    core.mods_list(&id).await
}

#[tauri::command]
pub async fn mod_set_enabled(
    core: CoreState<'_>,
    id: String,
    file: String,
    enabled: bool,
) -> Res<Vec<LocalMod>> {
    core.mod_set_enabled(&id, &file, enabled).await
}

#[tauri::command]
pub async fn mods_add(core: CoreState<'_>, id: String, paths: Vec<PathBuf>) -> Res<Vec<LocalMod>> {
    core.mods_add(&id, paths).await
}

#[tauri::command]
pub async fn mod_remove(core: CoreState<'_>, id: String, file: String) -> Res<Vec<LocalMod>> {
    core.mod_remove(&id, &file).await
}

#[tauri::command]
pub async fn content_list(
    core: CoreState<'_>,
    id: String,
    kind: ContentKind,
) -> Res<Vec<ContentFile>> {
    core.content_list(&id, kind).await
}

#[tauri::command]
pub async fn content_remove(
    core: CoreState<'_>,
    id: String,
    kind: ContentKind,
    file: String,
) -> Res<Vec<ContentFile>> {
    core.content_remove(&id, kind, &file).await
}

#[tauri::command]
pub async fn open_folder(app: AppHandle, core: CoreState<'_>, target: OpenTarget) -> Res<()> {
    let path = core.folder(target).await?;
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| {
            LauncherError::disk(ErrorCode::Internal, "Couldn't open the folder").with_detail(e)
        })
}

#[tauri::command]
pub async fn java_test(core: CoreState<'_>, path: PathBuf) -> Res<JavaTest> {
    core.java_test(path).await
}

#[tauri::command]
pub async fn pack_add(core: CoreState<'_>, request: PackAddRequest) -> Res<Instance> {
    core.pack_add(request).await
}

#[tauri::command]
pub fn play_answer(core: CoreState<'_>, id: String, update: bool) -> Res<()> {
    core.play_answer(&id, update)
}

#[tauri::command]
pub async fn pack_update(core: CoreState<'_>, id: String) -> Res<()> {
    core.inner().pack_update(&id).await
}

#[tauri::command]
pub async fn pack_status(core: CoreState<'_>, id: String) -> Res<PackStatus> {
    core.pack_status(&id).await
}

#[tauri::command]
pub async fn pack_contents(core: CoreState<'_>, id: String) -> Res<PackContents> {
    core.pack_contents(&id).await
}

#[tauri::command]
pub async fn pack_set_optional(
    core: CoreState<'_>,
    id: String,
    path: String,
    enabled: bool,
) -> Res<Instance> {
    core.pack_set_optional(&id, &path, enabled).await
}

#[tauri::command]
pub async fn manual_import(core: CoreState<'_>, id: String) -> Res<ManualProgress> {
    core.manual_import(&id).await
}

#[tauri::command]
pub async fn instance_art(core: CoreState<'_>, id: String) -> Res<InstanceArtData> {
    core.instance_art(&id).await
}

#[tauri::command]
pub async fn instance_art_set(core: CoreState<'_>, id: String, choice: ArtChoice) -> Res<Instance> {
    core.instance_art_set(&id, choice).await
}

#[tauri::command]
pub fn deep_link_take(core: CoreState<'_>) -> Option<DeepLink> {
    core.deep_link_take()
}

#[tauri::command]
pub async fn browse_search(core: CoreState<'_>, request: SearchRequest) -> Res<SearchPage> {
    core.browse_search(request).await
}

#[tauri::command]
pub async fn browse_categories(core: CoreState<'_>, kind: ContentKind) -> Res<Vec<String>> {
    core.browse_categories(kind).await
}

#[tauri::command]
pub async fn browse_project(core: CoreState<'_>, id: String) -> Res<ProjectDetail> {
    core.browse_project(&id).await
}

#[tauri::command]
pub async fn browse_versions(
    core: CoreState<'_>,
    project: String,
    instance: Option<String>,
) -> Res<Vec<VersionInfo>> {
    core.browse_versions(&project, instance.as_deref()).await
}

#[tauri::command]
pub async fn browse_image(core: CoreState<'_>, url: String, kind: ImageKind) -> Res<String> {
    core.browse_image(&url, kind).await
}

#[tauri::command]
pub async fn browse_installed(core: CoreState<'_>, id: String) -> Res<Vec<String>> {
    core.browse_installed(&id).await
}

#[tauri::command]
pub async fn browse_plan(core: CoreState<'_>, request: BrowsePlanRequest) -> Res<InstallPlanView> {
    core.browse_plan(request).await
}

#[tauri::command]
pub async fn browse_install(core: CoreState<'_>, plan_id: String) -> Res<InstallResult> {
    core.browse_install(&plan_id).await
}

/// Project and Markdown links: https only (checked in `launcher-app`), opened from Rust, so the
/// webview's opener scope stays as narrow as it was.
#[tauri::command]
pub fn open_link(app: AppHandle, url: String) -> Res<()> {
    let url = launcher_app::browse::link_to_open(&url)?;
    app.opener().open_url(url, None::<&str>).map_err(|e| {
        LauncherError::pack(ErrorCode::LinkRefused, "That link can't be opened").with_detail(e)
    })
}

#[tauri::command]
pub async fn join_preview(core: CoreState<'_>, link: String) -> Res<JoinPreview> {
    core.join_preview(&link).await
}

#[tauri::command]
pub async fn join_add(core: CoreState<'_>, link: String) -> Res<Instance> {
    core.join_add(&link).await
}

#[tauri::command]
pub async fn server_status(core: CoreState<'_>, id: String) -> Res<ServerStatusView> {
    core.server_status(&id).await
}

#[tauri::command]
pub async fn server_relink(core: CoreState<'_>, id: String, link: String) -> Res<Instance> {
    core.server_relink(&id, &link).await
}

/// Called by the close dialog. Stopping waits a few seconds so the kills land before exit.
#[tauri::command]
pub async fn quit(app: AppHandle, core: CoreState<'_>, stop_games: bool) -> Res<()> {
    if stop_games {
        core.stop_all(Duration::from_secs(5)).await;
    }
    app.exit(0);
    Ok(())
}

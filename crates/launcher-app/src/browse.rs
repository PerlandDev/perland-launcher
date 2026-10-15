//! Browse commands (spec §4): search, project pages, images, what an instance has, and (in
//! Task 14) planned installs. All Modrinth work happens behind `Runner::browser`.
use std::{
    collections::HashSet,
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    browse::{
        compat::{ContentKind, Target},
        identify::Installed,
        images::ImageKind,
        plan::{InstallPlan, PlanRequest},
        project::{ProjectDetail, VersionInfo},
        search::{SearchPage, SearchQuery},
    },
    events::{Event, EventSink, TaskId},
    instances::{GameSpec, Instance, LoaderSpec},
    locks::InstanceLock,
    sync::lockfile::Managed,
    ErrorCode, LauncherError, Result,
};
use tokio::time::Instant;

use crate::{
    app::AppCore,
    dto::{
        BrowsePlanRequest, InstallPlanView, InstallResult, NewInstance, PlanItemView,
        PlanTargetRequest, PlanTargetView, SearchRequest,
    },
    events::{AppEvent, Host, RunStateKind},
    runner::Runner,
};

/// How long a plan stays installable.
pub const PLAN_TTL: Duration = Duration::from_secs(10 * 60);

pub(crate) struct StoredPlan {
    plan: InstallPlan,
    /// The target instance, and its game when planned (a changed game makes the plan stale).
    instance: Option<(String, GameSpec)>,
    created: Instant,
}

impl StoredPlan {
    /// Does this plan install into the instance `id`?
    pub(crate) fn targets(&self, id: &str) -> bool {
        self.instance
            .as_ref()
            .is_some_and(|(target, _)| target == id)
    }
}

fn plan_expired() -> LauncherError {
    LauncherError::pack(
        ErrorCode::ContentPlanExpired,
        "That took too long. Try installing again",
    )
}

fn content_busy(name: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::ContentBusy,
        format!("Wait for {name} to finish updating"),
    )
    .with_param("name", name)
}

pub(crate) fn content_installing() -> LauncherError {
    LauncherError::pack(
        ErrorCode::ContentInstalling,
        "Wait for the download to finish",
    )
}

/// Marks an instance as being installed into; holds the instance lock when no run does.
pub(crate) struct ContentGuard<'a> {
    installing: &'a Mutex<HashSet<String>>,
    id: String,
    _lock: Option<InstanceLock>,
}

impl Drop for ContentGuard<'_> {
    fn drop(&mut self) {
        self.installing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

/// The link a project page may open in the browser: https only, with a host and no user info.
pub fn link_to_open(raw: &str) -> Result<String> {
    let refused = || LauncherError::pack(ErrorCode::LinkRefused, "That link can't be opened");
    let raw = raw.trim();
    // The URL parser reads `https:///host` as `https://host`; a link that omits its host in
    // the text is not one we vouch for.
    let after_scheme = raw
        .get(..8)
        .filter(|p| p.eq_ignore_ascii_case("https://"))
        .map(|_| &raw[8..]);
    if after_scheme.is_none_or(|rest| rest.starts_with(['/', '\\'])) {
        return Err(refused());
    }
    let url = url::Url::parse(raw).map_err(|_| refused())?;
    let plain = url.scheme() == "https"
        && url.host_str().is_some_and(|h| !h.is_empty())
        && url.username().is_empty()
        && url.password().is_none();
    if plain {
        Ok(url.to_string())
    } else {
        Err(refused())
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    pub async fn browse_search(&self, request: SearchRequest) -> Result<SearchPage> {
        let (minecraft, loader) = match &request.instance {
            Some(id) => {
                let instance = self.instances.load(id).await?;
                (
                    Some(instance.game.minecraft),
                    instance.game.loader.map(|l| l.kind),
                )
            }
            None => (request.minecraft, request.loader),
        };
        self.runner
            .browser()
            .search(SearchQuery {
                query: request.query,
                kind: request.kind,
                minecraft,
                loader,
                categories: request.categories,
                sort: request.sort,
                offset: request.offset,
            })
            .await
    }

    pub async fn browse_categories(&self, kind: ContentKind) -> Result<Vec<String>> {
        self.runner.browser().categories(kind).await
    }

    pub async fn browse_project(&self, id: &str) -> Result<ProjectDetail> {
        self.runner.browser().project(id.to_string()).await
    }

    /// The project's versions; only those that fit `instance` when one is given.
    pub async fn browse_versions(
        &self,
        project: &str,
        instance: Option<&str>,
    ) -> Result<Vec<VersionInfo>> {
        let target = match instance {
            Some(id) => Some(Target::of(&self.instances.load(id).await?.game)),
            None => None,
        };
        self.runner
            .browser()
            .versions(project.to_string(), target)
            .await
    }

    /// An icon or gallery image as a `data:` URL (the CSP allows no remote images).
    pub async fn browse_image(&self, url: &str, kind: ImageKind) -> Result<String> {
        let image = self.runner.browser().image(url.to_string(), kind).await?;
        Ok(format!(
            "data:{};base64,{}",
            image.mime,
            STANDARD.encode(&image.bytes)
        ))
    }

    /// The Modrinth projects already in an instance, for "Installed" lamps.
    pub async fn browse_installed(&self, id: &str) -> Result<Vec<String>> {
        let instance = self.instances.load(id).await?;
        let managed = Managed::load(&self.instances.perland_dir(&instance.id)).await;
        let installed = self
            .runner
            .browser()
            .identify(self.instances.game_dir(&instance.id), managed)
            .await?;
        Ok(installed.project_ids())
    }

    /// Spec §4.2: refused while the instance updates; fine while its game prepares or runs
    /// (the run holds the lock, and installs only add files); otherwise take the lock.
    fn content_guard(&self, instance: &Instance) -> Result<ContentGuard<'_>> {
        let runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        let mut installing = self.installing.lock().unwrap_or_else(|e| e.into_inner());
        if installing.contains(&instance.id) {
            return Err(content_installing());
        }
        let lock = match runs.get(&instance.id).map(|run| run.phase) {
            Some(
                RunStateKind::Checking
                | RunStateKind::Asking
                | RunStateKind::Queued
                | RunStateKind::Syncing,
            ) => return Err(content_busy(&instance.name)),
            Some(_) => None,
            None => Some(
                InstanceLock::try_acquire(&self.instances.perland_dir(&instance.id)).map_err(
                    |e| {
                        if e.code == ErrorCode::InstanceLocked {
                            content_busy(&instance.name)
                        } else {
                            e
                        }
                    },
                )?,
            ),
        };
        installing.insert(instance.id.clone());
        Ok(ContentGuard {
            installing: &self.installing,
            id: instance.id.clone(),
            _lock: lock,
        })
    }

    /// Install progress, tagged with the instance (the UI shows it on the install button).
    fn content_sink(&self, id: &str) -> EventSink {
        let (host, id) = (self.host.clone(), id.to_string());
        Arc::new(move |event| {
            if let Event::Progress {
                task,
                done,
                total,
                bytes,
            } = event
            {
                host.emit(AppEvent::Progress {
                    instance: Some(id.clone()),
                    task,
                    done,
                    total,
                    bytes,
                });
            }
        })
    }

    pub async fn browse_plan(&self, request: BrowsePlanRequest) -> Result<InstallPlanView> {
        let browser = self.runner.browser();
        let wanted = PlanRequest {
            project: request.project,
            version: request.version,
        };
        let (instance, plan) = match request.target {
            PlanTargetRequest::Instance { id } => {
                let instance = self.instances.load(&id).await?;
                let managed = Managed::load(&self.instances.perland_dir(&instance.id)).await;
                let installed = browser
                    .identify(self.instances.game_dir(&instance.id), managed)
                    .await?;
                let plan = browser
                    .plan(wanted, Some(Target::of(&instance.game)), installed)
                    .await?;
                (Some(instance), plan)
            }
            PlanTargetRequest::New => (
                None,
                browser.plan(wanted, None, Installed::default()).await?,
            ),
        };
        let target = match (&instance, &plan.new_instance) {
            (Some(i), _) => PlanTargetView::Instance {
                id: i.id.clone(),
                name: i.name.clone(),
                minecraft: i.game.minecraft.clone(),
                loader: i.game.loader.as_ref().map(|l| l.kind),
            },
            (None, Some(new)) => PlanTargetView::New {
                name: new.name.clone(),
                minecraft: new.minecraft.clone(),
                loader: new.loader,
            },
            (None, None) => return Err(plan_expired()),
        };
        let plan_id = format!("plan-{}", self.plan_seq.fetch_add(1, Ordering::Relaxed) + 1);
        let view = InstallPlanView {
            plan_id: plan_id.clone(),
            target,
            items: plan.items.iter().map(PlanItemView::from).collect(),
            skipped: plan.skipped.clone(),
            total_size: plan.items.iter().map(|i| i.file.size).sum(),
        };
        let mut plans = self.plans.lock().unwrap_or_else(|e| e.into_inner());
        plans.retain(|_, p| p.created.elapsed() < PLAN_TTL);
        plans.insert(
            plan_id,
            StoredPlan {
                plan,
                instance: instance.map(|i| (i.id, i.game)),
                created: Instant::now(),
            },
        );
        Ok(view)
    }

    /// Install a stored plan, once. A "New instance" plan creates the instance first; if the
    /// install then fails, the instance stays so the player can retry from it.
    pub async fn browse_install(&self, plan_id: &str) -> Result<InstallResult> {
        let stored = self
            .plans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(plan_id)
            .filter(|p| p.created.elapsed() < PLAN_TTL)
            .ok_or_else(plan_expired)?;
        let browser = self.runner.browser();
        let instance = match &stored.instance {
            Some((id, game)) => {
                let instance = self.instances.load(id).await?;
                if instance.game != *game {
                    return Err(plan_expired());
                }
                instance
            }
            None => {
                let new = stored.plan.new_instance.clone().ok_or_else(plan_expired)?;
                let loader = match new.loader {
                    Some(kind) => Some(LoaderSpec {
                        kind,
                        version: browser.loader_version(kind, new.minecraft.clone()).await?,
                    }),
                    None => None,
                };
                self.instance_create(NewInstance {
                    name: new.name,
                    minecraft: new.minecraft,
                    loader,
                })
                .await?
            }
        };
        let _guard = match self.content_guard(&instance) {
            Ok(guard) => guard,
            Err(refused) => {
                // Busy is temporary: keep the plan (and its age) so the player can retry it.
                // A new instance has just been created and can't be busy.
                if stored.instance.is_some() {
                    self.plans
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(plan_id.to_string(), stored);
                }
                return Err(refused);
            }
        };
        let managed = Managed::load(&self.instances.perland_dir(&instance.id)).await;
        let title = stored
            .plan
            .items
            .first()
            .map(|i| i.title.clone())
            .unwrap_or_default();
        let added = browser
            .install(
                self.instances.game_dir(&instance.id),
                stored.plan.items,
                managed,
                TaskId::new("install_content").with("name", &title),
                self.content_sink(&instance.id),
            )
            .await?;
        Ok(InstallResult { instance, added })
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        browse::{
            compat::ContentKind,
            identify::{Installed, InstalledProject},
            images::{Image, ImageKind},
            search::SortIndex,
        },
        instances::LoaderSpec,
        meta::loaders::LoaderKind,
        ErrorCode,
    };

    use super::*;
    use crate::{
        app::{tests::core, Run},
        dto::{BrowsePlanRequest, NewInstance, PlanTargetRequest, PlanTargetView, SearchRequest},
        events::RunStateKind,
        testing::{managed_for, FakeRunner, RecordingHost},
    };

    type Core = std::sync::Arc<crate::app::AppCore<FakeRunner, RecordingHost>>;

    async fn fabric(core: &Core) -> Instance {
        core.instance_create(NewInstance {
            name: "Asalith".into(),
            minecraft: "1.20.1".into(),
            loader: Some(LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.14".into(),
            }),
        })
        .await
        .unwrap()
    }

    fn into(id: &str) -> BrowsePlanRequest {
        BrowsePlanRequest {
            project: "sodium".into(),
            version: None,
            target: PlanTargetRequest::Instance { id: id.into() },
        }
    }

    fn fake_run(core: &Core, id: &str, phase: RunStateKind) {
        core.runs.lock().unwrap().insert(
            id.to_string(),
            Run {
                phase,
                abort: None,
                killer: None,
                cancel: Default::default(),
                answer: None,
                offer: None,
                update_only: false,
            },
        );
    }

    fn request(instance: Option<&str>) -> SearchRequest {
        SearchRequest {
            query: "sodium".into(),
            kind: ContentKind::Mod,
            instance: instance.map(str::to_string),
            minecraft: Some("1.19.2".into()),
            loader: Some(LoaderKind::Forge),
            categories: vec!["optimization".into()],
            sort: SortIndex::Downloads,
            offset: 20,
        }
    }

    #[tokio::test]
    async fn search_takes_the_game_from_a_scoped_instance() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        let fabric = core
            .instance_create(NewInstance {
                name: "Fabric".into(),
                minecraft: "1.20.1".into(),
                loader: Some(LoaderSpec {
                    kind: LoaderKind::Fabric,
                    version: "0.16.14".into(),
                }),
            })
            .await
            .unwrap();
        core.browse_search(request(Some(&fabric.id))).await.unwrap();
        core.browse_search(request(None)).await.unwrap();
        let searches = runner.browser.searches.lock().unwrap().clone();
        assert_eq!(searches[0].minecraft.as_deref(), Some("1.20.1"));
        assert_eq!(searches[0].loader, Some(LoaderKind::Fabric));
        assert_eq!(searches[1].minecraft.as_deref(), Some("1.19.2"));
        assert_eq!(searches[1].loader, Some(LoaderKind::Forge));
        assert_eq!(
            (searches[1].offset, searches[1].sort),
            (20, SortIndex::Downloads)
        );
        assert_eq!(
            core.browse_search(request(Some("missing")))
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceNotFound
        );
    }

    #[tokio::test]
    async fn images_become_data_urls() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        *runner.browser.image.lock().unwrap() = Some(Image {
            bytes: b"\x89PNG".to_vec(),
            mime: "image/png",
        });
        assert_eq!(
            core.browse_image("https://cdn.modrinth.com/x.png", ImageKind::Icon)
                .await
                .unwrap(),
            "data:image/png;base64,iVBORw=="
        );
    }

    #[tokio::test]
    async fn installed_reads_the_lockfile_and_sorts_ids() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        let instance = core
            .instance_create(NewInstance {
                name: "Sky".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        managed_for(&["mods/jei.jar"])
            .save(&core.instances.perland_dir(&instance.id))
            .await
            .unwrap();
        let project = |path: &str| InstalledProject {
            path: path.into(),
            managed: false,
            incompatible: vec![],
        };
        *runner.browser.installed.lock().unwrap() = Installed {
            projects: [
                ("ZZZ".to_string(), project("mods/z.jar")),
                ("AAA".to_string(), project("mods/a.jar")),
            ]
            .into_iter()
            .collect(),
        };
        assert_eq!(
            core.browse_installed(&instance.id).await.unwrap(),
            vec!["AAA".to_string(), "ZZZ".into()]
        );
        let identified = runner.browser.identified.lock().unwrap().clone();
        assert_eq!(
            identified,
            vec![(core.instances.game_dir(&instance.id), true)]
        );
    }

    #[test]
    fn only_https_links_open() {
        assert_eq!(
            link_to_open(" https://github.com/CaffeineMC/sodium ").unwrap(),
            "https://github.com/CaffeineMC/sodium"
        );
        assert_eq!(
            link_to_open("HTTPS://Example.com").unwrap(),
            "https://example.com/"
        );
        for bad in [
            "http://example.com",
            "javascript:alert(1)",
            "file:///C:/Windows/system32/calc.exe",
            "https://user:pw@example.com/",
            "perland://pack?url=https://x",
            "https:///nohost",
            r"https:\host",
            r"https://\host",
            "  https://user:pw@example.com/  ",
            "",
        ] {
            assert_eq!(
                link_to_open(bad).unwrap_err().code,
                ErrorCode::LinkRefused,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn a_plan_installs_once() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let instance = fabric(&core).await;
        let view = core.browse_plan(into(&instance.id)).await.unwrap();
        assert_eq!(view.items.len(), 2);
        assert_eq!(view.total_size, 200);
        assert_eq!(view.skipped[0].title, "Mod Menu");
        assert!(matches!(&view.target, PlanTargetView::Instance { id, .. } if *id == instance.id));
        let (_, target) = runner.browser.plans.lock().unwrap()[0].clone();
        assert_eq!(target.unwrap().minecraft, "1.20.1");

        let result = core.browse_install(&view.plan_id).await.unwrap();
        assert_eq!(
            result.added,
            vec!["sodium.jar".to_string(), "fabric-api.jar".into()]
        );
        assert_eq!(result.instance.id, instance.id);
        assert_eq!(
            core.browse_install(&view.plan_id).await.unwrap_err().code,
            ErrorCode::ContentPlanExpired
        );
        assert_eq!(runner.browser.installs.lock().unwrap().len(), 1);
        assert!(
            host.progress_count(&instance.id) >= 1,
            "progress names the instance"
        );
        assert!(core.installing.lock().unwrap().is_empty());
        // The instance lock was released: sync could take it now.
        InstanceLock::try_acquire(&core.instances.perland_dir(&instance.id)).unwrap();
    }

    #[tokio::test]
    async fn plans_expire_and_go_stale_when_the_game_changes() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        let instance = fabric(&core).await;
        let stale = core.browse_plan(into(&instance.id)).await.unwrap();
        let mut changed = instance.clone();
        changed.game.minecraft = "1.21.1".into();
        core.instance_update(changed).await.unwrap();
        assert_eq!(
            core.browse_install(&stale.plan_id).await.unwrap_err().code,
            ErrorCode::ContentPlanExpired
        );
        tokio::time::pause();
        let old = core.browse_plan(into(&instance.id)).await.unwrap();
        tokio::time::advance(PLAN_TTL + Duration::from_secs(1)).await;
        assert_eq!(
            core.browse_install(&old.plan_id).await.unwrap_err().code,
            ErrorCode::ContentPlanExpired
        );
        assert!(runner.browser.installs.lock().unwrap().is_empty());
        assert_eq!(
            core.browse_install("plan-999").await.unwrap_err().code,
            ErrorCode::ContentPlanExpired
        );
    }

    #[tokio::test]
    async fn installs_wait_for_updates_but_not_for_a_running_game() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        let instance = fabric(&core).await;
        let mut refused = String::new();
        for phase in [
            RunStateKind::Checking,
            RunStateKind::Asking,
            RunStateKind::Queued,
            RunStateKind::Syncing,
        ] {
            fake_run(&core, &instance.id, phase);
            let view = core.browse_plan(into(&instance.id)).await.unwrap();
            let error = core.browse_install(&view.plan_id).await.unwrap_err();
            assert_eq!(error.code, ErrorCode::ContentBusy, "{phase:?}");
            assert_eq!(error.params["name"], "Asalith");
            refused = view.plan_id;
        }
        // A refusal keeps the plan: once the update is over, the same plan id installs.
        core.runs.lock().unwrap().clear();
        core.browse_install(&refused).await.unwrap();
        assert_eq!(runner.browser.installs.lock().unwrap().len(), 1);

        // A starting or running game holds the instance lock itself; the install doesn't need it.
        let run_lock =
            InstanceLock::try_acquire(&core.instances.perland_dir(&instance.id)).unwrap();
        for phase in [
            RunStateKind::Preparing,
            RunStateKind::Running,
            RunStateKind::Stopping,
        ] {
            fake_run(&core, &instance.id, phase);
            let view = core.browse_plan(into(&instance.id)).await.unwrap();
            core.browse_install(&view.plan_id)
                .await
                .unwrap_or_else(|e| panic!("{phase:?}: {e:?}"));
        }
        core.runs.lock().unwrap().clear();
        drop(run_lock);
        assert_eq!(runner.browser.installs.lock().unwrap().len(), 4);

        // Another process (the CLI) syncing holds the instance lock.
        let foreign = InstanceLock::try_acquire(&core.instances.perland_dir(&instance.id)).unwrap();
        let view = core.browse_plan(into(&instance.id)).await.unwrap();
        assert_eq!(
            core.browse_install(&view.plan_id).await.unwrap_err().code,
            ErrorCode::ContentBusy
        );
        drop(foreign);
        core.browse_install(&view.plan_id).await.unwrap();
        assert_eq!(runner.browser.installs.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn an_instance_cannot_be_deleted_or_changed_mid_install() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        let instance = fabric(&core).await;
        runner.browser.hold_install.store(true, Ordering::SeqCst);
        let view = core.browse_plan(into(&instance.id)).await.unwrap();
        let task = {
            let core = core.clone();
            let id = view.plan_id.clone();
            tokio::spawn(async move { core.browse_install(&id).await })
        };
        while runner.browser.installs.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let mut other_game = instance.clone();
        other_game.game.minecraft = "1.21.1".into();
        let mut other_java = instance.clone();
        other_java.java.memory_mb += 512;
        assert_eq!(
            core.instance_update(other_game.clone())
                .await
                .unwrap_err()
                .code,
            ErrorCode::ContentInstalling
        );
        assert_eq!(
            core.instance_update(other_java.clone())
                .await
                .unwrap_err()
                .code,
            ErrorCode::ContentInstalling
        );
        assert_eq!(
            core.instance_delete(&instance.id).await.unwrap_err().code,
            ErrorCode::ContentInstalling
        );
        // Not even a same-family name: the install is writing into this folder.
        assert_eq!(
            core.instance_rename(&instance.id, "Asalith!")
                .await
                .unwrap_err()
                .code,
            ErrorCode::ContentInstalling
        );

        runner.browser.open_install_gate();
        task.await.unwrap().unwrap();
        let mut renamed_game = core.instances.load(&instance.id).await.unwrap();
        renamed_game.game.minecraft = "1.21.1".into();
        core.instance_update(renamed_game).await.unwrap();
        core.instance_delete(&instance.id).await.unwrap();
    }

    #[tokio::test]
    async fn a_rename_that_moves_the_folder_drops_its_plans() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        let instance = fabric(&core).await;
        let view = core.browse_plan(into(&instance.id)).await.unwrap();
        assert!(core.plans.lock().unwrap().contains_key(&view.plan_id));
        let renamed = core.instance_rename(&instance.id, "Zed").await.unwrap();
        assert_ne!(renamed.id, instance.id);
        assert!(core.plans.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn play_waits_for_a_content_install() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        let instance = fabric(&core).await;
        let accounts = core.account_add_offline("Steve").await.unwrap();
        core.account_set_active(&accounts.accounts[0].uuid)
            .await
            .unwrap();
        runner.browser.hold_install.store(true, Ordering::SeqCst);
        let first = core.browse_plan(into(&instance.id)).await.unwrap();
        let second = core.browse_plan(into(&instance.id)).await.unwrap();
        let task = {
            let core = core.clone();
            let id = first.plan_id.clone();
            tokio::spawn(async move { core.browse_install(&id).await })
        };
        while runner.browser.installs.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert_eq!(
            core.play(&instance.id).await.unwrap_err().code,
            ErrorCode::ContentInstalling
        );
        assert_eq!(
            core.browse_install(&second.plan_id).await.unwrap_err().code,
            ErrorCode::ContentInstalling
        );
        runner.browser.open_install_gate();
        task.await.unwrap().unwrap();
        assert!(core.installing.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_new_instance_plan_creates_the_instance_first() {
        let (core, runner, _, _t) = core(BuildKind::Dev);
        let view = core
            .browse_plan(BrowsePlanRequest {
                project: "sodium".into(),
                version: None,
                target: PlanTargetRequest::New,
            })
            .await
            .unwrap();
        assert!(
            matches!(&view.target, PlanTargetView::New { minecraft, .. } if minecraft == "1.21.1")
        );
        assert!(
            runner.browser.identified.lock().unwrap().is_empty(),
            "nothing to identify yet"
        );
        let result = core.browse_install(&view.plan_id).await.unwrap();
        assert_eq!(result.instance.name, "Sodium");
        assert_eq!(result.instance.game.minecraft, "1.21.1");
        assert_eq!(
            result.instance.game.loader,
            Some(LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.14".into()
            })
        );
        let (dir, _) = runner.browser.installs.lock().unwrap()[0].clone();
        assert_eq!(dir, core.instances.game_dir(&result.instance.id));

        // A failed install keeps the new instance, so the player can retry from it.
        *runner.browser.install_error.lock().unwrap() = Some(LauncherError::network(
            ErrorCode::HostUnreachable,
            "offline",
        ));
        let again = core
            .browse_plan(BrowsePlanRequest {
                project: "sodium".into(),
                version: None,
                target: PlanTargetRequest::New,
            })
            .await
            .unwrap();
        core.browse_install(&again.plan_id).await.unwrap_err();
        let listing = core.instances_list().await.unwrap();
        assert_eq!(listing.instances.len(), 2);
        assert!(core.installing.lock().unwrap().is_empty());
        for kept in &listing.instances {
            InstanceLock::try_acquire(&core.instances.perland_dir(&kept.id)).unwrap();
        }
    }
}

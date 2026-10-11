//! What installing a project means (spec §3.4): the version that fits, its required
//! dependencies (and a shader mod for shaders), what's already there, and what can't work.
use std::{
    collections::{HashSet, VecDeque},
    future::Future,
};

use serde::Serialize;

use super::{
    api::{Api, ApiProject, ApiVersion, DependencyType},
    compat::{file_hashes, fits, new_target, pick, shader_mod, usable_file, ContentKind, Target},
    identify::Installed,
    project::fetch_versions,
};
use crate::{
    download::Hashes,
    error::{ErrorCode, LauncherError, Result},
    instances::MAX_NAME_CHARS,
    meta::loaders::LoaderKind,
};

pub const MAX_DEPTH: usize = 8;
pub const MAX_ITEMS: usize = 50;

/// Where the planner looks things up. `Api` in the app; a fake in the tests.
pub trait Catalog: Send + Sync {
    fn project<'a>(&'a self, id: &'a str) -> impl Future<Output = Result<ApiProject>> + Send + 'a;
    fn versions<'a>(
        &'a self,
        project_id: &'a str,
        target: Option<&'a Target>,
        kind: ContentKind,
    ) -> impl Future<Output = Result<Vec<ApiVersion>>> + Send + 'a;
    fn version<'a>(&'a self, id: &'a str) -> impl Future<Output = Result<ApiVersion>> + Send + 'a;
}

impl Catalog for Api {
    fn project<'a>(&'a self, id: &'a str) -> impl Future<Output = Result<ApiProject>> + Send + 'a {
        Api::project(self, id)
    }
    fn versions<'a>(
        &'a self,
        project_id: &'a str,
        target: Option<&'a Target>,
        kind: ContentKind,
    ) -> impl Future<Output = Result<Vec<ApiVersion>>> + Send + 'a {
        fetch_versions(self, project_id, kind, target)
    }
    fn version<'a>(&'a self, id: &'a str) -> impl Future<Output = Result<ApiVersion>> + Send + 'a {
        Api::version(self, id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRequest {
    pub project: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemReason {
    Requested,
    Dependency { of: String },
    ShaderMod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    Installed,
    InPack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSkip {
    pub project_id: String,
    pub title: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub url: String,
    pub file_name: String,
    pub size: u64,
    pub hashes: Hashes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub project_id: String,
    pub title: String,
    pub kind: ContentKind,
    pub version_id: String,
    pub version_number: String,
    pub reason: ItemReason,
    pub file: PlannedFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewInstancePlan {
    pub name: String,
    pub minecraft: String,
    pub loader: Option<LoaderKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    /// What everything was resolved against (the new instance's game for `new_instance`).
    pub target: Target,
    pub new_instance: Option<NewInstancePlan>,
    /// The requested project first, then dependencies in the order they were resolved.
    pub items: Vec<PlanItem>,
    pub skipped: Vec<PlanSkip>,
}

fn incompatible(title: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::ContentIncompatible,
        format!("{title} doesn't have a version for this instance"),
    )
    .with_param("name", title)
}

fn needs_loader() -> LauncherError {
    LauncherError::pack(
        ErrorCode::ContentNeedsLoader,
        "This needs a mod loader. Pick an instance with Fabric, Quilt, Forge or NeoForge",
    )
}

fn dependency_missing(name: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::ContentDependencyMissing,
        format!("It needs {name}, which has no version for this instance"),
    )
    .with_param("name", name)
}

fn conflict(name: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::ContentConflict,
        format!("It doesn't work together with {name}"),
    )
    .with_param("name", name)
}

fn already_installed(name: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::ContentAlreadyInstalled,
        format!("{name} is already in this instance"),
    )
    .with_param("name", name)
}

fn item(
    project: &ApiProject,
    kind: ContentKind,
    v: &ApiVersion,
    reason: ItemReason,
) -> Result<PlanItem> {
    let file = usable_file(v, kind).ok_or_else(|| incompatible(&project.title))?;
    Ok(PlanItem {
        project_id: project.id.clone(),
        title: project.title.clone(),
        kind,
        version_id: v.id.clone(),
        version_number: v.version_number.clone(),
        reason,
        file: PlannedFile {
            url: file.url.clone(),
            file_name: file.filename.clone(),
            size: file.size,
            hashes: file_hashes(file),
        },
    })
}

/// A dependency waiting to be resolved: by project (id or slug), by version, or both.
struct Pending {
    project: Option<String>,
    version: Option<String>,
    reason: ItemReason,
    depth: usize,
}

struct Planner<'a, C: Catalog> {
    catalog: &'a C,
    target: &'a Target,
    installed: &'a Installed,
    items: Vec<PlanItem>,
    skipped: Vec<PlanSkip>,
    seen: HashSet<String>,
}

impl<C: Catalog> Planner<'_, C> {
    fn enqueue_deps(
        &self,
        queue: &mut VecDeque<Pending>,
        v: &ApiVersion,
        title: &str,
        depth: usize,
    ) {
        for d in v
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == DependencyType::Required)
        {
            queue.push_back(Pending {
                project: d.project_id.clone(),
                version: d.version_id.clone(),
                reason: ItemReason::Dependency {
                    of: title.to_string(),
                },
                depth,
            });
        }
    }

    /// A planned version that can't live with something installed, or the other way round.
    async fn check_conflicts(&self, v: &ApiVersion) -> Result<()> {
        let clash = v
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == DependencyType::Incompatible)
            .filter_map(|d| d.project_id.as_ref())
            .find(|p| self.installed.projects.contains_key(*p))
            .cloned()
            .or_else(|| {
                self.installed
                    .projects
                    .iter()
                    .find(|(_, i)| i.incompatible.contains(&v.project_id))
                    .map(|(id, _)| id.clone())
            });
        match clash {
            None => Ok(()),
            Some(id) => {
                let name = self
                    .catalog
                    .project(&id)
                    .await
                    .map(|p| p.title)
                    .unwrap_or(id);
                Err(conflict(&name))
            }
        }
    }

    /// The shader mod to bring: Iris first, then Oculus (NeoForge 1.20.1 has no Iris build but
    /// runs Forge's Oculus). One that is already installed wins, so it ends up as a skip.
    async fn pick_shader_mod(&self, preferred: &str) -> Result<String> {
        let candidates: &[&str] = if preferred == "iris" {
            &["iris", "oculus"]
        } else {
            &[preferred]
        };
        let mut first_title: Option<String> = None;
        for slug in candidates {
            let project = match self.catalog.project(slug).await {
                Ok(p) => p,
                Err(e) if e.code == ErrorCode::ContentNotFound => continue,
                Err(e) => return Err(e),
            };
            first_title.get_or_insert_with(|| project.title.clone());
            if self.installed.projects.contains_key(&project.id) {
                return Ok((*slug).to_string());
            }
            let list = self
                .catalog
                .versions(&project.id, Some(self.target), ContentKind::Mod)
                .await?;
            if list.iter().any(|v| fits(v, ContentKind::Mod, self.target)) {
                return Ok((*slug).to_string());
            }
        }
        Err(dependency_missing(
            first_title.as_deref().unwrap_or(preferred),
        ))
    }

    async fn add(&mut self, next: Pending, queue: &mut VecDeque<Pending>) -> Result<()> {
        if next.project.as_ref().is_some_and(|p| self.seen.contains(p)) {
            return Ok(());
        }
        // A version that is gone falls back to the project's newest fitting one.
        let pinned = match &next.version {
            Some(id) => match self.catalog.version(id).await {
                Ok(v) => Some(v),
                Err(e) if e.code == ErrorCode::ContentNotFound => {
                    if next.project.is_none() {
                        return Err(dependency_missing(id));
                    }
                    None
                }
                Err(e) => return Err(e),
            },
            None => None,
        };
        let project_ref = match (&next.project, &pinned) {
            (Some(p), _) => p.clone(),
            (None, Some(v)) => v.project_id.clone(),
            (None, None) => return Ok(()),
        };
        let project = match self.catalog.project(&project_ref).await {
            Ok(p) => p,
            Err(e) if e.code == ErrorCode::ContentNotFound => {
                return Err(dependency_missing(&project_ref));
            }
            Err(e) => return Err(e),
        };
        if !self.seen.insert(project.id.clone()) {
            return Ok(());
        }
        if let Some(found) = self.installed.projects.get(&project.id) {
            self.skipped.push(PlanSkip {
                project_id: project.id.clone(),
                title: project.title.clone(),
                reason: if found.managed {
                    SkipReason::InPack
                } else {
                    SkipReason::Installed
                },
            });
            return Ok(());
        }
        if next.depth > MAX_DEPTH || self.items.len() >= MAX_ITEMS {
            return Err(dependency_missing(&project.title));
        }
        let kind = match ContentKind::from_project_type(&project.project_type) {
            Some(ContentKind::Modpack) | None => return Err(dependency_missing(&project.title)),
            Some(kind) => kind,
        };
        let version = match pinned {
            Some(v) if v.project_id == project.id && fits(&v, kind, self.target) => v,
            _ => {
                let list = self
                    .catalog
                    .versions(&project.id, Some(self.target), kind)
                    .await?;
                pick(&list, |v| fits(v, kind, self.target))
                    .cloned()
                    .ok_or_else(|| dependency_missing(&project.title))?
            }
        };
        self.check_conflicts(&version).await?;
        self.items
            .push(item(&project, kind, &version, next.reason)?);
        self.enqueue_deps(queue, &version, &project.title, next.depth + 1);
        Ok(())
    }
}

/// Plan installing `request` into `target` (an instance), or into a new instance when `target`
/// is `None`. `installed` is what the instance already has (empty for a new one).
pub async fn plan<C: Catalog>(
    catalog: &C,
    request: &PlanRequest,
    target: Option<&Target>,
    installed: &Installed,
) -> Result<InstallPlan> {
    let project = catalog.project(&request.project).await?;
    if installed.projects.contains_key(&project.id) {
        return Err(already_installed(&project.title));
    }
    let kind = match ContentKind::from_project_type(&project.project_type) {
        Some(kind) if kind != ContentKind::Modpack && project.client_side != "unsupported" => kind,
        _ => return Err(incompatible(&project.title)),
    };
    if target.is_some_and(|t| t.loader.is_none())
        && matches!(kind, ContentKind::Mod | ContentKind::Shader)
    {
        return Err(needs_loader());
    }
    let pinned = match &request.version {
        Some(id) => {
            let v = catalog.version(id).await?;
            if v.project_id != project.id {
                return Err(incompatible(&project.title));
            }
            Some(v)
        }
        None => None,
    };
    let (version, target, new_instance) = match target {
        Some(t) => {
            let v = match pinned {
                Some(v) if fits(&v, kind, t) => v,
                Some(_) => return Err(incompatible(&project.title)),
                None => {
                    let list = catalog.versions(&project.id, Some(t), kind).await?;
                    pick(&list, |v| fits(v, kind, t))
                        .cloned()
                        .ok_or_else(|| incompatible(&project.title))?
                }
            };
            (v, t.clone(), None)
        }
        None => {
            let v = match pinned {
                Some(v) => v,
                None => {
                    let list = catalog.versions(&project.id, None, kind).await?;
                    pick(&list, |v| new_target(v, kind).is_some())
                        .cloned()
                        .ok_or_else(|| incompatible(&project.title))?
                }
            };
            let t = new_target(&v, kind).ok_or_else(|| incompatible(&project.title))?;
            let name: String = project.title.trim().chars().take(MAX_NAME_CHARS).collect();
            let new = NewInstancePlan {
                name: if name.trim().is_empty() {
                    project.slug.clone()
                } else {
                    name.trim().to_string()
                },
                minecraft: t.minecraft.clone(),
                loader: t.loader,
            };
            (v, t, Some(new))
        }
    };
    let mut planner = Planner {
        catalog,
        target: &target,
        installed,
        items: Vec::new(),
        skipped: Vec::new(),
        seen: HashSet::from([project.id.clone()]),
    };
    planner.check_conflicts(&version).await?;
    planner
        .items
        .push(item(&project, kind, &version, ItemReason::Requested)?);
    let mut queue = VecDeque::new();
    planner.enqueue_deps(&mut queue, &version, &project.title, 1);
    if kind == ContentKind::Shader {
        let slug = shader_mod(target.loader).ok_or_else(needs_loader)?;
        let slug = planner.pick_shader_mod(slug).await?;
        queue.push_back(Pending {
            project: Some(slug),
            version: None,
            reason: ItemReason::ShaderMod,
            depth: 1,
        });
    }
    while let Some(next) = queue.pop_front() {
        planner.add(next, &mut queue).await?;
    }
    Ok(InstallPlan {
        target: target.clone(),
        new_instance,
        items: planner.items,
        skipped: planner.skipped,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::browse::{
        api::{not_found, ApiDependency, ApiFile, ApiHashes},
        identify::InstalledProject,
    };

    #[derive(Default)]
    struct FakeCatalog {
        projects: HashMap<String, ApiProject>,
        versions: HashMap<String, Vec<ApiVersion>>,
        version_requests: std::sync::atomic::AtomicUsize,
    }

    impl FakeCatalog {
        fn add(&mut self, id: &str, title: &str, kind: &str, versions: Vec<ApiVersion>) {
            let project = ApiProject {
                id: id.into(),
                slug: id.to_lowercase(),
                title: title.into(),
                description: String::new(),
                body: String::new(),
                project_type: kind.into(),
                categories: vec![],
                client_side: "required".into(),
                server_side: "optional".into(),
                downloads: 0,
                followers: 0,
                icon_url: None,
                license: None,
                source_url: None,
                issues_url: None,
                wiki_url: None,
                gallery: vec![],
            };
            self.projects.insert(project.slug.clone(), project.clone());
            self.projects.insert(id.into(), project);
            self.versions.insert(id.into(), versions);
        }
    }

    impl Catalog for FakeCatalog {
        async fn project(&self, id: &str) -> Result<ApiProject> {
            self.projects.get(id).cloned().ok_or_else(not_found)
        }
        async fn versions(
            &self,
            project_id: &str,
            _target: Option<&Target>,
            _kind: ContentKind,
        ) -> Result<Vec<ApiVersion>> {
            self.version_requests
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.versions.get(project_id).cloned().unwrap_or_default())
        }
        async fn version(&self, id: &str) -> Result<ApiVersion> {
            self.versions
                .values()
                .flatten()
                .find(|v| v.id == id)
                .cloned()
                .ok_or_else(not_found)
        }
    }

    fn dep(project: &str, kind: DependencyType) -> ApiDependency {
        ApiDependency {
            version_id: None,
            project_id: Some(project.into()),
            dependency_type: kind,
        }
    }

    fn ver(
        id: &str,
        project: &str,
        mc: &[&str],
        loaders: &[&str],
        ext: &str,
        deps: Vec<ApiDependency>,
    ) -> ApiVersion {
        ApiVersion {
            id: id.into(),
            project_id: project.into(),
            name: id.into(),
            version_number: id.into(),
            version_type: "release".into(),
            game_versions: mc.iter().map(|s| s.to_string()).collect(),
            loaders: loaders.iter().map(|s| s.to_string()).collect(),
            date_published: "2026-01-01T00:00:00Z".into(),
            files: vec![ApiFile {
                url: format!("https://cdn.modrinth.com/data/{project}/{id}{ext}"),
                filename: format!("{project}-{id}{ext}"),
                primary: true,
                size: 7,
                hashes: ApiHashes {
                    sha1: Some("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into()),
                    sha512: None,
                },
            }],
            dependencies: deps,
        }
    }

    fn fabric_mod(id: &str, project: &str, deps: Vec<ApiDependency>) -> ApiVersion {
        ver(id, project, &["1.20.1"], &["fabric"], ".jar", deps)
    }

    fn fabric() -> Target {
        Target {
            minecraft: "1.20.1".into(),
            loader: Some(LoaderKind::Fabric),
        }
    }

    fn request(project: &str) -> PlanRequest {
        PlanRequest {
            project: project.into(),
            version: None,
        }
    }

    fn titles(plan: &InstallPlan) -> Vec<&str> {
        plan.items.iter().map(|i| i.title.as_str()).collect()
    }

    fn installed(entries: &[(&str, bool, &[&str])]) -> Installed {
        Installed {
            projects: entries
                .iter()
                .map(|(id, managed, incompatible)| {
                    (
                        id.to_string(),
                        InstalledProject {
                            path: format!("mods/{id}.jar"),
                            managed: *managed,
                            incompatible: incompatible.iter().map(|s| s.to_string()).collect(),
                        },
                    )
                })
                .collect(),
        }
    }

    /// Compile-time check: the future of `plan(&Api, ..)` is `Send` (it ends up in a boxed
    /// `dyn Future + Send`).
    #[allow(dead_code)]
    fn plan_with_the_api_is_send(
        api: &Api,
        request: &PlanRequest,
        target: Option<&Target>,
        installed: &Installed,
    ) {
        fn is_send<T: Send>(_: T) {}
        is_send(plan(api, request, target, installed));
    }

    #[tokio::test]
    async fn a_mod_brings_its_required_dependencies_and_skips_what_is_installed() {
        let mut c = FakeCatalog::default();
        c.add(
            "SOD",
            "Sodium",
            "mod",
            vec![fabric_mod(
                "s1",
                "SOD",
                vec![
                    dep("API", DependencyType::Required),
                    dep("OPT", DependencyType::Optional),
                    dep("EMB", DependencyType::Embedded),
                    dep("PACKED", DependencyType::Required),
                ],
            )],
        );
        c.add(
            "API",
            "Fabric API",
            "mod",
            vec![fabric_mod(
                "a1",
                "API",
                vec![dep("LIB", DependencyType::Required)],
            )],
        );
        c.add("LIB", "Lib", "mod", vec![fabric_mod("l1", "LIB", vec![])]);
        c.add(
            "PACKED",
            "Packed",
            "mod",
            vec![fabric_mod("p1", "PACKED", vec![])],
        );
        c.add(
            "OPT",
            "Optional",
            "mod",
            vec![fabric_mod("o1", "OPT", vec![])],
        );
        let plan = plan(
            &c,
            &request("sod"),
            Some(&fabric()),
            &installed(&[("PACKED", true, &[]), ("LIB", false, &[])]),
        )
        .await
        .unwrap();
        assert_eq!(titles(&plan), vec!["Sodium", "Fabric API"]);
        assert_eq!(plan.items[0].reason, ItemReason::Requested);
        assert_eq!(
            plan.items[1].reason,
            ItemReason::Dependency {
                of: "Sodium".into()
            }
        );
        assert_eq!(plan.items[0].file.file_name, "SOD-s1.jar");
        assert_eq!(
            plan.skipped,
            vec![
                PlanSkip {
                    project_id: "PACKED".into(),
                    title: "Packed".into(),
                    reason: SkipReason::InPack
                },
                PlanSkip {
                    project_id: "LIB".into(),
                    title: "Lib".into(),
                    reason: SkipReason::Installed
                },
            ]
        );
        assert_eq!(plan.new_instance, None);
    }

    #[tokio::test]
    async fn a_diamond_and_a_cycle_plan_each_project_once() {
        let mut c = FakeCatalog::default();
        c.add(
            "A",
            "A",
            "mod",
            vec![fabric_mod(
                "a",
                "A",
                vec![
                    dep("B", DependencyType::Required),
                    dep("C", DependencyType::Required),
                ],
            )],
        );
        c.add(
            "B",
            "B",
            "mod",
            vec![fabric_mod(
                "b",
                "B",
                vec![
                    dep("D", DependencyType::Required),
                    dep("A", DependencyType::Required),
                ],
            )],
        );
        c.add(
            "C",
            "C",
            "mod",
            vec![fabric_mod(
                "c",
                "C",
                vec![dep("D", DependencyType::Required)],
            )],
        );
        c.add("D", "D", "mod", vec![fabric_mod("d", "D", vec![])]);
        let plan = plan(&c, &request("A"), Some(&fabric()), &Installed::default())
            .await
            .unwrap();
        assert_eq!(titles(&plan), vec!["A", "B", "C", "D"]);
        assert!(plan.skipped.is_empty());
    }

    #[tokio::test]
    async fn dependencies_pinned_to_an_unfit_version_take_a_fitting_one_or_fail() {
        let mut c = FakeCatalog::default();
        let pinned = ApiDependency {
            version_id: Some("old".into()),
            project_id: None,
            dependency_type: DependencyType::Required,
        };
        c.add("M", "Mod", "mod", vec![fabric_mod("m", "M", vec![pinned])]);
        c.add(
            "L",
            "Lib",
            "mod",
            vec![
                ver("old", "L", &["1.19.2"], &["fabric"], ".jar", vec![]),
                fabric_mod("good", "L", vec![]),
            ],
        );
        let ok = plan(&c, &request("M"), Some(&fabric()), &Installed::default())
            .await
            .unwrap();
        assert_eq!(ok.items[1].version_id, "good");

        c.add(
            "N",
            "Needy",
            "mod",
            vec![fabric_mod(
                "n",
                "N",
                vec![dep("F", DependencyType::Required)],
            )],
        );
        c.add(
            "F",
            "Forge Thing",
            "mod",
            vec![ver("f", "F", &["1.20.1"], &["forge"], ".jar", vec![])],
        );
        let missing = plan(&c, &request("N"), Some(&fabric()), &Installed::default())
            .await
            .unwrap_err();
        assert_eq!(missing.code, ErrorCode::ContentDependencyMissing);
        assert_eq!(missing.params["name"], "Forge Thing");
    }

    #[tokio::test]
    async fn chains_deeper_than_the_cap_fail() {
        let mut c = FakeCatalog::default();
        // P0 -> P1 -> ... -> P9: P9 sits at depth 9.
        for i in 0..10 {
            let deps = if i < 9 {
                vec![dep(&format!("P{}", i + 1), DependencyType::Required)]
            } else {
                vec![]
            };
            c.add(
                &format!("P{i}"),
                &format!("Part {i}"),
                "mod",
                vec![fabric_mod(&format!("v{i}"), &format!("P{i}"), deps)],
            );
        }
        let error = plan(&c, &request("P0"), Some(&fabric()), &Installed::default())
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ContentDependencyMissing);
        assert_eq!(error.params["name"], "Part 9");
        let ok = plan(&c, &request("P1"), Some(&fabric()), &Installed::default())
            .await
            .unwrap();
        assert_eq!(ok.items.len(), 9);
    }

    #[tokio::test]
    async fn conflicts_are_refused_in_both_directions() {
        let mut c = FakeCatalog::default();
        c.add(
            "OPTI",
            "OptiFabric",
            "mod",
            vec![fabric_mod(
                "o",
                "OPTI",
                vec![dep("SOD", DependencyType::Incompatible)],
            )],
        );
        c.add("SOD", "Sodium", "mod", vec![fabric_mod("s", "SOD", vec![])]);
        c.add(
            "NEW",
            "New Thing",
            "mod",
            vec![fabric_mod("n", "NEW", vec![])],
        );
        let mine = plan(
            &c,
            &request("OPTI"),
            Some(&fabric()),
            &installed(&[("SOD", false, &[])]),
        )
        .await
        .unwrap_err();
        assert_eq!(mine.code, ErrorCode::ContentConflict);
        assert_eq!(mine.params["name"], "Sodium");
        let theirs = plan(
            &c,
            &request("NEW"),
            Some(&fabric()),
            &installed(&[("SOD", false, &["NEW"])]),
        )
        .await
        .unwrap_err();
        assert_eq!(theirs.code, ErrorCode::ContentConflict);
        assert_eq!(theirs.params["name"], "Sodium");
    }

    #[tokio::test]
    async fn shaders_bring_their_shader_mod_and_need_a_loader() {
        let mut c = FakeCatalog::default();
        c.add(
            "SH",
            "Complementary",
            "shader",
            vec![ver("s", "SH", &["1.20.1"], &["iris"], ".zip", vec![])],
        );
        c.add(
            "IRIS",
            "Iris",
            "mod",
            vec![fabric_mod(
                "i",
                "IRIS",
                vec![dep("SOD", DependencyType::Required)],
            )],
        );
        c.projects.insert("iris".into(), c.projects["IRIS"].clone());
        c.add(
            "SOD",
            "Sodium",
            "mod",
            vec![fabric_mod("s1", "SOD", vec![])],
        );
        c.add(
            "OCU",
            "Oculus",
            "mod",
            vec![ver("o", "OCU", &["1.20.1"], &["forge"], ".jar", vec![])],
        );
        c.projects
            .insert("oculus".into(), c.projects["OCU"].clone());
        let on_fabric = plan(&c, &request("SH"), Some(&fabric()), &Installed::default())
            .await
            .unwrap();
        assert_eq!(titles(&on_fabric), vec!["Complementary", "Iris", "Sodium"]);
        assert_eq!(on_fabric.items[1].reason, ItemReason::ShaderMod);
        assert_eq!(on_fabric.items[0].file.file_name, "SH-s.zip");
        let forge = Target {
            minecraft: "1.20.1".into(),
            loader: Some(LoaderKind::Forge),
        };
        assert_eq!(
            titles(
                &plan(&c, &request("SH"), Some(&forge), &Installed::default())
                    .await
                    .unwrap()
            ),
            vec!["Complementary", "Oculus"]
        );
        let vanilla = Target {
            minecraft: "1.20.1".into(),
            loader: None,
        };
        assert_eq!(
            plan(&c, &request("SH"), Some(&vanilla), &Installed::default())
                .await
                .unwrap_err()
                .code,
            ErrorCode::ContentNeedsLoader
        );
        assert_eq!(
            plan(&c, &request("SOD"), Some(&vanilla), &Installed::default())
                .await
                .unwrap_err()
                .code,
            ErrorCode::ContentNeedsLoader
        );
    }

    #[tokio::test]
    async fn a_new_instance_takes_the_newest_release_and_loader() {
        let mut c = FakeCatalog::default();
        c.add(
            "SOD",
            "Sodium".repeat(20).as_str(),
            "mod",
            vec![ver(
                "s",
                "SOD",
                &["1.20.1", "1.21.1", "24w14a"],
                &["quilt", "fabric"],
                ".jar",
                vec![dep("API", DependencyType::Required)],
            )],
        );
        c.add(
            "API",
            "Fabric API",
            "mod",
            vec![
                ver("a-old", "API", &["1.20.1"], &["fabric"], ".jar", vec![]),
                ver("a-new", "API", &["1.21.1"], &["fabric"], ".jar", vec![]),
            ],
        );
        let p = plan(&c, &request("SOD"), None, &Installed::default())
            .await
            .unwrap();
        let new = p.new_instance.clone().unwrap();
        assert_eq!(
            (new.minecraft.as_str(), new.loader),
            ("1.21.1", Some(LoaderKind::Fabric))
        );
        assert_eq!(new.name.chars().count(), 64);
        assert_eq!(
            p.target,
            Target {
                minecraft: "1.21.1".into(),
                loader: Some(LoaderKind::Fabric)
            }
        );
        assert_eq!(
            p.items[1].version_id, "a-new",
            "dependencies resolve against the new instance"
        );

        c.add(
            "RP",
            "Faithful",
            "resourcepack",
            vec![ver("r", "RP", &["1.21.1"], &["minecraft"], ".zip", vec![])],
        );
        let rp = plan(&c, &request("RP"), None, &Installed::default())
            .await
            .unwrap();
        assert_eq!(rp.new_instance.unwrap().loader, None);
    }

    #[tokio::test]
    async fn refusals_for_installed_modpacks_and_wrong_versions() {
        let mut c = FakeCatalog::default();
        c.add("SOD", "Sodium", "mod", vec![fabric_mod("s", "SOD", vec![])]);
        c.add(
            "OTHER",
            "Other",
            "mod",
            vec![fabric_mod("x", "OTHER", vec![])],
        );
        c.add(
            "PACK",
            "Pack",
            "modpack",
            vec![ver(
                "p",
                "PACK",
                &["1.20.1"],
                &["fabric"],
                ".mrpack",
                vec![],
            )],
        );
        let again = plan(
            &c,
            &request("SOD"),
            Some(&fabric()),
            &installed(&[("SOD", false, &[])]),
        )
        .await
        .unwrap_err();
        assert_eq!(again.code, ErrorCode::ContentAlreadyInstalled);
        assert_eq!(again.params["name"], "Sodium");
        assert_eq!(
            plan(&c, &request("PACK"), Some(&fabric()), &Installed::default())
                .await
                .unwrap_err()
                .code,
            ErrorCode::ContentIncompatible
        );
        let foreign = PlanRequest {
            project: "SOD".into(),
            version: Some("x".into()),
        };
        assert_eq!(
            plan(&c, &foreign, Some(&fabric()), &Installed::default())
                .await
                .unwrap_err()
                .code,
            ErrorCode::ContentIncompatible
        );
        // Already installed wins over "no version fits", and no versions are asked for.
        let before = c.version_requests.load(std::sync::atomic::Ordering::SeqCst);
        let old = Target {
            minecraft: "1.16.5".into(),
            loader: Some(LoaderKind::Fabric),
        };
        let held = plan(
            &c,
            &request("SOD"),
            Some(&old),
            &installed(&[("SOD", false, &[])]),
        )
        .await
        .unwrap_err();
        assert_eq!(held.code, ErrorCode::ContentAlreadyInstalled);
        assert_eq!(
            c.version_requests.load(std::sync::atomic::Ordering::SeqCst),
            before
        );
        let old_game = Target {
            minecraft: "1.16.5".into(),
            loader: Some(LoaderKind::Fabric),
        };
        let none = plan(&c, &request("SOD"), Some(&old_game), &Installed::default())
            .await
            .unwrap_err();
        assert_eq!(none.code, ErrorCode::ContentIncompatible);
        assert_eq!(none.params["name"], "Sodium");
        let mut server_only = c.projects["SOD"].clone();
        server_only.client_side = "unsupported".into();
        c.projects.insert("SOD".into(), server_only);
        assert_eq!(
            plan(&c, &request("SOD"), Some(&fabric()), &Installed::default())
                .await
                .unwrap_err()
                .code,
            ErrorCode::ContentIncompatible
        );
    }

    #[tokio::test]
    async fn a_gone_dependency_is_reported_by_name_not_as_not_found() {
        let mut c = FakeCatalog::default();
        // Project known, pinned version gone: take the project's newest fitting version.
        let gone_version = ApiDependency {
            version_id: Some("ghost".into()),
            project_id: Some("L".into()),
            dependency_type: DependencyType::Required,
        };
        c.add(
            "M",
            "Mod",
            "mod",
            vec![fabric_mod("m", "M", vec![gone_version])],
        );
        c.add("L", "Lib", "mod", vec![fabric_mod("good", "L", vec![])]);
        let ok = plan(&c, &request("M"), Some(&fabric()), &Installed::default())
            .await
            .unwrap();
        assert_eq!(titles(&ok), vec!["Mod", "Lib"]);
        assert_eq!(ok.items[1].version_id, "good");

        // Project gone: the dependency is missing, named by its ref.
        c.add(
            "N",
            "Needy",
            "mod",
            vec![fabric_mod(
                "n",
                "N",
                vec![dep("GONE", DependencyType::Required)],
            )],
        );
        let missing = plan(&c, &request("N"), Some(&fabric()), &Installed::default())
            .await
            .unwrap_err();
        assert_eq!(missing.code, ErrorCode::ContentDependencyMissing);
        assert_eq!(missing.params["name"], "GONE");

        // Only a gone version id: named by that id.
        let only_version = ApiDependency {
            version_id: Some("ghost".into()),
            project_id: None,
            dependency_type: DependencyType::Required,
        };
        c.add(
            "P",
            "Pinned",
            "mod",
            vec![fabric_mod("p", "P", vec![only_version])],
        );
        let missing = plan(&c, &request("P"), Some(&fabric()), &Installed::default())
            .await
            .unwrap_err();
        assert_eq!(missing.code, ErrorCode::ContentDependencyMissing);
        assert_eq!(missing.params["name"], "ghost");
    }

    #[tokio::test]
    async fn a_shader_falls_back_to_oculus_when_iris_has_no_fitting_build() {
        let mut c = FakeCatalog::default();
        c.add(
            "SH",
            "Complementary",
            "shader",
            vec![ver("s", "SH", &["1.20.1"], &["iris"], ".zip", vec![])],
        );
        c.add(
            "IRIS",
            "Iris",
            "mod",
            vec![ver("i", "IRIS", &["1.21.1"], &["neoforge"], ".jar", vec![])],
        );
        c.projects.insert("iris".into(), c.projects["IRIS"].clone());
        c.add(
            "OCU",
            "Oculus",
            "mod",
            vec![ver("o", "OCU", &["1.20.1"], &["forge"], ".jar", vec![])],
        );
        c.projects
            .insert("oculus".into(), c.projects["OCU"].clone());
        let neoforge = Target {
            minecraft: "1.20.1".into(),
            loader: Some(LoaderKind::NeoForge),
        };
        let p = plan(&c, &request("SH"), Some(&neoforge), &Installed::default())
            .await
            .unwrap();
        assert_eq!(titles(&p), vec!["Complementary", "Oculus"]);
        assert_eq!(p.items[1].reason, ItemReason::ShaderMod);

        // Neither fits: Iris is named.
        let older = Target {
            minecraft: "1.19.2".into(),
            loader: Some(LoaderKind::NeoForge),
        };
        c.add(
            "SH2",
            "Other Shader",
            "shader",
            vec![ver("s2", "SH2", &["1.19.2"], &["iris"], ".zip", vec![])],
        );
        let none = plan(&c, &request("SH2"), Some(&older), &Installed::default())
            .await
            .unwrap_err();
        assert_eq!(none.code, ErrorCode::ContentDependencyMissing);
        assert_eq!(none.params["name"], "Iris");

        // Iris already installed: it is a skip, Oculus is not brought.
        let have = installed(&[("IRIS", false, &[])]);
        let p = plan(&c, &request("SH"), Some(&neoforge), &have)
            .await
            .unwrap();
        assert_eq!(titles(&p), vec!["Complementary"]);
        assert_eq!(p.skipped.len(), 1);
    }
}

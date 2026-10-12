//! Browse commands (spec §4): search, project pages, images, what an instance has, and (in
//! Task 14) planned installs. All Modrinth work happens behind `Runner::browser`.
use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    browse::{
        compat::{ContentKind, Target},
        images::ImageKind,
        project::{ProjectDetail, VersionInfo},
        search::{SearchPage, SearchQuery},
    },
    sync::lockfile::Managed,
    ErrorCode, LauncherError, Result,
};

use crate::{app::AppCore, dto::SearchRequest, events::Host, runner::Runner};

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
        app::tests::core,
        dto::{NewInstance, SearchRequest},
        testing::managed_for,
    };

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
        for bad in [
            "http://example.com",
            "javascript:alert(1)",
            "file:///C:/Windows/system32/calc.exe",
            "https://user:pw@example.com/",
            "perland://pack?url=https://x",
            "https:///nohost",
            "",
        ] {
            assert_eq!(
                link_to_open(bad).unwrap_err().code,
                ErrorCode::LinkRefused,
                "{bad}"
            );
        }
    }
}

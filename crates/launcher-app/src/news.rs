//! A server's news on its card (server-news spec §5): one page of posts, the "seen" marker, and
//! a post's picture. The invite is the credential, so it never reaches a log or an error here.
use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    invite::InviteLink,
    news::{NameList, NewsPost},
    Result,
};

use crate::{
    app::AppCore,
    dto::{NameListView, NewsChangesView, NewsPostView, ServerNews},
    events::Host,
    runner::Runner,
};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

impl<R: Runner, H: Host> AppCore<R, H> {
    /// One page of a server's news (`before` is the previous page's `next_before`), with the
    /// player's seen marker. `Ok(None)` is "nothing to show": the server has no news, news are
    /// off, or `invite` isn't an invite. Errors are the Engine's own (`invite_rate_limited`).
    pub async fn server_news(
        &self,
        invite: &str,
        before: Option<String>,
        limit: u8,
    ) -> Result<Option<ServerNews>> {
        let Ok(link) = InviteLink::parse(invite) else {
            return Ok(None);
        };
        let canonical = link.canonical();
        let Some(page) = self.runner.news_page(link, limit, before).await? else {
            return Ok(None);
        };
        Ok(Some(ServerNews {
            posts: page.posts.iter().map(view).collect(),
            next_before: page.next_before,
            seen: self.news_seen.seen(&canonical).await,
        }))
    }

    /// The player opened the news: remember `newest` (the `createdAt` of the newest post they
    /// saw) for this server. The marker only moves forward.
    pub async fn server_news_opened(&self, invite: &str, newest: &str) -> Result<()> {
        let link = InviteLink::parse(invite)?;
        self.news_seen.mark_seen(&link.canonical(), newest).await
    }

    /// A post's picture as a `data:` URL (PNG or JPEG, by its signature), or `None`.
    pub async fn news_image(
        &self,
        invite: &str,
        post_id: &str,
        image_version: &str,
    ) -> Option<String> {
        let link = InviteLink::parse(invite).ok()?;
        let bytes = self
            .runner
            .news_image(link, post_id.to_string(), image_version.to_string())
            .await?;
        let mime = if bytes.starts_with(PNG_SIGNATURE) {
            "image/png"
        } else {
            "image/jpeg"
        };
        Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
    }
}

fn view(post: &NewsPost) -> NewsPostView {
    let kind = match post.kind.as_str() {
        "event" => "event",
        "modpack" => "modpack",
        _ => "update",
    };
    NewsPostView {
        id: post.id.clone(),
        kind: kind.to_string(),
        title: post.title.clone(),
        body: post.body.clone(),
        starts_at: post.starts_at.clone(),
        ends_at: post.ends_at.clone(),
        image_version: post.image_version.clone(),
        modpack: post
            .modpack
            .as_ref()
            .and_then(|m| m.changes.as_ref())
            .map(|c| NewsChangesView {
                added: names(&c.added),
                removed: names(&c.removed),
                updated: names(&c.updated),
            }),
        created_at: post.created_at.clone(),
    }
}

fn names(list: &NameList) -> NameListView {
    NameListView {
        count: list.count,
        names: list.names.clone(),
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        news::{NameList, NewsChanges, NewsModpack, NewsPage, NewsPost},
        ErrorCode, LauncherError,
    };

    use crate::{
        app::tests::core,
        testing::{invite, FakeRunner},
    };

    const POST: &str = "3f2b8c1e-5d4a-4e6b-9c7d-1a2b3c4d5e6f";

    fn post(id: &str, kind: &str, created_at: &str) -> NewsPost {
        NewsPost {
            id: id.into(),
            kind: kind.into(),
            title: "Title".into(),
            body: "Line one\nLine two".into(),
            created_at: created_at.into(),
            ..NewsPost::default()
        }
    }

    fn serve(runner: &FakeRunner, posts: Vec<NewsPost>, next_before: Option<&str>) {
        *runner.news_page.lock().unwrap() = Some(Ok(Some(NewsPage {
            posts,
            next_before: next_before.map(str::to_string),
        })));
    }

    #[tokio::test]
    async fn posts_map_to_views() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut event = post(POST, "event", "2026-10-15T10:00:00Z");
        event.starts_at = Some("2026-10-20T18:00:00Z".into());
        event.ends_at = Some("2026-10-20T20:00:00Z".into());
        event.image_version = Some("v2".into());
        let weird = post("p2", "announcement", "2026-10-14T10:00:00Z");
        let mut with_changes = post("p3", "modpack", "2026-10-13T10:00:00Z");
        with_changes.modpack = Some(NewsModpack {
            changes: Some(NewsChanges {
                added: NameList {
                    count: 3,
                    names: vec!["Sodium".into(), "Iris".into()],
                },
                removed: NameList {
                    count: 1,
                    names: vec!["Zoomify".into()],
                },
                updated: NameList::default(),
            }),
        });
        let mut without_changes = post("p4", "modpack", "2026-10-12T10:00:00Z");
        without_changes.modpack = Some(NewsModpack { changes: None });
        serve(
            &runner,
            vec![event, weird, with_changes, without_changes],
            Some("41"),
        );
        let news = core
            .server_news(&invite('A'), None, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(news.next_before.as_deref(), Some("41"));
        assert_eq!(news.posts.len(), 4);
        let e = &news.posts[0];
        assert_eq!(e.kind, "event");
        assert_eq!(e.starts_at.as_deref(), Some("2026-10-20T18:00:00Z"));
        assert_eq!(e.ends_at.as_deref(), Some("2026-10-20T20:00:00Z"));
        assert_eq!(e.image_version.as_deref(), Some("v2"));
        assert_eq!(e.body, "Line one\nLine two");
        assert_eq!(news.posts[1].kind, "update", "an unknown type is an update");
        let m = news.posts[2].modpack.as_ref().unwrap();
        assert_eq!(m.added.count, 3);
        assert_eq!(m.added.names, ["Sodium", "Iris"]);
        assert_eq!(m.removed.count, 1);
        assert_eq!(m.updated.count, 0);
        assert_eq!(news.posts[3].kind, "modpack");
        assert!(news.posts[3].modpack.is_none());
    }

    #[tokio::test]
    async fn the_request_reaches_the_runner() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, vec![], None);
        core.server_news(&invite('A'), Some("41".into()), 20)
            .await
            .unwrap();
        assert_eq!(
            runner.news_calls.lock().unwrap().as_slice(),
            [(invite('A'), 20, Some("41".to_string()))]
        );
    }

    #[tokio::test]
    async fn seen_comes_from_the_file() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(
            &runner,
            vec![post(POST, "update", "2026-10-15T10:00:00Z")],
            None,
        );
        let news = core
            .server_news(&invite('A'), None, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(news.seen, None);
        core.news_seen
            .mark_seen(&invite('A'), "2026-10-15T00:00:00Z")
            .await
            .unwrap();
        let news = core
            .server_news(&invite('A'), None, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(news.seen.as_deref(), Some("2026-10-15T00:00:00Z"));
        // Another server has its own marker.
        let other = core
            .server_news(&invite('B'), None, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(other.seen, None);
    }

    #[tokio::test]
    async fn opened_moves_the_marker() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(
            &runner,
            vec![post(POST, "update", "2026-10-15T10:00:00Z")],
            None,
        );
        core.server_news_opened(&invite('A'), "2026-10-15T10:00:00Z")
            .await
            .unwrap();
        let news = core
            .server_news(&invite('A'), None, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(news.seen.as_deref(), Some("2026-10-15T10:00:00Z"));
        // It only moves forward.
        core.server_news_opened(&invite('A'), "2026-10-01T10:00:00Z")
            .await
            .unwrap();
        let news = core
            .server_news(&invite('A'), None, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(news.seen.as_deref(), Some("2026-10-15T10:00:00Z"));
    }

    #[tokio::test]
    async fn opened_with_a_bad_invite_is_invalid() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let err = core
            .server_news_opened("modrinth:sky", "2026-10-15T10:00:00Z")
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteLinkInvalid);
    }

    #[tokio::test]
    async fn no_news_stays_none() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        // Nothing served: the fake answers "no news here".
        assert!(core
            .server_news(&invite('A'), None, 20)
            .await
            .unwrap()
            .is_none());
        *runner.news_page.lock().unwrap() = Some(Ok(None));
        assert!(core
            .server_news(&invite('A'), None, 20)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn errors_pass_through() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        *runner.news_page.lock().unwrap() = Some(Err(LauncherError::pack(
            ErrorCode::InviteRateLimited,
            "slow down",
        )));
        let err = core
            .server_news(&invite('A'), Some("41".into()), 20)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteRateLimited);
    }

    #[tokio::test]
    async fn a_bad_invite_is_no_news() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(
            &runner,
            vec![post(POST, "update", "2026-10-15T10:00:00Z")],
            None,
        );
        for bad in ["https://play.example.com/short", "modrinth:sky", ""] {
            assert!(
                core.server_news(bad, None, 20).await.unwrap().is_none(),
                "{bad}"
            );
        }
        assert!(runner.news_calls.lock().unwrap().is_empty());
        assert_eq!(core.news_image("modrinth:sky", POST, "v1").await, None);
        assert!(runner.news_image_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_image_is_a_data_url() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        assert_eq!(core.news_image(&invite('A'), POST, "v1").await, None);
        *runner.news_image.lock().unwrap() = Some(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec());
        let png = core.news_image(&invite('A'), POST, "v1").await.unwrap();
        assert!(
            png.starts_with("data:image/png;base64,iVBORw0KGgo"),
            "{png}"
        );
        *runner.news_image.lock().unwrap() = Some(b"\xFF\xD8\xFF\xE0\0\x10JFIF".to_vec());
        let jpeg = core.news_image(&invite('A'), POST, "v2").await.unwrap();
        assert!(jpeg.starts_with("data:image/jpeg;base64,/9j/"), "{jpeg}");
        let versions: Vec<String> = runner
            .news_image_calls
            .lock()
            .unwrap()
            .iter()
            .map(|(link, post, version)| {
                assert_eq!(link, &invite('A'));
                assert_eq!(post, POST);
                version.clone()
            })
            .collect();
        assert_eq!(versions, ["v1", "v1", "v2"]);
    }
}

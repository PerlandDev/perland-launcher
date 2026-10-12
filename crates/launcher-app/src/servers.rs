//! Joining Perland servers by invite link (join-by-link spec §2.2): previewing an invite,
//! adding it as an instance, its live status, and moving it to a new link. Tauri only forwards
//! these commands.
use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    instances::{unix_now, Instance, ServerLink, SourceRef, UpdatePolicy},
    invite::InviteLink,
    sources::{perland, PackInput},
    Result,
};

use crate::{app::AppCore, dto::JoinPreview, events::Host, runner::Runner};

/// A server instance's saved invite, and the Engine it last used.
pub(crate) fn server_source(instance: &Instance) -> Option<(InviteLink, String)> {
    match &instance.source {
        SourceRef::Perland { invite, engine } => InviteLink::parse(invite)
            .ok()
            .map(|link| (link, engine.clone())),
        _ => None,
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The instance that already joins `link`'s server, compared by canonical invite.
    pub(crate) async fn instance_for_invite(&self, link: &InviteLink) -> Result<Option<Instance>> {
        let canonical = link.canonical();
        Ok(self.instances.list().await?.into_iter().find(|instance| {
            server_source(instance).is_some_and(|(saved, _)| saved.canonical() == canonical)
        }))
    }

    /// What Join server shows: the view and the icon. Nothing else is downloaded, and nothing
    /// is written.
    pub async fn join_preview(&self, link: &str) -> Result<JoinPreview> {
        let link = InviteLink::parse(link)?;
        let ev = self.runner.invite_view(link.clone(), None).await?;
        perland::supported(&ev)?;
        let icon = self
            .runner
            .invite_icon(link.clone())
            .await
            .map(|png| format!("data:image/png;base64,{}", STANDARD.encode(png)));
        let existing = self.instance_for_invite(&link).await?.map(|i| i.id);
        Ok(JoinPreview::new(&link, &ev, icon, existing))
    }

    /// Add the server as an instance, or return the one that already joins it. Nothing
    /// installs: the first Play does that.
    pub async fn join_add(&self, link: &str) -> Result<Instance> {
        let link = InviteLink::parse(link)?;
        if let Some(existing) = self.instance_for_invite(&link).await? {
            return Ok(existing);
        }
        let created = self
            .runner
            .add_pack(PackInput::Perland { link: link.clone() })
            .await?;
        let mut instance = self.instances.load(&created.id).await?;
        instance.update_policy = UpdatePolicy::Auto;
        // The view was just read by `add_pack`, so this is the shared copy.
        if let Ok(ev) = self.runner.invite_view(link, None).await {
            instance.server = Some(ServerLink::from_view(&ev.view, &ev.engine, unix_now()));
        }
        self.instances.save(&instance).await?;
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        instances::{ServerStatus, SourceRef, UpdatePolicy},
        ErrorCode,
    };

    use crate::{
        app::tests::core,
        testing::{invite, serve, server_view},
    };

    #[tokio::test]
    async fn join_preview_reads_the_invite_and_writes_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        // A PNG's signature, then its IHDR chunk's length and type.
        *runner.icon.lock().unwrap() = Some(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec());
        let preview = core.join_preview(&invite('A')).await.unwrap();
        assert_eq!(preview.host, "play.example.com");
        assert_eq!(preview.name, "survival");
        assert_eq!(preview.kind, "stack");
        assert_eq!(preview.status, Some(ServerStatus::Online));
        assert_eq!(preview.players.as_ref().unwrap().max, Some(20));
        assert_eq!(preview.pack.as_ref().unwrap().mod_count, 2);
        assert_eq!(
            preview.team.as_ref().unwrap().accent.as_deref(),
            Some("violet")
        );
        assert_eq!(
            preview.address.as_ref().unwrap().display(),
            "play.example.com:25566"
        );
        assert!(preview
            .icon
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert_eq!(preview.existing, None);
        assert!(core.instances_list().await.unwrap().instances.is_empty());
        assert!(runner.added.lock().unwrap().is_empty(), "nothing is added");
    }

    #[tokio::test]
    async fn join_preview_refuses_networks_with_their_address() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut network = server_view("hub");
        network.view.kind = "network".into();
        network.view.pack = None;
        network.view.address.port = 25577;
        serve(&runner, &invite('A'), Ok(network));
        let err = core.join_preview(&invite('A')).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteUnsupported);
        assert_eq!(err.params["address"], "play.example.com:25577");
    }

    /// Ruling R1: a modded stack that serves no pack can't be joined (never a vanilla instance).
    #[tokio::test]
    async fn join_preview_refuses_a_modded_stack_without_a_pack() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut modded = server_view("survival");
        modded.view.pack = None;
        assert_eq!(modded.view.game.loader.as_deref(), Some("fabric"));
        serve(&runner, &invite('A'), Ok(modded));
        let err = core.join_preview(&invite('A')).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteUnsupported);
        assert_eq!(err.params["address"], "play.example.com:25566");
    }

    #[tokio::test]
    async fn junk_is_not_an_invite() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        for bad in ["https://play.example.com/short", "modrinth:sky", ""] {
            assert_eq!(
                core.join_preview(bad).await.unwrap_err().code,
                ErrorCode::InviteLinkInvalid,
                "{bad}"
            );
            assert_eq!(
                core.join_add(bad).await.unwrap_err().code,
                ErrorCode::InviteLinkInvalid,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn join_add_creates_an_automatic_server_instance() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let instance = core.join_add(&invite('A')).await.unwrap();
        assert_eq!(instance.update_policy, UpdatePolicy::Auto);
        assert!(
            matches!(&instance.source, SourceRef::Perland { invite: i, .. } if *i == invite('A'))
        );
        let server = instance.server.clone().unwrap();
        assert_eq!(server.name, "survival");
        assert_eq!(server.accent.as_deref(), Some("violet"));
        assert_eq!(server.last_status, Some(ServerStatus::Online));
        assert_eq!(server.address.unwrap().display(), "play.example.com:25566");
        assert!(!server.gone);
        assert_eq!(core.instances.load(&instance.id).await.unwrap(), instance);
        let preview = core.join_preview(&invite('A')).await.unwrap();
        assert_eq!(preview.existing.as_deref(), Some(instance.id.as_str()));
    }

    #[tokio::test]
    async fn join_add_dedupes_case_and_slash_variants() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let first = core.join_add(&invite('A')).await.unwrap();
        let shouted = format!(
            "  {}/  ",
            invite('A').replace("play.example.com", "PLAY.Example.com:443")
        );
        let second = core.join_add(&shouted).await.unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(runner.added.lock().unwrap().len(), 1);
    }
}

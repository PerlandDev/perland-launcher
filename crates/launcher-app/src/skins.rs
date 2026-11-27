//! The Skins screen: each account's wardrobe on disk, an account's current skin, and wearing a
//! skin. Textures cross to the webview as `data:image/png;base64,` URLs. `owner` is the account
//! whose wardrobe is shown; `None` is the loose wardrobe, used while no account is signed in.
use launcher_core::telemetry::Counter;
use std::path::PathBuf;

use launcher_core::{
    accounts::{Account, Provider},
    msa::minecraft::McToken,
    skin_catalog::{CatalogStatus, Manifest},
    skins,
    skins_library::{SkinEntry, SkinLook, SkinModel, SkinVersion, VersionReason},
    skins_png, ErrorCategory, ErrorCode, LauncherError, Result,
};
use serde::Serialize;

use crate::{
    app::{AppCore, SKIN_MAX_AGE},
    events::Host,
    runner::Runner,
};

/// The skin an account wears now, ready for the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentSkin {
    pub texture: String,
    pub model: SkinModel,
}

/// The skin the launcher draws as the player's own figure (the Profile), and how many skins
/// the account's wardrobe holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSkin {
    /// A PNG data URL; `None` when the account has nothing of its own to show, and the UI draws
    /// the launcher's starter.
    pub texture: Option<String>,
    pub model: SkinModel,
    pub wardrobe: usize,
}

/// The skin catalog and how it was reached: `manifest` is `None` when there is nothing to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogAnswer {
    pub manifest: Option<Manifest>,
    pub status: CatalogStatus,
}

fn offline_account() -> LauncherError {
    LauncherError::new(
        ErrorCategory::Auth,
        ErrorCode::SkinOfflineAccount,
        "Offline accounts can't change their skin in Minecraft.",
    )
    .with_fix("Export the PNG, or wear it from a Microsoft account.")
}

fn no_account(uuid: &str) -> LauncherError {
    LauncherError::auth(
        ErrorCode::NoAccount,
        "That account isn't in the launcher anymore.",
    )
    .with_param("uuid", uuid)
}

/// `path` with a `.png` extension.
fn png_path(path: PathBuf) -> PathBuf {
    let is_png = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"));
    if is_png {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".png");
        PathBuf::from(name)
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The wardrobe of `owner`. The active account first adopts any loose skins: those made with
    /// no account signed in, or kept from when every account shared one wardrobe. A starter left
    /// untouched is dropped instead; `starter` is the launcher's starter (a PNG data URL, drawn
    /// by the UI) to recognise one saved before starters were marked.
    pub async fn skins_list(
        &self,
        owner: Option<&str>,
        starter: Option<&str>,
    ) -> Result<Vec<SkinEntry>> {
        if let Some(owner) = owner {
            let active = self.accounts.lock().await.list().await?.active;
            if active.as_deref() == Some(owner) {
                let starter = starter.and_then(|url| skins_png::from_data_url(url).ok());
                self.skins.adopt_loose(owner, starter.as_deref()).await?;
            }
        }
        self.skins.list(owner).await
    }

    pub async fn skin_texture(&self, id: &str) -> Result<String> {
        Ok(skins_png::data_url(&self.skins.texture(id).await?))
    }

    /// A new skin from `texture`, a PNG data URL (the starter, a blank or the current skin).
    /// `seeded`: the launcher put it in an empty wardrobe by itself (see `create_seeded`).
    pub async fn skin_create(
        &self,
        name: &str,
        model: SkinModel,
        texture: &str,
        owner: Option<&str>,
        seeded: bool,
    ) -> Result<SkinEntry> {
        let png = skins_png::from_data_url(texture)?;
        if seeded {
            self.skins.create_seeded(name, model, &png, owner).await
        } else {
            self.skins.create(name, model, &png, owner).await
        }
    }

    /// A new skin from a PNG file the player picked, named after the file.
    pub async fn skin_import(&self, path: PathBuf, owner: Option<&str>) -> Result<SkinEntry> {
        let meta = tokio::fs::metadata(&path).await?;
        if !meta.is_file() || meta.len() > skins_png::MAX_SKIN_BYTES as u64 {
            return Err(skins_png::normalise(&[]).unwrap_err());
        }
        let bytes = tokio::fs::read(&path).await?;
        let name: String = path
            .file_stem()
            .map(|s| s.to_string_lossy().trim().chars().take(64).collect())
            .filter(|s: &String| !s.is_empty())
            .unwrap_or_else(|| "Skin".to_string());
        // `create` checks and normalises the PNG; doing it here as well read the file twice over.
        self.skins
            .create(&name, SkinModel::Classic, &bytes, owner)
            .await
    }

    pub async fn skin_save(&self, id: &str, texture: &str) -> Result<SkinEntry> {
        let png = skins_png::from_data_url(texture)?;
        let saved = self.skins.save_texture(id, &png).await?;
        self.medals_note(crate::medals::MedalEvent::SkinSaved).await;
        Ok(saved)
    }

    pub async fn skin_update(
        &self,
        id: &str,
        name: Option<String>,
        model: Option<SkinModel>,
    ) -> Result<SkinEntry> {
        self.skins.update(id, name.as_deref(), model).await
    }

    pub async fn skin_duplicate(&self, id: &str, name: &str) -> Result<SkinEntry> {
        self.skins.duplicate(id, name).await
    }

    /// Puts `owner`'s wardrobe in the order of `ids`, and answers it as it now stands.
    pub async fn skin_reorder(
        &self,
        owner: Option<&str>,
        ids: &[String],
    ) -> Result<Vec<SkinEntry>> {
        self.skins.reorder(owner, ids).await
    }

    pub async fn skin_delete(&self, id: &str) -> Result<()> {
        self.skins.delete(id).await
    }

    /// Writes `id` as a PNG at `path` (a `.png` extension is added when it has none).
    pub async fn skin_export(&self, id: &str, path: PathBuf) -> Result<PathBuf> {
        let png = self.skins.texture(id).await?;
        let path = png_path(path);
        launcher_core::fsutil::write_atomic(&path, &png).await?;
        if let Err(e) = self.skins.checkpoint(id, VersionReason::Exported).await {
            tracing::debug!(code = ?e.code, "couldn't keep the exported version");
        }
        Ok(path)
    }

    /// The kept versions of `id`, newest first.
    pub async fn skin_versions(&self, id: &str) -> Result<Vec<SkinVersion>> {
        self.skins.versions(id).await
    }

    pub async fn skin_version_texture(&self, id: &str, version: &str) -> Result<String> {
        Ok(skins_png::data_url(
            &self.skins.version_texture(id, version).await?,
        ))
    }

    /// Makes `version` the texture of `id`, keeping the one it replaces.
    pub async fn skin_restore(&self, id: &str, version: &str) -> Result<SkinEntry> {
        self.skins.restore(id, version).await
    }

    /// A new skin from `version` of `id`.
    pub async fn skin_version_copy(
        &self,
        id: &str,
        version: &str,
        name: &str,
    ) -> Result<SkinEntry> {
        self.skins.copy_version(id, version, name).await
    }

    /// Forgets the kept versions of `id`; the skin stays as it is.
    pub async fn skin_versions_clear(&self, id: &str) -> Result<()> {
        self.skins.clear_versions(id).await
    }

    /// Keeps the texture of `id` now: the player left a skin they changed.
    pub async fn skin_checkpoint(&self, id: &str) -> Result<()> {
        self.skins.checkpoint(id, VersionReason::Edited).await
    }

    /// The skin catalog: the cached one at once (`Refreshing`) while the Engine is asked in the
    /// background, and the Engine's answer at the next call; with nothing cached, the Engine's
    /// answer. After a manifest new to this run,
    /// catalog files nothing wears or lists anymore are pruned in the background.
    pub async fn skin_catalog(&self) -> CatalogAnswer {
        let catalog = self.runner.catalog();
        let (manifest, status) = catalog.load().await;
        if let (Some(m), CatalogStatus::Fresh) = (&manifest, status) {
            let is_new = {
                let mut seen = self.catalog_seen.lock().unwrap_or_else(|e| e.into_inner());
                if seen.as_deref() == Some(m.version.as_str()) {
                    false
                } else {
                    *seen = Some(m.version.clone());
                    true
                }
            };
            if is_new {
                match self.skins.look_hashes(m).await {
                    Ok(keep) => {
                        tokio::spawn(async move {
                            if let Err(e) = catalog.prune(keep).await {
                                tracing::debug!(code = ?e.code, "couldn't prune the skin catalog");
                            }
                        });
                    }
                    Err(e) => {
                        // Nobody prunes what a look may still need; try again next time.
                        *self.catalog_seen.lock().unwrap_or_else(|e| e.into_inner()) = None;
                        tracing::warn!(code = ?e.code, "skipped pruning the skin catalog");
                    }
                }
            }
        }
        CatalogAnswer { manifest, status }
    }

    /// A catalog PNG (a thumbnail, art, a mask, or a thumbnail sheet the manifest names) as a
    /// data URL.
    pub async fn skin_catalog_file(&self, sha256: &str) -> Result<String> {
        let bytes = self.runner.catalog().file(sha256.to_string()).await?;
        Ok(skins_png::data_url(&bytes))
    }

    /// The player's own painting on `id`, as a data URL; `None` without one.
    pub async fn skin_painting(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .skins
            .painting(id)
            .await?
            .map(|png| skins_png::data_url(&png)))
    }

    /// Saves the look of `id`: its pieces, the player's `painting` (a PNG data URL, if any) and
    /// the finished `texture`.
    pub async fn skin_look_save(
        &self,
        id: &str,
        look: SkinLook,
        painting: Option<&str>,
        texture: &str,
    ) -> Result<SkinEntry> {
        let texture = skins_png::from_data_url(texture)?;
        let painting = painting.map(skins_png::from_data_url).transpose()?;
        self.skins
            .save_look(id, look, painting.as_deref(), &texture)
            .await
    }

    /// Drops the look of `id`; the finished texture stays as a plain skin.
    pub async fn skin_look_clear(&self, id: &str) -> Result<SkinEntry> {
        self.skins.clear_look(id).await
    }

    async fn account(&self, uuid: &str) -> Result<Account> {
        self.accounts
            .lock()
            .await
            .list()
            .await?
            .accounts
            .into_iter()
            .find(|a| a.uuid == uuid)
            .ok_or_else(|| no_account(uuid))
    }

    /// The Minecraft token for `account`, refreshed the way a launch refreshes it.
    async fn minecraft_token(&self, account: Account) -> Result<McToken> {
        let session = self.session_for(account).await?;
        Ok(McToken {
            access_token: session.access_token,
            expires_at: u64::MAX,
        })
    }

    /// The skin `uuid` wears now; `None` for an offline account or one without a skin.
    pub async fn skin_current(&self, uuid: &str) -> Result<Option<CurrentSkin>> {
        let account = self.account(uuid).await?;
        Ok(self
            .current_skin(account)
            .await?
            .map(|(png, model)| CurrentSkin {
                texture: skins_png::data_url(&png),
                model,
            }))
    }

    /// The skin to draw as `uuid`'s figure: what a Microsoft account wears (the texture its face
    /// is cut from, so nothing new is fetched, with the arm model read off the texture), else the
    /// first skin in its wardrobe. It only reads: listing here never adopts loose skins the way
    /// opening the Skins screen does, and the Minecraft profile is never asked.
    pub async fn player_skin(&self, uuid: &str) -> Result<PlayerSkin> {
        let account = self.account(uuid).await?;
        let wardrobe = self.skins.list(Some(uuid)).await?;
        let worn = match self.worn_texture(&account).await {
            Some(texture) => skins_png::normalise(&texture).ok(),
            None => None,
        };
        let (texture, model) = match (worn, wardrobe.first()) {
            (Some(png), _) => {
                let model = skins_png::guess_model(&skins_png::decode(&png)?);
                (Some(skins_png::data_url(&png)), model)
            }
            (None, Some(first)) => {
                let png = self.skins.texture(&first.id).await?;
                (Some(skins_png::data_url(&png)), first.model)
            }
            (None, None) => (None, SkinModel::Classic),
        };
        Ok(PlayerSkin {
            texture,
            model,
            wardrobe: wardrobe.len(),
        })
    }

    /// The texture a Microsoft account wears, from the same cache as its face (no token needed);
    /// `None` for an offline account or one without a skin of its own.
    async fn worn_texture(&self, account: &Account) -> Option<Vec<u8>> {
        if account.provider != Provider::Microsoft {
            return None;
        }
        let runner = self.runner.clone();
        skins::cached_texture(
            &self.config.dirs.skins(),
            &account.uuid,
            SKIN_MAX_AGE,
            |id| runner.skin_texture(id),
        )
        .await
    }

    /// Puts the skin the Microsoft account `uuid` wears in its wardrobe, named `name` when it's
    /// new (an entry with the same texture and arm model is reused), and marks it worn. `None`
    /// for an offline account or one wearing Minecraft's default skin.
    pub async fn skin_sync_current(&self, uuid: &str, name: &str) -> Result<Option<SkinEntry>> {
        let account = self.account(uuid).await?;
        let Some((png, model)) = self.current_skin(account).await? else {
            return Ok(None);
        };
        Ok(Some(
            self.skins.record_current(uuid, name, model, &png).await?,
        ))
    }

    /// The skin `account` wears: the texture from the same cache as the account's face (no
    /// token needed), the arm model from the Minecraft profile, or read off the texture when the
    /// profile can't be asked.
    async fn current_skin(&self, account: Account) -> Result<Option<(Vec<u8>, SkinModel)>> {
        let Some(texture) = self.worn_texture(&account).await else {
            return Ok(None);
        };
        let png = skins_png::normalise(&texture)?;
        let asked = match self.minecraft_token(account).await {
            Ok(token) => self.runner.msa().skin_model(token).await.ok().flatten(),
            Err(_) => None,
        };
        let model = match asked {
            Some(model) => model,
            None => skins_png::guess_model(&skins_png::decode(&png)?),
        };
        Ok(Some((png, model)))
    }

    /// Uploads skin `id` to the Microsoft account `uuid` and marks it worn.
    pub async fn skin_wear(&self, id: &str, uuid: &str) -> Result<Vec<SkinEntry>> {
        let account = self.account(uuid).await?;
        if account.provider != Provider::Microsoft {
            return Err(offline_account());
        }
        let model = self.skins.entry(id).await?.model;
        let png = self.skins.texture(id).await?;
        let token = self.minecraft_token(account).await?;
        self.runner
            .msa()
            .upload_skin(token, png.clone(), model)
            .await?;
        self.skins.mark_worn(id, uuid).await?;
        self.config.telemetry.count(Counter::SkinsChanged);
        // The account's face shows the new skin at once, not when the remembered one expires.
        if let Err(e) = skins::remember_texture(&self.config.dirs.skins(), uuid, &png).await {
            tracing::debug!(code = ?e.code, "couldn't remember the skin just worn");
        }
        self.skins_list(Some(uuid), None).await
    }
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};
    use launcher_core::{
        accounts::{offline_account, Account, BuildKind, Provider},
        msa::{
            minecraft::{dashed, McToken},
            secrets::save_refresh,
        },
        skins_library::{SkinModel, VersionReason},
        skins_png, ErrorCode,
    };

    use crate::{
        app::tests::{core, TestCore},
        restore::now_secs,
    };

    const ALEX_ID: &str = "0123456789abcdef0123456789abcdef";

    async fn alex(core: &TestCore) -> Account {
        let account = Account {
            provider: Provider::Microsoft,
            username: "Alex".into(),
            uuid: dashed(ALEX_ID),
        };
        core.accounts
            .lock()
            .await
            .add_microsoft(account.clone())
            .await
            .unwrap();
        save_refresh(&*core.msa_secrets, &account.uuid, "refresh-Alex").unwrap();
        core.msa_tokens.lock().unwrap().insert(
            account.uuid.clone(),
            (
                McToken {
                    access_token: "mc-cached".into(),
                    expires_at: now_secs() + 7200,
                },
                None,
            ),
        );
        account
    }

    async fn steve(core: &TestCore) -> Account {
        let account = offline_account("Steve").unwrap();
        core.accounts
            .lock()
            .await
            .add_offline("Steve", BuildKind::Dev)
            .await
            .unwrap();
        account
    }

    fn texture() -> String {
        let mut image = RgbaImage::new(64, 64);
        image.put_pixel(8, 8, Rgba([1, 2, 3, 255]));
        skins_png::data_url(&skins_png::encode(&image).unwrap())
    }

    #[tokio::test]
    async fn wear_uploads_with_entry_model_and_marks_worn() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        let entry = core
            .skin_create("Mine", SkinModel::Slim, &texture(), Some(&alex.uuid), false)
            .await
            .unwrap();
        let list = core.skin_wear(&entry.id, &alex.uuid).await.unwrap();
        let calls = runner.msa.calls_to("upload_skin");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].args[0], "mc-cached");
        assert_eq!(calls[0].args[1], "slim");
        assert_eq!(list[0].worn_by.as_deref(), Some(alex.uuid.as_str()));
    }

    #[tokio::test]
    async fn wearing_a_skin_is_counted_when_it_worked() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        core.telemetry().force_on();
        let alex = alex(&core).await;
        let entry = core
            .skin_create("Mine", SkinModel::Slim, &texture(), Some(&alex.uuid), false)
            .await
            .unwrap();
        assert!(core.skin_wear("no-such-skin", &alex.uuid).await.is_err());
        assert!(core.telemetry().snapshot().counters().is_empty());
        core.skin_wear(&entry.id, &alex.uuid).await.unwrap();
        assert_eq!(core.telemetry().snapshot().counters()["skins_changed"], 1);
    }

    #[tokio::test]
    async fn reorder_sets_the_wardrobe_order() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        let mut made = Vec::new();
        for name in ["A", "B"] {
            made.push(
                core.skin_create(
                    name,
                    SkinModel::Classic,
                    &texture(),
                    Some(&alex.uuid),
                    false,
                )
                .await
                .unwrap()
                .id,
            );
        }
        let list = core.skin_reorder(Some(&alex.uuid), &made).await.unwrap();
        let ids: Vec<String> = list.into_iter().map(|e| e.id).collect();
        assert_eq!(ids, made);
        let listed: Vec<String> = core
            .skins_list(Some(&alex.uuid), None)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(listed, made);
    }

    #[tokio::test]
    async fn failed_upload_marks_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        let entry = core
            .skin_create(
                "Mine",
                SkinModel::Classic,
                &texture(),
                Some(&alex.uuid),
                false,
            )
            .await
            .unwrap();
        runner
            .msa
            .uploads
            .lock()
            .unwrap()
            .push_back(Err(launcher_core::LauncherError::auth(
                ErrorCode::SkinRejected,
                "rejected",
            )));
        let e = core.skin_wear(&entry.id, &alex.uuid).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinRejected);
        assert!(core.skins_list(Some(&alex.uuid), None).await.unwrap()[0]
            .worn_by
            .is_none());
    }

    #[tokio::test]
    async fn wear_offline_is_skin_offline_account() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let steve = steve(&core).await;
        let entry = core
            .skin_create(
                "Mine",
                SkinModel::Classic,
                &texture(),
                Some(&steve.uuid),
                false,
            )
            .await
            .unwrap();
        let e = core.skin_wear(&entry.id, &steve.uuid).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinOfflineAccount);
        assert!(runner.msa.calls_to("upload_skin").is_empty());
    }

    #[tokio::test]
    async fn player_skin_of_an_empty_offline_wardrobe_is_nothing_to_show() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let steve = steve(&core).await;
        let shown = core.player_skin(&steve.uuid).await.unwrap();
        assert_eq!(shown.texture, None);
        assert_eq!(shown.wardrobe, 0);
        assert!(runner.msa.calls.lock().unwrap().is_empty());
        assert!(runner.skin_asks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn player_skin_offline_is_the_first_in_the_wardrobe() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let steve = steve(&core).await;
        core.skin_create(
            "Old",
            SkinModel::Classic,
            &texture(),
            Some(&steve.uuid),
            false,
        )
        .await
        .unwrap();
        // A new skin goes on top of the player's order.
        let mut image = RgbaImage::new(64, 64);
        image.put_pixel(9, 9, Rgba([7, 7, 7, 255]));
        let top = skins_png::data_url(&skins_png::encode(&image).unwrap());
        core.skin_create("New", SkinModel::Slim, &top, Some(&steve.uuid), false)
            .await
            .unwrap();
        let shown = core.player_skin(&steve.uuid).await.unwrap();
        assert_eq!(shown.texture.as_deref(), Some(top.as_str()));
        assert_eq!(shown.model, SkinModel::Slim);
        assert_eq!(shown.wardrobe, 2);
    }

    #[tokio::test]
    async fn player_skin_only_reads_so_loose_skins_stay_loose() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let steve = steve(&core).await;
        core.skin_create("Loose", SkinModel::Classic, &texture(), None, false)
            .await
            .unwrap();
        let shown = core.player_skin(&steve.uuid).await.unwrap();
        assert_eq!(shown.wardrobe, 0);
        assert_eq!(core.skins.list(None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn player_skin_microsoft_is_the_skin_it_wears_without_asking_the_profile() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        let png = skins_png::from_data_url(&texture()).unwrap();
        *runner.skin.lock().unwrap() = Some(Ok(Some(png)));
        let shown = core.player_skin(&alex.uuid).await.unwrap();
        assert_eq!(shown.texture.as_deref(), Some(texture().as_str()));
        assert!(runner.msa.calls_to("skin_model").is_empty());
    }

    #[tokio::test]
    async fn player_skin_microsoft_without_a_skin_falls_back_to_the_wardrobe() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        *runner.skin.lock().unwrap() = Some(Ok(None));
        core.skin_create("Mine", SkinModel::Slim, &texture(), Some(&alex.uuid), false)
            .await
            .unwrap();
        let shown = core.player_skin(&alex.uuid).await.unwrap();
        assert_eq!(shown.texture.as_deref(), Some(texture().as_str()));
        assert_eq!(shown.model, SkinModel::Slim);
        assert_eq!(shown.wardrobe, 1);
    }

    #[tokio::test]
    async fn player_skin_of_an_unknown_account_is_no_account() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let e = core.player_skin("not-an-account").await.unwrap_err();
        assert_eq!(e.code, ErrorCode::NoAccount);
    }

    #[tokio::test]
    async fn current_offline_is_none() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let steve = steve(&core).await;
        assert_eq!(core.skin_current(&steve.uuid).await.unwrap(), None);
        assert!(runner.msa.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn current_microsoft_is_data_url() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        let png = skins_png::from_data_url(&texture()).unwrap();
        *runner.skin.lock().unwrap() = Some(Ok(Some(png)));
        runner
            .msa
            .skin_models
            .lock()
            .unwrap()
            .push_back(Ok(Some(SkinModel::Slim)));
        let current = core.skin_current(&alex.uuid).await.unwrap().unwrap();
        assert_eq!(current.model, SkinModel::Slim);
        assert_eq!(current.texture, texture());
    }

    #[tokio::test]
    async fn current_reads_the_model_off_the_texture_without_a_profile() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        // Classic: the column slim arms leave empty is painted.
        let mut image = RgbaImage::new(64, 64);
        for y in 20..32 {
            image.put_pixel(54, y, Rgba([9, 9, 9, 255]));
        }
        *runner.skin.lock().unwrap() = Some(Ok(Some(skins_png::encode(&image).unwrap())));
        runner.msa.skin_models.lock().unwrap().push_back(Err(
            launcher_core::LauncherError::network(ErrorCode::HostUnreachable, "down"),
        ));
        assert_eq!(
            core.skin_current(&alex.uuid).await.unwrap().unwrap().model,
            SkinModel::Classic
        );
    }

    #[tokio::test]
    async fn wearing_updates_the_accounts_face() {
        let (core, _runner, _host, t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        let entry = core
            .skin_create("Mine", SkinModel::Slim, &texture(), Some(&alex.uuid), false)
            .await
            .unwrap();
        core.skin_wear(&entry.id, &alex.uuid).await.unwrap();
        let id = launcher_core::skins::profile_id(&alex.uuid).unwrap();
        assert!(t.path().join(format!("cache/skins/{id}.png")).is_file());
    }

    fn real_skin(core_runner: &crate::testing::FakeRunner) {
        let png = skins_png::from_data_url(&texture()).unwrap();
        *core_runner.skin.lock().unwrap() = Some(Ok(Some(png)));
        core_runner
            .msa
            .skin_models
            .lock()
            .unwrap()
            .push_back(Ok(Some(SkinModel::Slim)));
    }

    #[tokio::test]
    async fn sync_current_adds_a_microsoft_accounts_skin_once() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        real_skin(&runner);
        let first = core
            .skin_sync_current(&alex.uuid, "Alex's skin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.model, SkinModel::Slim);
        assert_eq!(first.worn_by.as_deref(), Some(alex.uuid.as_str()));
        assert_eq!(core.skin_texture(&first.id).await.unwrap(), texture());
        real_skin(&runner);
        let again = core
            .skin_sync_current(&alex.uuid, "Alex's skin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(again.id, first.id);
        assert_eq!(
            core.skins_list(Some(&alex.uuid), None).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn sync_current_is_none_offline_or_wearing_the_default() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let steve = steve(&core).await;
        assert_eq!(
            core.skin_sync_current(&steve.uuid, "x").await.unwrap(),
            None
        );
        let alex = alex(&core).await;
        *runner.skin.lock().unwrap() = Some(Ok(None));
        assert_eq!(core.skin_sync_current(&alex.uuid, "x").await.unwrap(), None);
        assert!(core
            .skins_list(Some(&alex.uuid), None)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn the_active_account_adopts_loose_skins_and_others_dont() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let loose = core
            .skin_create("Loose", SkinModel::Classic, &texture(), None, false)
            .await
            .unwrap();
        let steve = steve(&core).await;
        let alex = alex(&core).await; // now the active account
        assert!(core
            .skins_list(Some(&steve.uuid), None)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(core.skins_list(None, None).await.unwrap().len(), 1);
        let mine = core.skins_list(Some(&alex.uuid), None).await.unwrap();
        assert_eq!(mine[0].id, loose.id);
        assert!(core.skins_list(None, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn signing_in_drops_the_untouched_starter_made_with_no_account() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let starter = core
            .skin_create("The Perlander", SkinModel::Classic, &texture(), None, true)
            .await
            .unwrap();
        let mine = core
            .skin_create("Mine", SkinModel::Classic, &texture(), None, false)
            .await
            .unwrap();
        let alex = alex(&core).await;
        let ids: Vec<_> = core
            .skins_list(Some(&alex.uuid), None)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, vec![mine.id]);
        assert!(core.skin_texture(&starter.id).await.is_err());
    }

    #[tokio::test]
    async fn the_starter_passed_in_recognises_an_old_untouched_one() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let old = core
            .skin_create("The Perlander", SkinModel::Classic, &texture(), None, false)
            .await
            .unwrap();
        let alex = alex(&core).await;
        let list = core
            .skins_list(Some(&alex.uuid), Some(&texture()))
            .await
            .unwrap();
        assert!(list.is_empty());
        assert!(core.skin_texture(&old.id).await.is_err());
    }

    #[tokio::test]
    async fn new_skins_go_to_the_wardrobe_shown() {
        let (core, _runner, _host, t) = core(BuildKind::Release);
        let steve = steve(&core).await;
        let path = t.path().join("castle.png");
        std::fs::write(&path, skins_png::encode(&RgbaImage::new(64, 64)).unwrap()).unwrap();
        let imported = core.skin_import(path, Some(&steve.uuid)).await.unwrap();
        assert_eq!(imported.owner.as_deref(), Some(steve.uuid.as_str()));
        assert_eq!(
            core.skins_list(Some(&steve.uuid), None)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn import_legacy_file_becomes_64x64() {
        let (core, _runner, _host, t) = core(BuildKind::Release);
        let path = t.path().join("old skin.png");
        let legacy = skins_png::encode(&RgbaImage::new(64, 32)).unwrap();
        std::fs::write(&path, legacy).unwrap();
        let entry = core.skin_import(path, None).await.unwrap();
        assert_eq!(entry.name, "old skin");
        let url = core.skin_texture(&entry.id).await.unwrap();
        let image = skins_png::decode(&skins_png::from_data_url(&url).unwrap()).unwrap();
        assert_eq!((image.width(), image.height()), (64, 64));
    }

    #[tokio::test]
    async fn import_non_skin_is_skin_invalid() {
        let (core, _runner, _host, t) = core(BuildKind::Release);
        let path = t.path().join("photo.png");
        std::fs::write(&path, b"not a png").unwrap();
        assert_eq!(
            core.skin_import(path, None).await.unwrap_err().code,
            ErrorCode::SkinInvalid
        );
    }

    #[tokio::test]
    async fn export_adds_png_extension() {
        let (core, _runner, _host, t) = core(BuildKind::Release);
        let entry = core
            .skin_create("Mine", SkinModel::Classic, &texture(), None, false)
            .await
            .unwrap();
        let out = core
            .skin_export(&entry.id, t.path().join("mine"))
            .await
            .unwrap();
        assert_eq!(out, t.path().join("mine.png"));
        assert_eq!(
            std::fs::read(&out).unwrap(),
            skins_png::from_data_url(&texture()).unwrap()
        );
    }

    #[tokio::test]
    async fn export_keeps_an_exported_version() {
        let (core, _runner, _host, t) = core(BuildKind::Release);
        let entry = core
            .skin_create("Mine", SkinModel::Classic, &texture(), None, false)
            .await
            .unwrap();
        core.skin_export(&entry.id, t.path().join("mine"))
            .await
            .unwrap();
        let versions = core.skin_versions(&entry.id).await.unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].reason, VersionReason::Exported);
        assert!(versions[0].current);
    }

    #[tokio::test]
    async fn wearing_keeps_a_worn_version() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        let entry = core
            .skin_create("Mine", SkinModel::Slim, &texture(), None, false)
            .await
            .unwrap();
        core.skin_wear(&entry.id, &alex.uuid).await.unwrap();
        let versions = core.skin_versions(&entry.id).await.unwrap();
        assert!(versions[0].worn);
    }

    #[tokio::test]
    async fn restore_and_copy_from_a_version() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let entry = core
            .skin_create("Mine", SkinModel::Slim, &texture(), None, false)
            .await
            .unwrap();
        core.skin_checkpoint(&entry.id).await.unwrap();
        let mut image = RgbaImage::new(64, 64);
        image.put_pixel(8, 8, Rgba([7, 7, 7, 255]));
        let painted = skins_png::data_url(&skins_png::encode(&image).unwrap());
        core.skin_save(&entry.id, &painted).await.unwrap();
        let first = core.skin_versions(&entry.id).await.unwrap().pop().unwrap();
        assert_eq!(
            core.skin_version_texture(&entry.id, &first.id)
                .await
                .unwrap(),
            texture()
        );
        core.skin_restore(&entry.id, &first.id).await.unwrap();
        assert_eq!(core.skin_texture(&entry.id).await.unwrap(), texture());
        let copy = core
            .skin_version_copy(&entry.id, &first.id, "Old")
            .await
            .unwrap();
        assert_eq!(copy.model, SkinModel::Slim);
        assert_eq!(core.skin_texture(&copy.id).await.unwrap(), texture());
        assert_eq!(
            core.skin_restore(&entry.id, "1").await.unwrap_err().code,
            ErrorCode::SkinVersionNotFound
        );
    }

    fn manifest(version: &str) -> launcher_core::skin_catalog::Manifest {
        let mut m: launcher_core::skin_catalog::Manifest = serde_json::from_str(include_str!(
            "../../launcher-core/tests/fixtures/skin-catalog-manifest.json"
        ))
        .unwrap();
        m.version = version.into();
        m
    }

    async fn pruned(runner: &crate::testing::FakeRunner, n: usize) {
        for _ in 0..200 {
            if runner.catalog.prunes.lock().unwrap().len() >= n {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    #[tokio::test]
    async fn skin_catalog_reports_status() {
        use launcher_core::skin_catalog::CatalogStatus;
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let none = core.skin_catalog().await;
        assert_eq!(none.status, CatalogStatus::Unavailable);
        assert!(none.manifest.is_none());
        *runner.catalog.load.lock().unwrap() = (Some(manifest("v1")), CatalogStatus::Cached);
        let cached = core.skin_catalog().await;
        assert_eq!(cached.status, CatalogStatus::Cached);
        assert_eq!(cached.manifest.unwrap().version, "v1");
        // Only a fresh manifest prunes.
        tokio::task::yield_now().await;
        assert!(runner.catalog.prunes.lock().unwrap().is_empty());
        *runner.catalog.load.lock().unwrap() = (Some(manifest("v1")), CatalogStatus::Fresh);
        assert_eq!(core.skin_catalog().await.status, CatalogStatus::Fresh);
        pruned(&runner, 1).await;
        assert_eq!(runner.catalog.prunes.lock().unwrap().len(), 1);
        // The same version again doesn't prune again; a new one does.
        core.skin_catalog().await;
        *runner.catalog.load.lock().unwrap() = (Some(manifest("v2")), CatalogStatus::Fresh);
        core.skin_catalog().await;
        pruned(&runner, 2).await;
        assert_eq!(runner.catalog.prunes.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn skin_catalog_file_is_a_data_url_or_an_error() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let png = skins_png::from_data_url(&texture()).unwrap();
        runner
            .catalog
            .files
            .lock()
            .unwrap()
            .insert("abc".into(), png);
        assert_eq!(core.skin_catalog_file("abc").await.unwrap(), texture());
        assert_eq!(
            core.skin_catalog_file("nope").await.unwrap_err().code,
            ErrorCode::SkinCatalogUnavailable
        );
    }

    fn look(ids: &[&str]) -> launcher_core::skins_library::SkinLook {
        launcher_core::skins_library::SkinLook {
            pieces: ids
                .iter()
                .map(|id| launcher_core::skins_library::WornPiece {
                    id: (*id).into(),
                    colours: Default::default(),
                })
                .collect(),
            painting: false,
        }
    }

    #[tokio::test]
    async fn skin_look_save_round_trips() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let entry = core
            .skin_create("Mine", SkinModel::Classic, &texture(), None, false)
            .await
            .unwrap();
        let mut image = RgbaImage::new(64, 64);
        image.put_pixel(1, 1, Rgba([9, 9, 9, 255]));
        let painting = skins_png::data_url(&skins_png::encode(&image).unwrap());
        let saved = core
            .skin_look_save(&entry.id, look(&["hat-cap"]), Some(&painting), &texture())
            .await
            .unwrap();
        let l = saved.look.unwrap();
        assert_eq!(l.pieces[0].id, "hat-cap");
        assert!(l.painting);
        assert_eq!(
            core.skin_painting(&entry.id).await.unwrap().as_deref(),
            Some(painting.as_str())
        );
        let cleared = core.skin_look_clear(&entry.id).await.unwrap();
        assert!(cleared.look.is_none());
        assert_eq!(core.skin_painting(&entry.id).await.unwrap(), None);
        assert_eq!(core.skin_texture(&entry.id).await.unwrap(), texture());
    }

    #[tokio::test]
    async fn skin_painting_none_without_look() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let entry = core
            .skin_create("Mine", SkinModel::Classic, &texture(), None, false)
            .await
            .unwrap();
        assert_eq!(core.skin_painting(&entry.id).await.unwrap(), None);
        let saved = core
            .skin_look_save(&entry.id, look(&[]), None, &texture())
            .await
            .unwrap();
        assert!(!saved.look.unwrap().painting);
        assert_eq!(core.skin_painting(&entry.id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn clearing_the_history_keeps_the_skin() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let entry = core
            .skin_create("Mine", SkinModel::Slim, &texture(), None, false)
            .await
            .unwrap();
        core.skin_checkpoint(&entry.id).await.unwrap();
        assert!(!core.skin_versions(&entry.id).await.unwrap().is_empty());
        core.skin_versions_clear(&entry.id).await.unwrap();
        assert!(core.skin_versions(&entry.id).await.unwrap().is_empty());
        assert_eq!(core.skin_texture(&entry.id).await.unwrap(), texture());
    }
}

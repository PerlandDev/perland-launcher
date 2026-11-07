//! The Skins screen: the wardrobe on disk, an account's current skin, and wearing a skin.
//! Textures cross to the webview as `data:image/png;base64,` URLs.
use std::path::PathBuf;

use launcher_core::{
    accounts::{Account, Provider},
    msa::minecraft::McToken,
    skins,
    skins_library::{SkinEntry, SkinModel, SkinVersion, VersionReason},
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
    pub async fn skins_list(&self) -> Result<Vec<SkinEntry>> {
        self.skins.list().await
    }

    pub async fn skin_texture(&self, id: &str) -> Result<String> {
        Ok(skins_png::data_url(&self.skins.texture(id).await?))
    }

    /// A new skin from `texture`, a PNG data URL (the starter, a blank or the current skin).
    pub async fn skin_create(
        &self,
        name: &str,
        model: SkinModel,
        texture: &str,
    ) -> Result<SkinEntry> {
        let png = skins_png::from_data_url(texture)?;
        self.skins.create(name, model, &png).await
    }

    /// A new skin from a PNG file the player picked, named after the file.
    pub async fn skin_import(&self, path: PathBuf) -> Result<SkinEntry> {
        let meta = tokio::fs::metadata(&path).await?;
        if !meta.is_file() || meta.len() > skins_png::MAX_SKIN_BYTES as u64 {
            return Err(skins_png::normalise(&[]).unwrap_err());
        }
        let bytes = tokio::fs::read(&path).await?;
        let png = skins_png::normalise(&bytes)?;
        let name: String = path
            .file_stem()
            .map(|s| s.to_string_lossy().trim().chars().take(64).collect())
            .filter(|s: &String| !s.is_empty())
            .unwrap_or_else(|| "Skin".to_string());
        self.skins.create(&name, SkinModel::Classic, &png).await
    }

    pub async fn skin_save(&self, id: &str, texture: &str) -> Result<SkinEntry> {
        let png = skins_png::from_data_url(texture)?;
        self.skins.save_texture(id, &png).await
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

    /// Keeps the texture of `id` now: the player left a skin they changed.
    pub async fn skin_checkpoint(&self, id: &str) -> Result<()> {
        self.skins.checkpoint(id, VersionReason::Edited).await
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

    /// The skin `account` wears: the texture from the same cache as the account's face (no
    /// token needed), the arm model from the Minecraft profile, or read off the texture when the
    /// profile can't be asked.
    async fn current_skin(&self, account: Account) -> Result<Option<(Vec<u8>, SkinModel)>> {
        if account.provider != Provider::Microsoft {
            return Ok(None);
        }
        let runner = self.runner.clone();
        let Some(texture) = skins::cached_texture(
            &self.config.dirs.skins(),
            &account.uuid,
            SKIN_MAX_AGE,
            |id| runner.skin_texture(id),
        )
        .await
        else {
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
        let model = self
            .skins
            .list()
            .await?
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.model)
            .ok_or_else(|| {
                LauncherError::disk(
                    ErrorCode::SkinNotFound,
                    "That skin isn't in your wardrobe anymore.",
                )
            })?;
        let png = self.skins.texture(id).await?;
        let token = self.minecraft_token(account).await?;
        self.runner
            .msa()
            .upload_skin(token, png.clone(), model)
            .await?;
        self.skins.mark_worn(id, uuid).await?;
        // The account's face shows the new skin at once, not when the remembered one expires.
        if let Err(e) = skins::remember_texture(&self.config.dirs.skins(), uuid, &png).await {
            tracing::debug!(code = ?e.code, "couldn't remember the skin just worn");
        }
        self.skins.list().await
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
            .skin_create("Mine", SkinModel::Slim, &texture())
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
    async fn failed_upload_marks_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = alex(&core).await;
        let entry = core
            .skin_create("Mine", SkinModel::Classic, &texture())
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
        assert!(core.skins_list().await.unwrap()[0].worn_by.is_none());
    }

    #[tokio::test]
    async fn wear_offline_is_skin_offline_account() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let steve = steve(&core).await;
        let entry = core
            .skin_create("Mine", SkinModel::Classic, &texture())
            .await
            .unwrap();
        let e = core.skin_wear(&entry.id, &steve.uuid).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinOfflineAccount);
        assert!(runner.msa.calls_to("upload_skin").is_empty());
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
            .skin_create("Mine", SkinModel::Slim, &texture())
            .await
            .unwrap();
        core.skin_wear(&entry.id, &alex.uuid).await.unwrap();
        let id = launcher_core::skins::profile_id(&alex.uuid).unwrap();
        assert!(t.path().join(format!("cache/skins/{id}.png")).is_file());
    }

    #[tokio::test]
    async fn import_legacy_file_becomes_64x64() {
        let (core, _runner, _host, t) = core(BuildKind::Release);
        let path = t.path().join("old skin.png");
        let legacy = skins_png::encode(&RgbaImage::new(64, 32)).unwrap();
        std::fs::write(&path, legacy).unwrap();
        let entry = core.skin_import(path).await.unwrap();
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
            core.skin_import(path).await.unwrap_err().code,
            ErrorCode::SkinInvalid
        );
    }

    #[tokio::test]
    async fn export_adds_png_extension() {
        let (core, _runner, _host, t) = core(BuildKind::Release);
        let entry = core
            .skin_create("Mine", SkinModel::Classic, &texture())
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
            .skin_create("Mine", SkinModel::Classic, &texture())
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
            .skin_create("Mine", SkinModel::Slim, &texture())
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
            .skin_create("Mine", SkinModel::Slim, &texture())
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
}

//! The Skins screen: the wardrobe on disk, an account's current skin, and wearing a skin.
//! Textures cross to the webview as `data:image/png;base64,` URLs.
use std::path::PathBuf;

use launcher_core::{
    accounts::{Account, Provider},
    msa::minecraft::McToken,
    skins_library::{SkinEntry, SkinModel},
    skins_png, ErrorCategory, ErrorCode, LauncherError, Result,
};
use serde::Serialize;

use crate::{app::AppCore, events::Host, runner::Runner};

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
        Ok(path)
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

    /// The one place the current skin is read. When the avatar work (`launcher_core::skins`,
    /// with its `cache/skins/<uuid>.png`) lands, switch the texture read to that cache here.
    async fn current_skin(&self, account: Account) -> Result<Option<crate::msa::SkinTexture>> {
        if account.provider != Provider::Microsoft {
            return Ok(None);
        }
        let token = self.minecraft_token(account).await?;
        self.runner.msa().active_skin(token).await
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
        self.runner.msa().upload_skin(token, png, model).await?;
        self.skins.mark_worn(id, uuid).await?;
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
        skins_library::SkinModel,
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
        runner
            .msa
            .active_skins
            .lock()
            .unwrap()
            .push_back(Ok(Some((png, SkinModel::Slim))));
        let current = core.skin_current(&alex.uuid).await.unwrap().unwrap();
        assert_eq!(current.model, SkinModel::Slim);
        assert_eq!(current.texture, texture());
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
}

//! A Microsoft account's own skin: its texture from Mojang's public session server (no token
//! needed), cached on disk by profile id, and the face the launcher draws as the account's
//! picture. An account without a skin of its own, or one that can't be fetched, keeps the
//! generated picture.
use std::{
    future::Future,
    io::Cursor,
    path::Path,
    time::{Duration, SystemTime},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use image::{imageops, DynamicImage, ImageFormat, ImageReader, Limits, RgbaImage};
use serde::Deserialize;

use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::write_atomic,
    http::{read_capped, HttpClient},
    invite::Url,
};

/// The most of a session server answer that is read: a profile is well under 4 KiB.
const PROFILE_CAP: usize = 64 << 10;
/// The most of a skin texture that is read: a 64×64 PNG is a few KiB; HD skins stay far under.
pub const TEXTURE_CAP: usize = 1 << 20;
/// No skin is wider than this (an HD skin of 16× the classic size).
const MAX_SIDE: u32 = 1024;
/// The face is drawn at most at 40 CSS pixels; 64 covers a 2x screen with crisp pixels.
const MAX_FACE: u32 = 64;
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
/// Cached "this account wears no skin of its own".
const NONE_EXT: &str = "none";
/// The only host a skin texture is fetched from, whatever the profile names.
const TEXTURES_HOST: &str = "textures.minecraft.net";

/// Where skins come from. Tests point these at a mock server.
#[derive(Debug, Clone)]
pub struct SkinEndpoints {
    /// The session server base, without a path.
    pub session: Url,
    /// The texture server base, without a path.
    pub textures: Url,
}

impl SkinEndpoints {
    pub fn production() -> Self {
        let url = |s: &str| Url::parse(s).expect("a constant URL");
        Self {
            session: url("https://sessionserver.mojang.com"),
            textures: url("https://textures.minecraft.net"),
        }
    }
}

/// Fetches skin textures. Use a client that follows no redirects, so a texture only ever comes
/// from the texture server.
#[derive(Debug, Clone)]
pub struct SkinClient {
    http: HttpClient,
    endpoints: SkinEndpoints,
}

/// A profile id without dashes, lower case, or `None` when `uuid` isn't one. The result is safe
/// in a path and a URL.
pub fn profile_id(uuid: &str) -> Option<String> {
    let id: String = uuid.chars().filter(|c| *c != '-').collect();
    (id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())).then(|| id.to_ascii_lowercase())
}

/// The texture hash of the skin a session server profile answer names, or `None` when the
/// profile wears none of its own (or names a texture anywhere but the texture server).
pub fn texture_hash(profile: &[u8]) -> Option<String> {
    let profile: Profile = serde_json::from_slice(profile).ok()?;
    let value = profile
        .properties
        .into_iter()
        .find(|p| p.name == "textures")?
        .value;
    let textures: Textures = serde_json::from_slice(&STANDARD.decode(value).ok()?).ok()?;
    let url = Url::parse(&textures.textures.skin?.url).ok()?;
    let on_texture_server = matches!(url.scheme(), "http" | "https")
        && url
            .host_str()
            .is_some_and(|h| h.eq_ignore_ascii_case(TEXTURES_HOST));
    let mut segments = url.path_segments()?;
    let (Some("texture"), Some(hash), None) = (segments.next(), segments.next(), segments.next())
    else {
        return None;
    };
    let hash_like =
        !hash.is_empty() && hash.len() <= 128 && hash.bytes().all(|b| b.is_ascii_hexdigit());
    (on_texture_server && hash_like).then(|| hash.to_ascii_lowercase())
}

#[derive(Deserialize)]
struct Profile {
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct Textures {
    textures: TextureSet,
}

#[derive(Deserialize)]
struct TextureSet {
    #[serde(rename = "SKIN")]
    skin: Option<Texture>,
}

#[derive(Deserialize)]
struct Texture {
    url: String,
}

/// The face of a skin texture as a PNG: the head's front with the hat layer over it, at the
/// skin's own resolution (at most [`MAX_FACE`] pixels). `None` when `texture` isn't a skin.
pub fn face(texture: &[u8]) -> Option<Vec<u8>> {
    if !texture.starts_with(PNG) {
        return None;
    }
    let mut reader = ImageReader::with_format(Cursor::new(texture), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    reader.limits(limits);
    let skin = reader.decode().ok()?.to_rgba8();
    let (w, h) = skin.dimensions();
    let legacy = h * 2 == w;
    if w < 64 || w % 64 != 0 || !(h == w || legacy) {
        return None;
    }
    let unit = w / 64;
    let side = 8 * unit;
    let mut head = imageops::crop_imm(&skin, 8 * unit, 8 * unit, side, side).to_image();
    // The game draws the head itself opaque, whatever its alpha.
    opaque(&mut head);
    let hat = imageops::crop_imm(&skin, 40 * unit, 8 * unit, side, side).to_image();
    // Like the game: a legacy skin's hat counts only when some of it is see-through, since old
    // skins often filled it with one solid colour.
    if !legacy || hat.pixels().any(|p| p.0[3] < 128) {
        imageops::overlay(&mut head, &hat, 0, 0);
        // Blending rounds; over an opaque head the face stays opaque.
        opaque(&mut head);
    }
    let head = if side > MAX_FACE {
        imageops::resize(&head, MAX_FACE, MAX_FACE, imageops::FilterType::Nearest)
    } else {
        head
    };
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(head)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .ok()?;
    Some(out)
}

fn opaque(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        pixel.0[3] = 255;
    }
}

impl SkinClient {
    pub fn new(http: HttpClient, endpoints: SkinEndpoints) -> Self {
        Self { http, endpoints }
    }

    /// The skin texture PNG profile `uuid` wears, or `None` when it wears none of its own.
    pub async fn texture(&self, uuid: &str) -> Result<Option<Vec<u8>>> {
        let Some(id) = profile_id(uuid) else {
            return Ok(None);
        };
        let session = &self.endpoints.session;
        let Some(profile) = self
            .get(
                session,
                &format!("session/minecraft/profile/{id}"),
                PROFILE_CAP,
            )
            .await?
        else {
            return Ok(None);
        };
        if profile.is_empty() {
            return Ok(None);
        }
        if serde_json::from_slice::<serde_json::Value>(&profile).is_err() {
            return Err(trouble(session, "the profile answer wasn't readable"));
        }
        let Some(hash) = texture_hash(&profile) else {
            return Ok(None);
        };
        let textures = &self.endpoints.textures;
        let Some(texture) = self
            .get(textures, &format!("texture/{hash}"), TEXTURE_CAP)
            .await?
        else {
            return Ok(None);
        };
        if !texture.starts_with(PNG) {
            return Err(trouble(textures, "the skin isn't a PNG"));
        }
        Ok(Some(texture))
    }

    /// One GET under `base`: the body (at most `cap` bytes) on success, `None` on 204 or 404.
    async fn get(&self, base: &Url, path: &str, cap: usize) -> Result<Option<Vec<u8>>> {
        let url = base
            .join(path)
            .map_err(|_| trouble(base, "the address isn't valid"))?;
        let response = self.http.get(url).send().await.map_err(|e| {
            LauncherError::network(
                ErrorCode::HostUnreachable,
                format!("Couldn't reach {}", host(base)),
            )
            .with_detail(e)
        })?;
        let status = response.status();
        if matches!(status.as_u16(), 204 | 404) {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(trouble(base, format!("it answered {status}")));
        }
        read_capped(response, cap)
            .await
            .map_err(|e| trouble(base, e))?
            .map(Some)
            .ok_or_else(|| trouble(base, "the answer was too large"))
    }
}

fn host(base: &Url) -> &str {
    base.host_str().unwrap_or("the skin server")
}

fn trouble(base: &Url, detail: impl std::fmt::Display) -> LauncherError {
    LauncherError::network(
        ErrorCode::HostTrouble,
        format!("{} had trouble sending the skin", host(base)),
    )
    .with_detail(detail)
}

/// The skin texture of `uuid` from the cache in `cache_dir` while it is younger than `max_age`,
/// else from `fetch`, then remembered. When `fetch` fails the last texture seen is still used.
/// `None` when the account wears no skin of its own or none was ever fetched.
pub async fn cached_texture<F, Fut>(
    cache_dir: &Path,
    uuid: &str,
    max_age: Duration,
    fetch: F,
) -> Option<Vec<u8>>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<Option<Vec<u8>>>>,
{
    let id = profile_id(uuid)?;
    let found = cache_dir.join(format!("{id}.png"));
    let missing = cache_dir.join(format!("{id}.{NONE_EXT}"));
    if age(&found).await.is_some_and(|a| a < max_age) {
        if let Some(texture) = read_texture(&found).await {
            return Some(texture);
        }
    }
    if age(&missing).await.is_some_and(|a| a < max_age) {
        return None;
    }
    match fetch(id).await {
        // The cache only saves work: a texture that can't be saved is still shown.
        Ok(Some(texture)) => {
            if write_atomic(&found, &texture).await.is_ok() {
                let _ = tokio::fs::remove_file(&missing).await;
            }
            Some(texture)
        }
        Ok(None) => {
            if write_atomic(&missing, &[]).await.is_ok() {
                let _ = tokio::fs::remove_file(&found).await;
            }
            None
        }
        Err(error) => {
            tracing::debug!(code = ?error.code, "couldn't fetch a skin; using the last one seen");
            read_texture(&found).await
        }
    }
}

/// How long ago `path` was written, or `None` when it isn't there.
async fn age(path: &Path) -> Option<Duration> {
    let modified = tokio::fs::metadata(path).await.ok()?.modified().ok()?;
    // A clock set back makes an answer look new; it is fetched again once the clock passes it.
    Some(
        SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default(),
    )
}

async fn read_texture(path: &Path) -> Option<Vec<u8>> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    if !meta.is_file() || meta.len() > TEXTURE_CAP as u64 {
        return None;
    }
    tokio::fs::read(path).await.ok()
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    fn png(image: &RgbaImage) -> Vec<u8> {
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(image.clone())
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn decode_png(bytes: &[u8]) -> RgbaImage {
        image::load_from_memory_with_format(bytes, ImageFormat::Png)
            .unwrap()
            .to_rgba8()
    }

    const SKIN: Rgba<u8> = Rgba([200, 150, 100, 255]);
    const HAT: Rgba<u8> = Rgba([10, 20, 30, 255]);

    /// A `w`×`h` skin: the face all `SKIN` (one pixel half transparent, which the face ignores),
    /// the hat clear but for its top-left pixel.
    fn skin(w: u32, h: u32) -> RgbaImage {
        let unit = w / 64;
        let mut image = RgbaImage::new(w, h);
        for y in 0..8 * unit {
            for x in 0..8 * unit {
                image.put_pixel(8 * unit + x, 8 * unit + y, SKIN);
            }
        }
        image.put_pixel(8 * unit + 7 * unit, 8 * unit, Rgba([200, 150, 100, 40]));
        for y in 0..unit {
            for x in 0..unit {
                image.put_pixel(40 * unit + x, 8 * unit + y, HAT);
            }
        }
        image
    }

    #[test]
    fn profile_ids_lose_their_dashes() {
        assert_eq!(
            profile_id("069A79F4-44e9-4726-a5be-fca90e38aaf5").as_deref(),
            Some("069a79f444e94726a5befca90e38aaf5")
        );
        assert_eq!(
            profile_id("069a79f444e94726a5befca90e38aaf5").as_deref(),
            Some("069a79f444e94726a5befca90e38aaf5")
        );
        for bad in [
            "",
            "steve",
            "../../etc/passwd",
            "069a79f444e94726a5befca90e38aaz5",
        ] {
            assert_eq!(profile_id(bad), None, "{bad}");
        }
    }

    fn profile_with(textures: serde_json::Value) -> Vec<u8> {
        let value = STANDARD.encode(textures.to_string());
        serde_json::json!({
            "id": "069a79f444e94726a5befca90e38aaf5",
            "name": "Notch",
            "properties": [{ "name": "textures", "value": value }],
        })
        .to_string()
        .into_bytes()
    }

    const HASH: &str = "292009a4925b58f02c77dadc3ecef07ea4c7472f64e0fdc32ce5522489362680";

    #[test]
    fn the_texture_hash_comes_from_the_textures_property() {
        let profile = profile_with(serde_json::json!({
            "textures": { "SKIN": { "url": format!("http://textures.minecraft.net/texture/{HASH}") } }
        }));
        assert_eq!(texture_hash(&profile).as_deref(), Some(HASH));
        let https = profile_with(serde_json::json!({
            "textures": { "SKIN": {
                "url": format!("https://TEXTURES.minecraft.net/texture/{HASH}"),
                "metadata": { "model": "slim" },
            } }
        }));
        assert_eq!(texture_hash(&https).as_deref(), Some(HASH));
    }

    #[test]
    fn no_skin_of_its_own_or_a_strange_one_is_none() {
        let bare = profile_with(serde_json::json!({ "textures": {} }));
        let elsewhere = profile_with(serde_json::json!({
            "textures": { "SKIN": { "url": format!("https://evil.example/texture/{HASH}") } }
        }));
        let odd_path = profile_with(serde_json::json!({
            "textures": { "SKIN": { "url": "https://textures.minecraft.net/texture/../x" } }
        }));
        let no_property =
            br#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[]}"#;
        for answer in [&bare[..], &elsewhere, &odd_path, no_property, b"not json"] {
            assert_eq!(texture_hash(answer), None);
        }
    }

    #[test]
    fn the_face_is_the_head_front_with_the_hat_over_it() {
        let got = decode_png(&face(&png(&skin(64, 64))).unwrap());
        assert_eq!(got.dimensions(), (8, 8));
        assert_eq!(*got.get_pixel(0, 0), HAT, "the hat covers the face");
        assert_eq!(*got.get_pixel(1, 0), SKIN);
        assert_eq!(
            *got.get_pixel(7, 0),
            Rgba([200, 150, 100, 255]),
            "the face itself is always opaque"
        );
    }

    #[test]
    fn a_half_clear_hat_blends_over_the_face() {
        let mut image = skin(64, 64);
        image.put_pixel(41, 8, Rgba([0, 0, 0, 128]));
        let got = decode_png(&face(&png(&image)).unwrap());
        let Rgba([r, g, b, a]) = *got.get_pixel(1, 0);
        assert_eq!(a, 255);
        assert!((99..=101).contains(&r) && (74..=76).contains(&g) && (49..=51).contains(&b));
    }

    #[test]
    fn hd_skins_keep_their_detail_up_to_the_face_limit() {
        let got = decode_png(&face(&png(&skin(128, 128))).unwrap());
        assert_eq!(got.dimensions(), (16, 16));
        assert_eq!(*got.get_pixel(1, 1), HAT);
        assert_eq!(*got.get_pixel(2, 0), SKIN);
        let big = decode_png(&face(&png(&skin(1024, 1024))).unwrap());
        assert_eq!(big.dimensions(), (MAX_FACE, MAX_FACE));
    }

    #[test]
    fn a_legacy_skin_with_a_solid_hat_shows_no_hat() {
        // Old 64×32 skins often fill the hat with a solid colour; the game draws no hat then.
        let mut legacy = skin(64, 32);
        for y in 8..16 {
            for x in 40..48 {
                legacy.put_pixel(x, y, HAT);
            }
        }
        let got = decode_png(&face(&png(&legacy)).unwrap());
        assert_eq!(*got.get_pixel(0, 0), SKIN);
        let with_clear_hat = decode_png(&face(&png(&skin(64, 32))).unwrap());
        assert_eq!(*with_clear_hat.get_pixel(0, 0), HAT);
    }

    #[test]
    fn anything_but_a_skin_is_none() {
        for (w, h) in [(32, 32), (64, 48), (100, 100), (64, 128), (2048, 2048)] {
            assert_eq!(face(&png(&RgbaImage::new(w, h))), None, "{w}x{h}");
        }
        assert_eq!(face(b"not a picture"), None);
        let mut jpeg = Vec::new();
        DynamicImage::ImageRgba8(skin(64, 64))
            .to_rgb8()
            .write_to(&mut Cursor::new(&mut jpeg), ImageFormat::Jpeg)
            .unwrap();
        assert_eq!(face(&jpeg), None, "skins are PNGs");
    }

    const UUID: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
    const HOUR: Duration = Duration::from_secs(3600);

    #[tokio::test]
    async fn a_fetched_texture_is_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let got = cached_texture(dir.path(), UUID, HOUR, |id| async move {
            assert_eq!(id, "069a79f444e94726a5befca90e38aaf5");
            Ok(Some(b"skin v1".to_vec()))
        })
        .await;
        assert_eq!(got.as_deref(), Some(&b"skin v1"[..]));
        let again = cached_texture(dir.path(), UUID, HOUR, |_| async {
            panic!("a fresh texture isn't fetched again")
        })
        .await;
        assert_eq!(again.as_deref(), Some(&b"skin v1"[..]));
    }

    #[tokio::test]
    async fn no_skin_is_remembered_too() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            cached_texture(dir.path(), UUID, HOUR, |_| async { Ok(None) }).await,
            None
        );
        let again = cached_texture(dir.path(), UUID, HOUR, |_| async {
            panic!("a fresh answer isn't fetched again")
        })
        .await;
        assert_eq!(again, None);
    }

    #[tokio::test]
    async fn an_old_answer_is_fetched_again_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        cached_texture(dir.path(), UUID, HOUR, |_| async {
            Ok(Some(b"v1".to_vec()))
        })
        .await;
        let changed = cached_texture(dir.path(), UUID, Duration::ZERO, |_| async {
            Ok(Some(b"v2".to_vec()))
        })
        .await;
        assert_eq!(changed.as_deref(), Some(&b"v2"[..]));
        let removed =
            cached_texture(dir.path(), UUID, Duration::ZERO, |_| async { Ok(None) }).await;
        assert_eq!(removed, None, "a skin taken off is gone");
        let later = cached_texture(dir.path(), UUID, HOUR, |_| async {
            panic!("no skin is a fresh answer too")
        })
        .await;
        assert_eq!(later, None);
    }

    #[tokio::test]
    async fn a_failed_fetch_keeps_the_last_texture() {
        let dir = tempfile::tempdir().unwrap();
        cached_texture(dir.path(), UUID, HOUR, |_| async {
            Ok(Some(b"v1".to_vec()))
        })
        .await;
        let offline = cached_texture(dir.path(), UUID, Duration::ZERO, |_| async {
            Err(LauncherError::network(
                ErrorCode::HostUnreachable,
                "offline",
            ))
        })
        .await;
        assert_eq!(offline.as_deref(), Some(&b"v1"[..]));
        let never = tempfile::tempdir().unwrap();
        let none = cached_texture(never.path(), UUID, HOUR, |_| async {
            Err(LauncherError::network(
                ErrorCode::HostUnreachable,
                "offline",
            ))
        })
        .await;
        assert_eq!(none, None);
    }

    #[tokio::test]
    async fn a_strange_uuid_is_never_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let got = cached_texture(dir.path(), "../steve", HOUR, |_| async {
            panic!("not a profile id")
        })
        .await;
        assert_eq!(got, None);
    }
}

//! A player's skin texture: a PNG checked and normalised to 64 × 64 RGBA, the one shape the
//! launcher keeps and uploads. A legacy 64 × 32 skin gets its left arm and leg the way the game
//! makes them, by mirroring the right ones.
use std::io::Cursor;

use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageFormat, ImageReader, Limits, RgbaImage};

use crate::error::{ErrorCategory, ErrorCode, LauncherError, Result};

/// The most a skin file may be before it is read. A real 64 × 64 skin is a few KB.
pub const MAX_SKIN_BYTES: usize = 1 << 20;
/// A skin is 64 texels wide.
pub const SIZE: u32 = 64;
const PNG_URL: &str = "data:image/png;base64,";

fn invalid() -> LauncherError {
    LauncherError::new(
        ErrorCategory::Integrity,
        ErrorCode::SkinInvalid,
        "That file isn't a Minecraft skin. A skin is a PNG of 64 × 64 or 64 × 32 pixels.",
    )
    .with_fix("Pick a skin PNG of 64 × 64 or 64 × 32 pixels.")
}

/// `bytes`, a skin PNG, as a 64 × 64 RGBA PNG; `SkinInvalid` for anything else.
pub fn normalise(bytes: &[u8]) -> Result<Vec<u8>> {
    encode(&decode(bytes)?)
}

/// `bytes`, a skin PNG, as 64 × 64 RGBA texels.
pub fn decode(bytes: &[u8]) -> Result<RgbaImage> {
    if bytes.len() > MAX_SKIN_BYTES {
        return Err(invalid().with_detail(format!("{} bytes", bytes.len())));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| invalid())?;
    if reader.format() != Some(ImageFormat::Png) {
        return Err(invalid().with_detail("not a PNG"));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(SIZE);
    limits.max_image_height = Some(SIZE);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| invalid().with_detail(e.to_string()))?
        .to_rgba8();
    match (image.width(), image.height()) {
        (64, 64) => Ok(image),
        (64, 32) => Ok(from_legacy(&image)),
        (w, h) => Err(invalid().with_detail(format!("{w} × {h}"))),
    }
}

/// `image` as PNG bytes.
pub fn encode(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| invalid().with_detail(e.to_string()))?;
    Ok(out)
}

/// The copies the game makes from a 64 × 32 skin: source x, y, offset dx, dy, width, height.
/// Each rectangle is mirrored left to right as it is copied.
const LEGACY_COPIES: [(u32, u32, i32, i32, u32, u32); 12] = [
    (4, 16, 16, 32, 4, 4),
    (8, 16, 16, 32, 4, 4),
    (0, 20, 24, 32, 4, 12),
    (4, 20, 16, 32, 4, 12),
    (8, 20, 8, 32, 4, 12),
    (12, 20, 16, 32, 4, 12),
    (44, 16, -8, 32, 4, 4),
    (48, 16, -8, 32, 4, 4),
    (40, 20, 0, 32, 4, 12),
    (44, 20, -8, 32, 4, 12),
    (48, 20, -16, 32, 4, 12),
    (52, 20, -8, 32, 4, 12),
];

fn from_legacy(old: &RgbaImage) -> RgbaImage {
    let mut image = RgbaImage::new(SIZE, SIZE);
    image::imageops::replace(&mut image, old, 0, 0);
    for (x, y, dx, dy, w, h) in LEGACY_COPIES {
        for row in 0..h {
            for col in 0..w {
                let from = *old.get_pixel(x + col, y + row);
                let to_x = (x as i32 + dx) as u32 + (w - 1 - col);
                let to_y = (y as i32 + dy) as u32 + row;
                image.put_pixel(to_x, to_y, from);
            }
        }
    }
    image
}

/// The arm model a skin is drawn for, read off its texels: slim arms leave the last column of
/// the right arm's back face (x 54, y 20–31) empty, which a classic skin paints.
pub fn guess_model(image: &RgbaImage) -> crate::skins_library::SkinModel {
    let empty = (20..32).all(|y| image.get_pixel(54, y)[3] == 0);
    if empty {
        crate::skins_library::SkinModel::Slim
    } else {
        crate::skins_library::SkinModel::Classic
    }
}

/// `png` as a `data:` URL the webview can show.
pub fn data_url(png: &[u8]) -> String {
    format!("{PNG_URL}{}", STANDARD.encode(png))
}

/// The PNG bytes of a `data:image/png;base64,` URL, normalised; `SkinInvalid` otherwise.
pub fn from_data_url(url: &str) -> Result<Vec<u8>> {
    let encoded = url.strip_prefix(PNG_URL).ok_or_else(invalid)?;
    if encoded.len() > MAX_SKIN_BYTES * 4 / 3 + 4 {
        return Err(invalid());
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| invalid())?;
    normalise(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma, Rgba};

    fn png(image: &RgbaImage) -> Vec<u8> {
        encode(image).unwrap()
    }

    #[test]
    fn square_skin_round_trips() {
        let mut image = RgbaImage::new(64, 64);
        image.put_pixel(9, 9, Rgba([10, 20, 30, 255]));
        image.put_pixel(40, 8, Rgba([1, 2, 3, 128]));
        let out = decode(&normalise(&png(&image)).unwrap()).unwrap();
        assert_eq!(out, image);
    }

    #[test]
    fn legacy_skin_gets_left_limbs() {
        let mut old = RgbaImage::new(64, 32);
        // The right leg's front face spans x 4..8, y 20..32; mark its leftmost column.
        let red = Rgba([200, 0, 0, 255]);
        for y in 20..32 {
            old.put_pixel(4, y, red);
        }
        // The right arm's front face spans x 44..48; mark its top-left texel.
        let blue = Rgba([0, 0, 200, 255]);
        old.put_pixel(44, 20, blue);
        let out = decode(&png(&old)).unwrap();
        assert_eq!((out.width(), out.height()), (64, 64));
        // The left leg's front face is x 20..24, y 52..64, mirrored: column 4 lands at 23.
        for y in 52..64 {
            assert_eq!(*out.get_pixel(23, y), red);
        }
        assert_eq!(out.get_pixel(20, 52)[3], 0);
        // The left arm's front face is x 36..40, y 52..64: texel (44,20) lands at (39,52).
        assert_eq!(*out.get_pixel(39, 52), blue);
        // The original top half is kept.
        assert_eq!(*out.get_pixel(4, 20), red);
    }

    #[test]
    fn wrong_size_is_skin_invalid() {
        let e = normalise(&png(&RgbaImage::new(32, 32))).unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinInvalid);
        let e = normalise(&png(&RgbaImage::new(128, 128))).unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinInvalid);
    }

    #[test]
    fn too_big_is_skin_invalid() {
        let e = normalise(&vec![0; MAX_SKIN_BYTES + 1]).unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinInvalid);
    }

    #[test]
    fn not_png_is_skin_invalid() {
        let e = normalise(b"GIF89a not a skin").unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinInvalid);
    }

    #[test]
    fn greyscale_png_normalises_to_rgba() {
        let grey = GrayImage::from_pixel(64, 64, Luma([90]));
        let mut bytes = Vec::new();
        grey.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        let out = decode(&normalise(&bytes).unwrap()).unwrap();
        assert_eq!(*out.get_pixel(0, 0), Rgba([90, 90, 90, 255]));
    }

    #[test]
    fn guess_model_reads_the_slim_gap() {
        use crate::skins_library::SkinModel;
        let mut image = RgbaImage::new(64, 64);
        assert_eq!(guess_model(&image), SkinModel::Slim);
        for y in 20..32 {
            image.put_pixel(54, y, Rgba([1, 1, 1, 255]));
        }
        assert_eq!(guess_model(&image), SkinModel::Classic);
    }

    #[test]
    fn data_url_round_trips() {
        let mut image = RgbaImage::new(64, 64);
        image.put_pixel(1, 1, Rgba([5, 6, 7, 255]));
        let url = data_url(&png(&image));
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(decode(&from_data_url(&url).unwrap()).unwrap(), image);
        assert_eq!(
            from_data_url("data:image/jpeg;base64,AAAA")
                .unwrap_err()
                .code,
            ErrorCode::SkinInvalid
        );
    }
}

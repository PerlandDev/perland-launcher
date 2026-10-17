//! The player's own pictures (an upload, a screenshot): a big one is scaled down to what the
//! launcher shows instead of refused, and the screenshot picker gets small thumbnails.
use std::io::Cursor;

use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, DynamicImage, ImageReader, Limits};

/// The most a picked file may be before it is even read: a 4K screenshot is well under it.
pub const MAX_SOURCE_BYTES: u64 = 64 << 20;
/// A background is never drawn larger than this, so a bigger picture is scaled down to fit it.
pub const BACKGROUND_FIT: (u32, u32) = (2560, 1440);
/// The picker's tiles are about 13rem wide; this covers a 2x screen.
pub const THUMBNAIL_FIT: (u32, u32) = (480, 270);
const JPEG_QUALITY: u8 = 85;
/// No real picture is wider or taller, and the limit keeps a crafted file from taking all memory.
const MAX_SIDE: u32 = 16_384;

fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    reader.limits(limits);
    reader.decode().ok()
}

fn jpeg(image: &DynamicImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&image.to_rgb8())
        .ok()?;
    Some(out)
}

/// `bytes`, a picture of type `ext`, ready to keep: as it is when it is `max` bytes or less,
/// else scaled down to fit within `bound` (never up) as a JPEG. `None` when it can't be read
/// as a picture or is still too big.
pub fn fit(
    bytes: Vec<u8>,
    ext: &'static str,
    max: usize,
    bound: (u32, u32),
) -> Option<(Vec<u8>, &'static str)> {
    if bytes.len() <= max {
        return Some((bytes, ext));
    }
    let image = decode(&bytes)?;
    let (w, h) = bound;
    let image = if image.width() > w || image.height() > h {
        image.resize(w, h, FilterType::Triangle)
    } else {
        image
    };
    let out = jpeg(&image)?;
    (out.len() <= max).then_some((out, "jpg"))
}

/// A small JPEG of `bytes` that fits within `bound`, or `None` when it isn't a picture.
pub fn thumbnail(bytes: &[u8], bound: (u32, u32)) -> Option<Vec<u8>> {
    let image = decode(bytes)?;
    let (w, h) = bound;
    let image = if image.width() > w || image.height() > h {
        image.thumbnail(w, h)
    } else {
        image
    };
    jpeg(&image)
}

#[cfg(test)]
mod tests {
    use image::{ImageFormat, RgbImage};

    use super::*;

    /// A `w`×`h` PNG of noise, so it doesn't compress away.
    fn noisy_png(w: u32, h: u32) -> Vec<u8> {
        let mut seed = 7u32;
        let image = RgbImage::from_fn(w, h, |_, _| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let [a, b, c, _] = seed.to_le_bytes();
            image::Rgb([a, b, c])
        });
        let mut out = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn size(bytes: &[u8]) -> (u32, u32) {
        let image = decode(bytes).unwrap();
        (image.width(), image.height())
    }

    #[test]
    fn a_picture_within_the_limit_is_kept_as_it_is() {
        let png = noisy_png(30, 20);
        let (out, ext) = fit(png.clone(), "png", png.len(), (10, 10)).unwrap();
        assert_eq!((out, ext), (png, "png"));
    }

    #[test]
    fn a_bigger_picture_is_scaled_down_to_a_jpeg() {
        let png = noisy_png(300, 200);
        let (out, ext) = fit(png.clone(), "png", png.len() / 4, (120, 120)).unwrap();
        assert_eq!(ext, "jpg");
        assert!(out.len() <= png.len() / 4);
        assert_eq!(size(&out), (120, 80), "the shape is kept");
    }

    #[test]
    fn a_picture_that_is_big_only_in_bytes_keeps_its_size() {
        let png = noisy_png(60, 40);
        let (out, ext) = fit(png.clone(), "png", png.len() - 1, (2560, 1440)).unwrap();
        assert_eq!(ext, "jpg");
        assert_eq!(size(&out), (60, 40), "never scaled up or down without need");
    }

    #[test]
    fn what_is_not_a_picture_is_refused() {
        assert_eq!(
            fit(b"\x89PNG\r\n\x1a\nbroken".to_vec(), "png", 4, (10, 10)),
            None
        );
        assert_eq!(thumbnail(b"<html>not a png</html>", (10, 10)), None);
    }

    #[test]
    fn a_thumbnail_fits_its_bound() {
        assert_eq!(
            size(&thumbnail(&noisy_png(300, 200), (60, 60)).unwrap()),
            (60, 40)
        );
        assert_eq!(
            size(&thumbnail(&noisy_png(30, 20), (60, 60)).unwrap()),
            (30, 20)
        );
    }
}

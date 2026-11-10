//! The profile picture: a square JPEG sealed under the vault key (`docs/protocol.md`, "Profile
//! picture"), so the server keeps it without being able to show it.

use std::io::Cursor;
use std::sync::Arc;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;

use super::crypto::{self, CryptoError, Key};

pub const AAD: &[u8] = b"avatar";
/// Pixels across.
const SIDE: u32 = 512;
const QUALITY: u8 = 82;

/// The picture an account shows, with the version the server gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub version: u64,
    pub jpeg: Arc<[u8]>,
}

/// The centre square of an image file, 512 pixels across, as a JPEG. `None` for bytes that are
/// not a picture this build can read.
pub fn square_jpeg(file: &[u8]) -> Option<Vec<u8>> {
    let decoded = image::load_from_memory(file).ok()?;
    let side = decoded.width().min(decoded.height());
    if side == 0 {
        return None;
    }
    let (x, y) = ((decoded.width() - side) / 2, (decoded.height() - side) / 2);
    let square = decoded.crop_imm(x, y, side, side).resize_exact(SIDE, SIDE, FilterType::Lanczos3).into_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut out), QUALITY).encode_image(&square).ok()?;
    Some(out)
}

pub fn seal(vault: &Key, jpeg: &[u8]) -> Result<String, CryptoError> {
    Ok(crypto::to_base64(&crypto::seal(vault, jpeg, AAD)?))
}

pub fn open(vault: &Key, blob: &str) -> Result<Vec<u8>, CryptoError> {
    crypto::open(vault, &crypto::from_base64(blob)?, AAD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_picture_becomes_its_centre_square() {
        let wide = image::RgbImage::from_fn(300, 100, |x, _| {
            if (100..200).contains(&x) { [255, 0, 0] } else { [0, 0, 255] }.into()
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(wide).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let jpeg = square_jpeg(&png).unwrap();
        let back = image::load_from_memory(&jpeg).unwrap().into_rgb8();
        assert_eq!((back.width(), back.height()), (SIDE, SIDE));
        for (x, y) in [(10, 10), (256, 256), (500, 500)] {
            let [r, _, b] = back.get_pixel(x, y).0;
            assert!(r > 200 && b < 60, "pixel {x},{y} is not from the centre: {r} {b}");
        }
        assert_eq!(square_jpeg(b"not a picture"), None);
    }

    #[test]
    fn a_sealed_picture_opens_only_under_its_own_key() {
        let key = [7; 32];
        let blob = seal(&key, b"jpeg bytes").unwrap();
        assert_eq!(open(&key, &blob).unwrap(), b"jpeg bytes");
        assert!(open(&[8; 32], &blob).is_err());
    }
}

//! Rendering of a player's head and export of the raw skin texture.
//!
//! Pure image manipulation, no network access.

use image::imageops::{self, FilterType};
use image::{ImageFormat, RgbaImage};
use std::io::Cursor;

/// Upscale factor of the 8x8 head canvas (128x128 output).
const HEAD_SCALE: u32 = 16;

/// Arm model of a skin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SkinModel {
    /// 4 pixels wide arms (Steve).
    Classic,
    /// 3 pixels wide arms (Alex).
    Slim,
}

impl SkinModel {
    /// Name of the model as used by Mojang's profile API.
    pub fn as_str(self) -> &'static str {
        match self {
            SkinModel::Classic => "classic",
            SkinModel::Slim => "slim",
        }
    }
}

/// A texture region: (x, y, width, height).
type Region = (u32, u32, u32, u32);

/// Whether the texture is the 64x32 pre-1.8 format (no left limbs, no outer layers).
fn is_legacy(texture: &RgbaImage) -> bool {
    texture.dimensions() == (64, 32)
}

fn check_size(texture: &RgbaImage) -> Result<(), String> {
    match texture.dimensions() {
        (64, 64) | (64, 32) => Ok(()),
        (w, h) => Err(format!("unexpected skin texture size {w}x{h}")),
    }
}

fn crop(texture: &RgbaImage, (x, y, w, h): Region) -> RgbaImage {
    imageops::crop_imm(texture, x, y, w, h).to_image()
}

fn has_transparency(image: &RgbaImage) -> bool {
    image.pixels().any(|p| p[3] < 255)
}

/// Draws `part` on `canvas` at (x, y), respecting its alpha.
fn paste(canvas: &mut RgbaImage, part: &RgbaImage, x: u32, y: u32) {
    imageops::overlay(canvas, part, i64::from(x), i64::from(y));
}

/// Renders the head (face plus hat layer) as a 128x128 image.
pub fn render_head(texture: &RgbaImage) -> Result<RgbaImage, String> {
    check_size(texture)?;
    let mut canvas = RgbaImage::new(8, 8);
    paste(&mut canvas, &crop(texture, (8, 8, 8, 8)), 0, 0);
    let hat = crop(texture, (40, 8, 8, 8));
    // On legacy skins the hat area is only meaningful if it has transparency.
    if !is_legacy(texture) || has_transparency(&hat) {
        paste(&mut canvas, &hat, 0, 0);
    }
    Ok(imageops::resize(
        &canvas,
        8 * HEAD_SCALE,
        8 * HEAD_SCALE,
        FilterType::Nearest,
    ))
}

/// Encodes the raw skin texture (64x64, or 64x32 for legacy skins) as PNG.
pub fn texture_png(texture: &RgbaImage) -> Result<Vec<u8>, String> {
    check_size(texture)?;
    encode_png(texture)
}

/// Encodes an image as PNG.
pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| format!("failed to encode PNG: {e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);
    const BLUE: Rgba<u8> = Rgba([0, 0, 255, 255]);
    const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);

    fn texture(w: u32, h: u32) -> RgbaImage {
        RgbaImage::new(w, h)
    }

    fn fill(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
        for py in y..y + h {
            for px in x..x + w {
                img.put_pixel(px, py, color);
            }
        }
    }

    #[test]
    fn head_is_128_and_uses_face_then_hat() {
        let mut tex = texture(64, 64);
        fill(&mut tex, 8, 8, 8, 8, RED);
        tex.put_pixel(40, 8, BLUE);
        let head = render_head(&tex).unwrap();
        assert_eq!(head.dimensions(), (128, 128));
        assert_eq!(*head.get_pixel(0, 0), BLUE);
        assert_eq!(*head.get_pixel(127, 127), RED);
    }

    #[test]
    fn legacy_opaque_hat_is_ignored() {
        let mut tex = texture(64, 32);
        fill(&mut tex, 8, 8, 8, 8, RED);
        fill(&mut tex, 40, 8, 8, 8, BLACK);
        let head = render_head(&tex).unwrap();
        assert_eq!(*head.get_pixel(0, 0), RED);
    }

    #[test]
    fn rejects_unexpected_texture_size() {
        let tex = texture(128, 128);
        assert!(render_head(&tex).is_err());
    }

    #[test]
    fn texture_png_keeps_texture() {
        let mut tex = texture(64, 64);
        tex.put_pixel(8, 8, RED);
        let decoded = image::load_from_memory(&texture_png(&tex).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (64, 64));
        assert_eq!(*decoded.get_pixel(8, 8), RED);
    }

    #[test]
    fn texture_png_accepts_legacy() {
        let bytes = texture_png(&texture(64, 32)).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 32));
    }

    #[test]
    fn texture_png_rejects_unexpected_size() {
        assert!(texture_png(&texture(128, 128)).is_err());
    }

    #[test]
    fn encode_png_round_trips() {
        let img = texture(128, 256);
        let bytes = encode_png(&img).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (128, 256));
    }
}

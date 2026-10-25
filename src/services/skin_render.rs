//! Rendering of a player's head and front-facing body from a Minecraft skin texture.
//!
//! Pure image manipulation, no network access.

use image::imageops::{self, FilterType};
use image::{ImageFormat, RgbaImage};
use std::io::Cursor;

/// Upscale factor of the 8x8 head canvas (128x128 output).
const HEAD_SCALE: u32 = 16;
/// Upscale factor of the 16x32 body canvas (128x256 output).
const BODY_SCALE: u32 = 8;

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

    fn arm_width(self) -> u32 {
        match self {
            SkinModel::Classic => 4,
            SkinModel::Slim => 3,
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

/// Renders the front of the body (head, torso, arms, legs) as a 128x256 image.
pub fn render_body(texture: &RgbaImage, model: SkinModel) -> Result<RgbaImage, String> {
    check_size(texture)?;
    let legacy = is_legacy(texture);
    let arm = model.arm_width();
    let mut canvas = RgbaImage::new(16, 32);

    // (base region, outer layer region, position on the canvas)
    let head: (Region, Region, (u32, u32)) = ((8, 8, 8, 8), (40, 8, 8, 8), (4, 0));
    let torso: (Region, Region, (u32, u32)) = ((20, 20, 8, 12), (20, 36, 8, 12), (4, 8));
    let right_arm: (Region, Region, (u32, u32)) =
        ((44, 20, arm, 12), (44, 36, arm, 12), (4 - arm, 8));
    let right_leg: (Region, Region, (u32, u32)) = ((4, 20, 4, 12), (4, 36, 4, 12), (4, 20));
    let left_arm: (Region, Region, (u32, u32)) = ((36, 52, arm, 12), (52, 52, arm, 12), (12, 8));
    let left_leg: (Region, Region, (u32, u32)) = ((20, 52, 4, 12), (4, 52, 4, 12), (8, 20));

    for (base, outer, (x, y)) in [head, torso, right_arm, right_leg] {
        paste(&mut canvas, &crop(texture, base), x, y);
        let layer = crop(texture, outer);
        // Legacy skins only have a hat layer, ignored when fully opaque.
        if !legacy || (outer == head.1 && has_transparency(&layer)) {
            paste(&mut canvas, &layer, x, y);
        }
    }

    if legacy {
        // Left limbs are the mirrored right ones.
        let mirrored_arm = imageops::flip_horizontal(&crop(texture, right_arm.0));
        let mirrored_leg = imageops::flip_horizontal(&crop(texture, right_leg.0));
        paste(&mut canvas, &mirrored_arm, left_arm.2.0, left_arm.2.1);
        paste(&mut canvas, &mirrored_leg, left_leg.2.0, left_leg.2.1);
    } else {
        for (base, outer, (x, y)) in [left_arm, left_leg] {
            paste(&mut canvas, &crop(texture, base), x, y);
            paste(&mut canvas, &crop(texture, outer), x, y);
        }
    }

    Ok(imageops::resize(
        &canvas,
        16 * BODY_SCALE,
        32 * BODY_SCALE,
        FilterType::Nearest,
    ))
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
    const GREEN: Rgba<u8> = Rgba([0, 255, 0, 255]);
    const YELLOW: Rgba<u8> = Rgba([255, 255, 0, 255]);
    const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
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
    fn body_is_128x256_with_parts_in_place() {
        let mut tex = texture(64, 64);
        fill(&mut tex, 20, 20, 8, 12, GREEN);
        fill(&mut tex, 20, 52, 4, 12, YELLOW);
        let body = render_body(&tex, SkinModel::Classic).unwrap();
        assert_eq!(body.dimensions(), (128, 256));
        assert_eq!(*body.get_pixel(4 * 8, 8 * 8), GREEN);
        assert_eq!(*body.get_pixel(8 * 8, 20 * 8), YELLOW);
        assert_eq!(body.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn slim_arms_are_three_pixels_wide() {
        let mut tex = texture(64, 64);
        fill(&mut tex, 44, 20, 3, 12, WHITE);
        let slim = render_body(&tex, SkinModel::Slim).unwrap();
        assert_eq!(slim.get_pixel(8, 8 * 8)[3], 255);
        assert_eq!(slim.get_pixel(0, 8 * 8)[3], 0);
        let classic = render_body(&tex, SkinModel::Classic).unwrap();
        assert_eq!(classic.get_pixel(0, 8 * 8)[3], 255);
    }

    #[test]
    fn legacy_texture_mirrors_right_limbs() {
        let mut tex = texture(64, 32);
        fill(&mut tex, 44, 20, 4, 12, WHITE);
        fill(&mut tex, 44, 20, 1, 12, RED);
        let body = render_body(&tex, SkinModel::Classic).unwrap();
        assert_eq!(*body.get_pixel(0, 8 * 8), RED);
        assert_eq!(*body.get_pixel(15 * 8, 8 * 8), RED);
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
        assert!(render_body(&tex, SkinModel::Classic).is_err());
    }

    #[test]
    fn encode_png_round_trips() {
        let img = texture(128, 256);
        let bytes = encode_png(&img).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (128, 256));
    }
}

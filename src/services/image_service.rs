use tracing::debug;
use crate::models::{ImageRequest, ImageResponse, DiscordUser};
use crate::utils::{formatters, constants::*};
use std::fs;
use std::path::Path;
use image::{RgbaImage, Rgba, imageops::FilterType};
use rusttype::{Font, Scale};
use std::collections::HashMap;

// --- Visual Configuration Constants ---
const CARD_WIDTH: u32 = 1100;
const CARD_HEIGHT: u32 = 650;
const SIDEBAR_WIDTH: u32 = 300;

const COLOR_BG_DEEP: [u8; 4] = [15, 15, 18, 255];
const COLOR_SIDEBAR_BG: [u8; 4] = [30, 31, 34, 255];
const COLOR_ACCENT_BLURPLE: [u8; 4] = [88, 101, 242, 255];
const COLOR_WHITE: [u8; 4] = [255, 255, 255, 255];
const COLOR_GRAY_LABEL: [u8; 4] = [150, 152, 157, 255];
const COLOR_CARD_BG: [u8; 4] = [43, 45, 49, 255];
const COLOR_INNER_BADGE: [u8; 4] = [30, 31, 34, 255];
const COLOR_SEPARATOR: [u8; 4] = [40, 41, 45, 255];

const FONT_DATA: &[u8] = include_bytes!("../../public/font/ARIAL.TTF");

const OUTPUT_DIR: &str = "public/generated_images";

/// Service for generating an image locally and returning its metadata
pub fn generate_image_mock(req: ImageRequest) -> ImageResponse {
    debug!("Generating local image for prompt: {}", req.prompt);
    let _ = fs::create_dir_all(OUTPUT_DIR);
    let file_id = uuid::Uuid::new_v4().to_string();
    let file_name = format!("{}.png", file_id);
    let file_path = Path::new(OUTPUT_DIR).join(&file_name);
    let mut img = RgbaImage::new(req.width, req.height);
    for pixel in img.pixels_mut() { *pixel = Rgba(COLOR_SIDEBAR_BG); }
    let _ = img.save(&file_path);
    ImageResponse { url: format!("/generated_images/{}", file_name) }
}

/// Helper to parse hex color strings like "#2ecc71"
fn parse_hex_color(hex: &str) -> Rgba<u8> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(88);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(101);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(242);
        Rgba([r, g, b, 255])
    } else {
        Rgba(COLOR_ACCENT_BLURPLE) 
    }
}

/// Service for generating a MASTERPIECE Discord profile image summary
pub async fn generate_discord_profil(user: DiscordUser) -> ImageResponse {
    debug!("Generating OVERHAULED profile image for user: {}", user.pseudo_discord);
    
    let _ = fs::create_dir_all(OUTPUT_DIR);
    let file_id = uuid::Uuid::new_v4().to_string();
    let file_name = format!("{}.png", file_id);
    let file_path = Path::new(OUTPUT_DIR).join(&file_name);

    let mut img = RgbaImage::new(CARD_WIDTH, CARD_HEIGHT);

    let bg_deep = Rgba(COLOR_BG_DEEP);
    let sidebar_bg = Rgba(COLOR_SIDEBAR_BG);
    let accent_blurple = Rgba(COLOR_ACCENT_BLURPLE);
    let white = Rgba(COLOR_WHITE);
    let gray_label = Rgba(COLOR_GRAY_LABEL);

    for pixel in img.pixels_mut() { *pixel = bg_deep; }

    // --- 1. SIDEBAR ---
    for x in 0..SIDEBAR_WIDTH { for y in 0..CARD_HEIGHT { img.put_pixel(x as u32, y as u32, sidebar_bg); } }
    for y in 0..CARD_HEIGHT { img.put_pixel(SIDEBAR_WIDTH, y, Rgba(COLOR_SEPARATOR)); }
    for x in 0..SIDEBAR_WIDTH { for y in 0..6 { img.put_pixel(x, y, accent_blurple); } }

    let font = Font::try_from_bytes(FONT_DATA).expect("Error loading FONT");

    // --- 2. AVATAR ---
    if let Some(avatar_url) = user.avatar_url {
        let client = reqwest::Client::new();
        if let Ok(resp) = client.get(avatar_url).send().await {
            if let Ok(bytes) = resp.bytes().await {
                if let Ok(avatar_img) = image::load_from_memory(&bytes) {
                    let mut avatar = avatar_img.resize_exact(200, 200, FilterType::Lanczos3).to_rgba8();
                    for ax in 0..200 {
                        for ay in 0..200 {
                            let dist = ((ax as f32 - 100.0).powi(2) + (ay as f32 - 100.0).powi(2)).sqrt();
                            if dist > 100.0 { avatar.get_pixel_mut(ax, ay).0[3] = 0; }
                            else if dist > 96.0 {
                                let p = avatar.get_pixel_mut(ax, ay);
                                p.0[0] = 255; p.0[1] = 255; p.0[2] = 255; p.0[3] = 255;
                            }
                        }
                    }
                    image::imageops::overlay(&mut img, &avatar, 50, 60);
                }
            }
        }
    }

    // --- 3. IDENTITY ---
    let pseudo_truncated = formatters::truncate_text(&user.pseudo_discord, 15);
    draw_text_centered_rgba(&mut img, &font, &pseudo_truncated, 150, 280, Scale::uniform(38.0), white);
    draw_text_centered_rgba(&mut img, &font, &format!("{}", user.tag_discord), 150, 325, Scale::uniform(22.0), gray_label);
    
    let formatted_join = formatters::format_discord_date(&user.join_date_discord);
    draw_text_centered_rgba(&mut img, &font, LABEL_MEMBER_SINCE, 150, 380, Scale::uniform(14.0), gray_label);
    draw_text_centered_rgba(&mut img, &font, &formatted_join, 150, 405, Scale::uniform(18.0), white);

    // --- 4. DATA ---
    let total_messages: i64 = user.stats.iter().map(|s| s.nb_message).sum();
    let total_vocal_decimal: f64 = user.stats.iter().map(|s| s.vocal_time.parse::<f64>().unwrap_or(0.0)).sum();

    let mut text_counts = HashMap::new();
    let mut voice_counts = HashMap::new();
    let mut companion_counts = HashMap::new();
    for stat in &user.stats {
        for ch in &stat.text_channels { *text_counts.entry(ch.name.clone()).or_insert(0) += 1; }
        for ch in &stat.voice_channels { *voice_counts.entry(ch.name.clone()).or_insert(0) += 1; }
        for comp in &stat.vocal_with { *companion_counts.entry(comp.username.clone()).or_insert(0) += 1; }
    }
    let top_text = text_counts.into_iter().max_by_key(|&(_, c)| c).map(|(n, _)| n).unwrap_or_else(|| "Aucun".to_string());
    let top_voice = voice_counts.into_iter().max_by_key(|&(_, c)| c).map(|(n, _)| n).unwrap_or_else(|| "Aucun".to_string());
    let top_companion = companion_counts.into_iter().max_by_key(|&(_, c)| c).map(|(n, _)| n).unwrap_or_else(|| "Aucun".to_string());

    let filtered_roles: Vec<_> = user.roles.iter()
        .filter(|r| r.name.to_lowercase().contains("loutre") || r.name.to_lowercase().contains("rôle"))
        .take(8).collect();

    // --- 5. STATS GRID ---
    let grid_x = 350;
    let grid_y = 60;
    let card_w = 340;
    let card_h = 120;

    draw_stat_card(&mut img, grid_x, grid_y, card_w, card_h, LABEL_MESSAGES, &format!("{}", total_messages), &font);
    draw_stat_card(&mut img, grid_x + 370, grid_y, card_w, card_h, LABEL_VOCAL_TIME, &formatters::format_vocal_time(total_vocal_decimal), &font);
    draw_stat_card(&mut img, grid_x, grid_y + 150, card_w, card_h, LABEL_COMPANION, &formatters::truncate_text(&top_companion, 20), &font);
    draw_stat_card(&mut img, grid_x + 370, grid_y + 150, card_w, card_h, LABEL_TEXT_CHANNEL, &formatters::truncate_text(&top_text, 20), &font);
    draw_stat_card(&mut img, grid_x, grid_y + 300, 710, card_h, LABEL_VOICE_CHANNEL, &formatters::truncate_text(&top_voice, 40), &font);

    // --- 6. ROLES SECTION ---
    let roles_label_y = grid_y + 300 + card_h + 30; 
    draw_text_rgba(&mut img, &font, LABEL_ROLES_TITLE, grid_x as i32, roles_label_y as i32, Scale::uniform(18.0), gray_label);
    
    let mut rx = grid_x as i32;
    let mut ry = roles_label_y as i32 + 35;
    for role in filtered_roles {
        let color = parse_hex_color(&role.color);
        let w = draw_pill_high_fidelity(&mut img, rx, ry, &role.name, &font, color);
        rx += w + 12;
        if rx > CARD_WIDTH as i32 - 150 { rx = grid_x as i32; ry += 45; }
    }

    let _ = img.save(&file_path);
    ImageResponse { url: format!("/generated_images/{}", file_name) }
}

fn draw_stat_card(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, label: &str, val: &str, font: &Font) {
    let card_bg = Rgba(COLOR_CARD_BG);
    let gray_label = Rgba(COLOR_GRAY_LABEL);
    let white = Rgba(COLOR_WHITE);
    for bx in x..(x + w) { for by in y..(y + h) { if bx < img.width() && by < img.height() { img.put_pixel(bx, by, card_bg); } } }
    for bx in x..(x + w) { img.put_pixel(bx, y + h - 1, Rgba([88, 101, 242, 100])); }
    draw_text_rgba(img, font, label, (x + 25) as i32, (y + 25) as i32, Scale::uniform(15.0), gray_label);
    draw_text_rgba(img, font, val, (x + 25) as i32, (y + 55) as i32, Scale::uniform(36.0), white);
}

fn draw_pill_high_fidelity(img: &mut RgbaImage, x: i32, y: i32, text: &str, font: &Font, color: Rgba<u8>) -> i32 {
    let inner_bg = Rgba(COLOR_INNER_BADGE);
    let text_len = text.chars().count() as i32 * 11;
    let width = (text_len + 40).max(120);
    let height = 34;
    let radius = 17.0;
    let border_width = 2.0;

    for dx in 0..width {
        for dy in 0..height {
            let px = x + dx; let py = y + dy;
            if px < 0 || px >= img.width() as i32 || py < 0 || py >= img.height() as i32 { continue; }
            let cx = if dx < radius as i32 { radius } else if dx >= width - radius as i32 { width as f32 - radius } else { dx as f32 };
            let cy = radius;
            let dist = ((dx as f32 - cx).powi(2) + (dy as f32 - cy).powi(2)).sqrt();

            if dist <= radius {
                let current_pixel = img.get_pixel(px as u32, py as u32);
                let mut target_color = inner_bg;
                if dist > radius - border_width {
                    let alpha = (radius - dist).clamp(0.0, 1.0);
                    let inv_alpha = 1.0 - alpha;
                    target_color = Rgba([
                        (color.0[0] as f32 * alpha + inner_bg.0[0] as f32 * inv_alpha) as u8,
                        (color.0[1] as f32 * alpha + inner_bg.0[1] as f32 * inv_alpha) as u8,
                        (color.0[2] as f32 * alpha + inner_bg.0[2] as f32 * inv_alpha) as u8,
                        255
                    ]);
                }
                if dist > radius - 1.0 {
                    let edge_alpha = (radius - dist).clamp(0.0, 1.0);
                    let inv_edge_alpha = 1.0 - edge_alpha;
                    let fr = (target_color.0[0] as f32 * edge_alpha + current_pixel.0[0] as f32 * inv_edge_alpha) as u8;
                    let fg = (target_color.0[1] as f32 * edge_alpha + current_pixel.0[1] as f32 * inv_edge_alpha) as u8;
                    let fb = (target_color.0[2] as f32 * edge_alpha + current_pixel.0[2] as f32 * inv_edge_alpha) as u8;
                    img.put_pixel(px as u32, py as u32, Rgba([fr, fg, fb, 255]));
                } else {
                    img.put_pixel(px as u32, py as u32, target_color);
                }
            }
        }
    }
    draw_text_rgba(img, font, text, x + 20, y + 8, Scale::uniform(16.0), Rgba(COLOR_WHITE));
    width
}

fn draw_text_rgba(img: &mut RgbaImage, font: &Font, text: &str, x: i32, y: i32, scale: Scale, color: Rgba<u8>) {
    let v_metrics = font.v_metrics(scale);
    let offset = rusttype::point(x as f32, y as f32 + v_metrics.ascent);
    for glyph in font.layout(text, scale, offset) {
        if let Some(bb) = glyph.pixel_bounding_box() {
            glyph.draw(|gx, gy, gv| {
                let px = bb.min.x + gx as i32; let py = bb.min.y + gy as i32;
                if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
                    let pixel = img.get_pixel_mut(px as u32, py as u32);
                    if gv > 0.05 {
                        let alpha = gv; let inv_alpha = 1.0 - alpha;
                        pixel.0[0] = (pixel.0[0] as f32 * inv_alpha + color.0[0] as f32 * alpha) as u8;
                        pixel.0[1] = (pixel.0[1] as f32 * inv_alpha + color.0[1] as f32 * alpha) as u8;
                        pixel.0[2] = (pixel.0[2] as f32 * inv_alpha + color.0[2] as f32 * alpha) as u8;
                        pixel.0[3] = 255;
                    }
                }
            });
        }
    }
}

fn draw_text_centered_rgba(img: &mut RgbaImage, font: &Font, text: &str, center_x: i32, y: i32, scale: Scale, color: Rgba<u8>) {
    let glyphs: Vec<_> = font.layout(text, scale, rusttype::point(0.0, 0.0)).collect();
    let width = glyphs.iter().rev().filter_map(|g| g.pixel_bounding_box().map(|b| b.max.x)).next().unwrap_or(0);
    let start_x = center_x - (width / 2);
    draw_text_rgba(img, font, text, start_x, y, scale, color);
}

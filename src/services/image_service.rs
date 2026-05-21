use tracing::debug;
use crate::models::{ImageRequest, ImageResponse, DiscordUser};
use crate::utils::formatters;
use std::fs;
use std::path::Path;
use image::{RgbaImage, Rgba, imageops::FilterType};
use rusttype::{Font, Scale};
use std::collections::HashMap;

/// Service for generating an image locally and returning its metadata
pub fn generate_image_mock(req: ImageRequest) -> ImageResponse {
    debug!("Generating local image for prompt: {}", req.prompt);
    let output_dir = "public/generated_images";
    let _ = fs::create_dir_all(output_dir);
    let file_id = uuid::Uuid::new_v4().to_string();
    let file_name = format!("{}.png", file_id);
    let file_path = Path::new(output_dir).join(&file_name);
    let mut img = RgbaImage::new(req.width, req.height);
    for pixel in img.pixels_mut() { *pixel = Rgba([30, 30, 35, 255]); }
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
        Rgba([88, 101, 242, 255]) 
    }
}

/// Service for generating an ULTRA MODERN Discord profile image
pub async fn generate_discord_profil(user: DiscordUser) -> ImageResponse {
    debug!("Generating HIGH-END profile image for user: {}", user.pseudo_discord);
    
    let output_dir = "public/generated_images";
    let _ = fs::create_dir_all(output_dir);
    let file_id = uuid::Uuid::new_v4().to_string();
    let file_name = format!("{}.png", file_id);
    let file_path = Path::new(output_dir).join(&file_name);

    // Canvas: 1000x600 for better spacing
    let width = 1000;
    let height = 600;
    let mut img = RgbaImage::new(width, height);

    // --- 1. Background: Deep Gradient Look ---
    let bg_color = Rgba([15, 15, 18, 255]);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        // Subtle vertical gradient
        let val = (y as f32 / height as f32 * 10.0) as u8;
        *pixel = Rgba([bg_color.0[0] + val, bg_color.0[1] + val, bg_color.0[2] + val, 255]);
    }

    // --- 2. Load Fonts ---
    let font_bold = Font::try_from_bytes(include_bytes!("C:\\Windows\\Fonts\\arialbd.ttf") as &[u8]).expect("E1");
    let font_reg = Font::try_from_bytes(include_bytes!("C:\\Windows\\Fonts\\arial.ttf") as &[u8]).expect("E2");

    // --- 3. Data Processing ---
    let total_messages: i64 = user.stats.iter().map(|s| s.nb_message).sum();
    let total_vocal_decimal: f64 = user.stats.iter().map(|s| s.vocal_time.parse::<f64>().unwrap_or(0.0)).sum();
    
    let mut text_counts = HashMap::new();
    let mut voice_counts = HashMap::new();
    for stat in &user.stats {
        for ch in &stat.text_channels { *text_counts.entry(ch.name.clone()).or_insert(0) += 1; }
        for ch in &stat.voice_channels { *voice_counts.entry(ch.name.clone()).or_insert(0) += 1; }
    }
    let top_text = formatters::truncate_text(&text_counts.into_iter().max_by_key(|&(_, c)| c).map(|(n, _)| n).unwrap_or("None".into()), 20);
    let top_voice = formatters::truncate_text(&voice_counts.into_iter().max_by_key(|&(_, c)| c).map(|(n, _)| n).unwrap_or("None".into()), 20);

    let filtered_roles: Vec<_> = user.roles.iter()
        .filter(|r| r.name.to_lowercase().contains("loutre") || r.name.to_lowercase().contains("rôle"))
        .take(4).collect();

    // --- 4. Main Glass Container ---
    draw_rounded_rect(&mut img, 40, 40, 920, 520, 24, Rgba([30, 31, 34, 180])); // Glassmorphism container
    
    // Header Accent Line (Blurple)
    for x in 64..936 { for y in 40..43 { img.put_pixel(x, y, Rgba([88, 101, 242, 255])); } }

    // Colors
    let white = Rgba([255, 255, 255, 255]);
    let gray = Rgba([150, 152, 157, 255]);

    // --- 5. Avatar (Glowing border) ---
    if let Some(avatar_url) = user.avatar_url {
        let client = reqwest::Client::new();
        if let Ok(resp) = client.get(avatar_url).send().await {
            if let Ok(bytes) = resp.bytes().await {
                if let Ok(avatar_img) = image::load_from_memory(&bytes) {
                    let mut avatar = avatar_img.resize_exact(140, 140, FilterType::Lanczos3).to_rgba8();
                    // Circle Mask
                    for ax in 0..140 {
                        for ay in 0..140 {
                            let dist = ((ax as f32 - 70.0).powi(2) + (ay as f32 - 70.0).powi(2)).sqrt();
                            if dist > 70.0 { avatar.get_pixel_mut(ax, ay).0[3] = 0; }
                            else if dist > 68.0 { // Subtle white ring
                                let a = avatar.get_pixel_mut(ax, ay);
                                a.0[0] = 255; a.0[1] = 255; a.0[2] = 255;
                            }
                        }
                    }
                    image::imageops::overlay(&mut img, &avatar, 80, 80);
                }
            }
        }
    }

    // --- 6. Identity Header ---
    draw_text_rgba(&mut img, &font_bold, &user.pseudo_discord, 250, 90, Scale::uniform(55.0), white);
    draw_text_rgba(&mut img, &font_reg, &format!("@{}", user.tag_discord), 250, 150, Scale::uniform(26.0), gray);
    
    let join_text = format!("Membre depuis le {}", formatters::format_discord_date(&user.join_date_discord));
    draw_text_rgba(&mut img, &font_reg, &join_text, 250, 185, Scale::uniform(18.0), gray);

    // --- 7. Stats Grid (3 Boxes) ---
    let stats_y = 240;
    draw_modern_stat(&mut img, 80, stats_y, "MESSAGES ENVOYÉS", &format!("{}", total_messages), &font_bold, &font_reg);
    draw_modern_stat(&mut img, 375, stats_y, "TEMPS EN VOCAL", &formatters::format_vocal_time(total_vocal_decimal), &font_bold, &font_reg);
    draw_modern_stat(&mut img, 670, stats_y, "SALON PRÉFÉRÉ (TXT)", &top_text, &font_bold, &font_reg);

    // Second Row Stats
    draw_modern_stat(&mut img, 80, 360, "SALON PRÉFÉRÉ (VOC)", &top_voice, &font_bold, &font_reg);
    draw_modern_stat(&mut img, 375, 360, "PROFIL ID", &format!("#{}", user.id), &font_bold, &font_reg);

    // --- 8. Roles Section (Bottom) ---
    draw_text_rgba(&mut img, &font_bold, "RÔLES PRIORITAIRES", 80, 485, Scale::uniform(18.0), gray);
    let mut rx = 80;
    for role in filtered_roles {
        let color = parse_hex_color(&role.color);
        let w = draw_pill_high_end(&mut img, rx, 515, &role.name, &font_reg, color);
        rx += w + 15;
    }

    // Save
    let _ = img.save(&file_path);
    ImageResponse { url: format!("/generated_images/{}", file_name) }
}

fn draw_modern_stat(img: &mut RgbaImage, x: i32, y: i32, label: &str, val: &str, font_bold: &Font, font_reg: &Font) {
    draw_rounded_rect(img, x, y, 250, 90, 12, Rgba([43, 45, 49, 255])); // Container
    draw_text_rgba(img, font_reg, label, x + 20, y + 15, Scale::uniform(14.0), Rgba([185, 187, 190, 255]));
    draw_text_rgba(img, font_bold, val, x + 20, y + 40, Scale::uniform(28.0), Rgba([255, 255, 255, 255]));
}

fn draw_pill_high_end(img: &mut RgbaImage, x: i32, y: i32, text: &str, font: &Font, color: Rgba<u8>) -> i32 {
    let text_len = text.len() as i32 * 11;
    let width = (text_len + 40).max(120);
    let height = 34;
    draw_rounded_rect_outline(img, x, y, width, height, 17, color, 2);
    draw_text_rgba(img, font, text, x + 20, y + 8, Scale::uniform(16.0), Rgba([255, 255, 255, 255]));
    width
}

fn draw_rounded_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, r: i32, color: Rgba<u8>) {
    for dx in 0..w {
        for dy in 0..h {
            let px = x + dx;
            let py = y + dy;
            if px < 0 || px >= img.width() as i32 || py < 0 || py >= img.height() as i32 { continue; }
            let mut inside = true;
            if dx < r && dy < r { if ((dx-r).pow(2) + (dy-r).pow(2)) as f32 > (r*r) as f32 { inside = false; } }
            else if dx > w-r-1 && dy < r { if ((dx-(w-r-1)).pow(2) + (dy-r).pow(2)) as f32 > (r*r) as f32 { inside = false; } }
            else if dx < r && dy > h-r-1 { if ((dx-r).pow(2) + (dy-(h-r-1)).pow(2)) as f32 > (r*r) as f32 { inside = false; } }
            else if dx > w-r-1 && dy > h-r-1 { if ((dx-(w-r-1)).pow(2) + (dy-(h-r-1)).pow(2)) as f32 > (r*r) as f32 { inside = false; } }
            
            if inside {
                let current = img.get_pixel(px as u32, py as u32);
                let alpha = color.0[3] as f32 / 255.0;
                let inv_alpha = 1.0 - alpha;
                let nr = (current.0[0] as f32 * inv_alpha + color.0[0] as f32 * alpha) as u8;
                let ng = (current.0[1] as f32 * inv_alpha + color.0[1] as f32 * alpha) as u8;
                let nb = (current.0[2] as f32 * inv_alpha + color.0[2] as f32 * alpha) as u8;
                img.put_pixel(px as u32, py as u32, Rgba([nr, ng, nb, 255]));
            }
        }
    }
}

fn draw_rounded_rect_outline(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, r: i32, color: Rgba<u8>, border: i32) {
    for dx in 0..w {
        for dy in 0..h {
            let px = x + dx; let py = y + dy;
            if px < 0 || px >= img.width() as i32 || py < 0 || py >= img.height() as i32 { continue; }
            let dist_to_edge = |cx: i32, cy: i32| -> f32 { (((dx-cx).pow(2) + (dy-cy).pow(2)) as f32).sqrt() };
            let mut inside = false;
            let mut on_border = false;
            if dx >= r && dx < w-r { if dy < border || dy >= h-border { on_border = true; } inside = true; }
            else if dy >= r && dy < h-r { if dx < border || dx >= w-border { on_border = true; } inside = true; }
            else {
                let (cx, cy) = if dx < r && dy < r { (r, r) } else if dx >= w-r && dy < r { (w-r-1, r) } else if dx < r && dy >= h-r { (r, h-r-1) } else { (w-r-1, h-r-1) };
                let d = dist_to_edge(cx, cy);
                if d <= r as f32 { inside = true; if d > (r-border) as f32 { on_border = true; } }
            }
            if on_border { img.put_pixel(px as u32, py as u32, color); }
            else if inside { img.put_pixel(px as u32, py as u32, Rgba([30, 31, 34, 255])); }
        }
    }
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
                    if gv > 0.01 {
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

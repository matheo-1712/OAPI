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

/// Service for generating a MODERN and CLEAN Discord profile image summary
pub async fn generate_discord_profil(user: DiscordUser) -> ImageResponse {
    debug!("Generating MODERN profile image for user: {}", user.pseudo_discord);
    
    let output_dir = "public/generated_images";
    let _ = fs::create_dir_all(output_dir);

    let file_id = uuid::Uuid::new_v4().to_string();
    let file_name = format!("{}.png", file_id);
    let file_path = Path::new(output_dir).join(&file_name);

    // Image Setup: 900x500 (Clean, Square Borders)
    let width = 900;
    let height = 500;
    let mut img = RgbaImage::new(width, height);

    // Background: Discord Dark Mode feel
    let bg_color = Rgba([24, 25, 28, 255]);
    for pixel in img.pixels_mut() { *pixel = bg_color; }

    // Top Accent (Blurple)
    for x in 0..width {
        for y in 0..4 {
            img.put_pixel(x, y, Rgba([88, 101, 242, 255]));
        }
    }

    // Load fonts
    let font_data_bold = include_bytes!("C:\\Windows\\Fonts\\arialbd.ttf");
    let font_bold = Font::try_from_bytes(font_data_bold as &[u8]).expect("Error Bold Font");
    let font_data_reg = include_bytes!("C:\\Windows\\Fonts\\arial.ttf");
    let font_reg = Font::try_from_bytes(font_data_reg as &[u8]).expect("Error Font");

    // --- LOGIQUE D'AGRÉGATION ---
    let total_messages: i64 = user.stats.iter().map(|s| s.nb_message).sum();
    
    let stats_vocal_sum: f64 = user.stats.iter()
        .map(|s| s.vocal_time.parse::<f64>().unwrap_or(0.0))
        .sum();
    
    let total_vocal_decimal = stats_vocal_sum; // As per user's earlier instruction to only use stats array

    let mut text_counts = HashMap::new();
    let mut voice_counts = HashMap::new();
    for stat in &user.stats {
        for ch in &stat.text_channels { *text_counts.entry(ch.name.clone()).or_insert(0) += 1; }
        for ch in &stat.voice_channels { *voice_counts.entry(ch.name.clone()).or_insert(0) += 1; }
    }
    let top_text_channel = text_counts.into_iter().max_by_key(|&(_, count)| count).map(|(name, _)| name).unwrap_or_else(|| "Aucun".to_string());
    let top_voice_channel = voice_counts.into_iter().max_by_key(|&(_, count)| count).map(|(name, _)| name).unwrap_or_else(|| "Aucun".to_string());

    let filtered_roles: Vec<_> = user.roles.iter()
        .filter(|r| r.name.to_lowercase().contains("loutre") || r.name.to_lowercase().contains("rôle"))
        .take(4)
        .collect();

    let white = Rgba([255, 255, 255, 255]);
    let gray = Rgba([185, 187, 190, 255]);

    // --- RENDU ---

    // 1. Avatar (Circular)
    if let Some(avatar_url) = user.avatar_url {
        let client = reqwest::Client::new();
        if let Ok(resp) = client.get(avatar_url).send().await {
            if let Ok(bytes) = resp.bytes().await {
                if let Ok(avatar_img) = image::load_from_memory(&bytes) {
                    let mut avatar = avatar_img.resize_exact(160, 160, FilterType::Lanczos3).to_rgba8();
                    let radius = 80.0;
                    for ax in 0..160 {
                        for ay in 0..160 {
                            let dx = ax as f32 - 80.0;
                            let dy = ay as f32 - 80.0;
                            if dx*dx + dy*dy > radius*radius {
                                avatar.get_pixel_mut(ax, ay).0[3] = 0;
                            }
                        }
                    }
                    image::imageops::overlay(&mut img, &avatar, 50, 60);
                }
            }
        }
    }

    // 2. Identity
    draw_text_rgba(&mut img, &font_bold, &user.pseudo_discord, 240, 80, Scale::uniform(50.0), white);
    draw_text_rgba(&mut img, &font_reg, &format!("@{}", user.tag_discord), 240, 140, Scale::uniform(28.0), gray);

    // 3. Stats Section (Rounded Cards)
    let formatted_vocal = formatters::format_vocal_time(total_vocal_decimal);
    draw_stat_box(&mut img, 240, 200, "MESSAGES", &format!("{}", total_messages), &font_bold, &font_reg);
    draw_stat_box(&mut img, 460, 200, "TEMPS VOCAL", &formatted_vocal, &font_bold, &font_reg);
    draw_stat_box(&mut img, 680, 200, "RÔLES CLÉS", &format!("{}", filtered_roles.len()), &font_bold, &font_reg);

    // 4. Preferred Channels
    draw_text_rgba(&mut img, &font_reg, "SALON TEXTUEL PRÉFÉRÉ", 240, 320, Scale::uniform(16.0), gray);
    draw_text_rgba(&mut img, &font_bold, &formatters::truncate_text(&top_text_channel, 25), 240, 345, Scale::uniform(22.0), white);
    draw_text_rgba(&mut img, &font_reg, "SALON VOCAL PRÉFÉRÉ", 580, 320, Scale::uniform(16.0), gray);
    draw_text_rgba(&mut img, &font_bold, &formatters::truncate_text(&top_voice_channel, 25), 580, 345, Scale::uniform(22.0), white);

    // 5. Dates
    let formatted_join = formatters::format_discord_date(&user.join_date_discord);
    draw_text_rgba(&mut img, &font_reg, "MEMBRE DEPUIS LE", 50, 260, Scale::uniform(16.0), gray);
    draw_text_rgba(&mut img, &font_bold, &formatted_join, 50, 285, Scale::uniform(20.0), white);

    // 6. Roles (Rounded Pills with actual colors)
    let mut role_x = 50;
    for role in filtered_roles {
        let role_color = parse_hex_color(&role.color);
        let width = draw_role_pill_modern(&mut img, role_x, 400, &role.name, &font_reg, role_color);
        role_x += width + 12;
    }

    // Save final image
    let _ = img.save(&file_path);

    ImageResponse {
        url: format!("/generated_images/{}", file_name),
    }
}

fn draw_stat_box(img: &mut RgbaImage, x: i32, y: i32, label: &str, value: &str, font_bold: &Font, font_reg: &Font) {
    let white = Rgba([255, 255, 255, 255]);
    let gray = Rgba([185, 187, 190, 255]);
    let card_bg = Rgba([35, 36, 40, 255]);
    for bx in x..(x+200) {
        for by in y..(y+100) {
            if bx >= 0 && bx < img.width() as i32 && by >= 0 && by < img.height() as i32 {
                img.put_pixel(bx as u32, by as u32, card_bg);
            }
        }
    }
    draw_text_rgba(img, font_reg, label, x + 20, y + 20, Scale::uniform(18.0), gray);
    draw_text_rgba(img, font_bold, value, x + 20, y + 50, Scale::uniform(32.0), white);
}

fn draw_role_pill_modern(img: &mut RgbaImage, x: i32, y: i32, text: &str, font: &Font, color: Rgba<u8>) -> i32 {
    let white = Rgba([255, 255, 255, 255]);
    let inner_bg = Rgba([30, 31, 34, 255]);
    let text_len = text.len() as i32 * 11;
    let width = (text_len + 40).max(120);
    let height = 36;
    let radius = 18.0;
    let border_width = 2;

    for bx in 0..width {
        for by in 0..height {
            let px = x + bx;
            let py = y + by;
            if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
                let mut is_inside = false;
                if bx >= radius as i32 && bx < width - radius as i32 { is_inside = true; }
                else {
                    let cx = if bx < radius as i32 { radius } else { width as f32 - radius };
                    let dy = by as f32 - radius;
                    if (bx as f32 - cx).powi(2) + dy.powi(2) <= radius*radius { is_inside = true; }
                }

                if is_inside {
                    let mut is_border = bx < border_width || bx >= width - border_width || by < border_width || by >= height - border_width;
                    if !is_border && (bx < radius as i32 || bx >= width - radius as i32) {
                         let cx = if bx < radius as i32 { radius } else { width as f32 - radius };
                         let dist = ((bx as f32 - cx).powi(2) + (by as f32 - radius).powi(2)).sqrt();
                         if dist > radius - border_width as f32 { is_border = true; }
                    }
                    if is_border { img.put_pixel(px as u32, py as u32, color); }
                    else { img.put_pixel(px as u32, py as u32, inner_bg); }
                }
            }
        }
    }
    draw_text_rgba(img, font, text, x + 20, y + 8, Scale::uniform(16.0), white);
    width
}

fn draw_text_rgba(img: &mut RgbaImage, font: &Font, text: &str, x: i32, y: i32, scale: Scale, color: Rgba<u8>) {
    let v_metrics = font.v_metrics(scale);
    let offset = rusttype::point(x as f32, y as f32 + v_metrics.ascent);
    for glyph in font.layout(text, scale, offset) {
        if let Some(bb) = glyph.pixel_bounding_box() {
            glyph.draw(|gx, gy, gv| {
                let px = bb.min.x + gx as i32;
                let py = bb.min.y + gy as i32;
                if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
                    let pixel = img.get_pixel_mut(px as u32, py as u32);
                    if gv > 0.01 {
                        let alpha = gv;
                        let inv_alpha = 1.0 - alpha;
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

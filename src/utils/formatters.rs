use chrono::DateTime;

/// Formate une date ISO 8601 (ex: 2023-05-29T...) en format lisible JJ/MM/AAAA
pub fn format_discord_date(iso_date: &str) -> String {
    match DateTime::parse_from_rfc3339(iso_date) {
        Ok(dt) => dt.format("%d/%m/%Y").to_string(),
        Err(_) => iso_date.to_string(),
    }
}

/// Convertit un temps vocal décimal (heures) en format "Xh Ym"
pub fn format_vocal_time(decimal_hours: f64) -> String {
    let total_seconds = (decimal_hours * 3600.0).round() as i64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    
    if hours > 0 {
        format!("{}h {:02}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// Troncature simple d'un texte s'il dépasse une certaine longueur
pub fn truncate_text(text: &str, max_len: usize) -> String {
    if text.chars().count() > max_len {
        let truncated: String = text.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        text.to_string()
    }
}

use chrono::DateTime;

/// Formats an ISO 8601 date (e.g., 2023-05-29T...) into a readable DD/MM/YYYY format.
pub fn format_discord_date(iso_date: &str) -> String {
    match DateTime::parse_from_rfc3339(iso_date) {
        Ok(dt) => dt.format("%d/%m/%Y").to_string(),
        Err(_) => iso_date.to_string(),
    }
}

/// Converts decimal vocal time (hours) into "Xh Ym" format.
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

/// Simple text truncation if it exceeds a certain length.
pub fn truncate_text(text: &str, max_len: usize) -> String {
    if text.chars().count() > max_len {
        let truncated: String = text.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_discord_date() {
        assert_eq!(format_discord_date("2023-05-29T10:00:00Z"), "29/05/2023");
        assert_eq!(format_discord_date("invalid-date"), "invalid-date");
    }

    #[test]
    fn test_format_vocal_time() {
        assert_eq!(format_vocal_time(1.5), "1h 30m");
        assert_eq!(format_vocal_time(0.75), "45m");
        assert_eq!(format_vocal_time(10.01), "10h 00m");
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("Hello World", 20), "Hello World");
        assert_eq!(
            truncate_text("This is a very long string", 10),
            "This is..."
        );
        assert_eq!(truncate_text("Loutre", 6), "Loutre");
    }
}

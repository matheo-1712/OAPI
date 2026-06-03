use chrono::DateTime;

/// Formats an ISO 8601 date (e.g., 2023-05-29T...) into a readable DD/MM/YYYY format.
pub fn format_discord_date(iso_date: &str) -> String {
    match DateTime::parse_from_rfc3339(iso_date) {
        Ok(dt) => dt.format("%d/%m/%Y").to_string(),
        Err(_) => iso_date.to_string(),
    }
}

/// Converts decimal vocal time (hours) into "Xh Ymin Zs" format.
pub fn format_vocal_time(decimal_hours: f64) -> String {
    if decimal_hours <= 0.0 {
        return "0s".to_string();
    }

    let total_seconds = (decimal_hours * 3600.0).floor() as i64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    let mut parts = Vec::new();

    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}min", minutes));
    }
    if seconds > 0 && hours == 0 {
        parts.push(format!("{}s", seconds));
    }

    if parts.is_empty() {
        "0s".to_string()
    } else {
        parts.join(" ")
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

/// Formats a number with spaces as thousands separators.
pub fn format_number(n: i64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();
    for (i, c) in s.chars().enumerate() {
        result.push(c);
        let remaining = len - i - 1;
        if remaining > 0 && remaining.is_multiple_of(3) {
            result.push(' ');
        }
    }
    result
}

/// Formats Minecraft distance into "X blocs" with thousands separators.
pub fn format_minecraft_distance(blocks: f64) -> String {
    format!("{} blocs", format_number(blocks.round() as i64))
}

/// Formats Minecraft playtime (seconds) into a readable duration.
pub fn format_minecraft_playtime(seconds: i64) -> String {
    if seconds <= 0 {
        return "0s".to_string();
    }

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    let mut parts = Vec::new();

    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}min", minutes));
    }
    if secs > 0 && hours == 0 {
        parts.push(format!("{}s", secs));
    }

    if parts.is_empty() {
        "0s".to_string()
    } else {
        parts.join(" ")
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
        assert_eq!(format_vocal_time(1.5), "1h 30min");
        assert_eq!(format_vocal_time(0.75), "45min");
        assert_eq!(format_vocal_time(0.01), "36s");
        assert_eq!(format_vocal_time(10.01), "10h");
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

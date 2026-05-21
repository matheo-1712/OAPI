// ──────────────────────────────────────────────
//  System Paths
// ──────────────────────────────────────────────
pub const DISCORD_SUMMARY_DIR: &str = "public/generated_images/discord_summary";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_defined() {
        assert!(!DISCORD_SUMMARY_DIR.is_empty());
    }
}


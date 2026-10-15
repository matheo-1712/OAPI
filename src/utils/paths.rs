// ──────────────────────────────────────────────
//  System Paths
// ──────────────────────────────────────────────
pub const DISCORD_SUMMARY_DIR: &str = "generated_images/discord_summary";
pub const MINECRAFT_SUMMARY_DIR: &str = "generated_images/minecraft_summary";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_defined() {
        assert_eq!(
            DISCORD_SUMMARY_DIR,
            "generated_images/discord_summary"
        );
    }
}

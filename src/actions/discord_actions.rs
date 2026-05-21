//! Actions related to Discord data orchestration.
//!
//! This module handles the fetching of Discord user data and statistics
//! and coordinates the generation of profile images.

use crate::models::{DiscordUser, ImageResponse};
use crate::services;
use crate::utils::api_endpoints::{discord_stats_url, discord_user_url};
use crate::utils::api_fetch::fetch_api_data;
use tracing::debug;

/// Orchestrates the process of fetching Discord data and generating a summary image.
///
/// This is the main "Use Case" for Discord profile generation.
///
/// # Arguments
///
/// * `id` - The unique internal database ID of the user.
///
/// # Errors
///
/// Returns an error if any part of the data fetching or image generation fails.
pub async fn get_discord_summary_action(id: &str) -> Result<ImageResponse, String> {
    // 1. Fetch data (Logic moved from handler)
    let user = fetch_discord_data(id).await?;

    // 2. We could here "structure" or "filter" what we want to keep
    // for the final response if we were returning the object itself.
    // For now, we only return the image URL via the service.

    // 3. Orchestrate business logic (Image generation)
    let response = services::generate_discord_profile(user).await;

    Ok(response)
}

/// Private helper to aggregate Discord user info and stats from multiple endpoints.
async fn fetch_discord_data(id: &str) -> Result<DiscordUser, String> {
    debug!("Action: Fetching external data for id: {}", id);

    // Fetch user info (ID is appended to the configured full URL)
    let user_url = format!("{}/{}", discord_user_url(), id);
    let mut user: DiscordUser = fetch_api_data(&user_url, "user info").await?;

    // Fetch user stats (ID is appended to the configured full URL)
    let stats_url = format!("{}/{}", discord_stats_url(), id);
    user.stats = fetch_api_data(&stats_url, "stats").await?;

    Ok(user)
}

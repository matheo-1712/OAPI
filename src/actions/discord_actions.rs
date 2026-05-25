//! Actions related to Discord data orchestration.
//!
//! This module handles the fetching of Discord user data and statistics
//! and coordinates the generation of profile images.

use crate::models::{DiscordUser, ImageResponse};
use crate::services;
use crate::utils::constants::{DISCORD_USERS_COLLECTION, DISCORD_USER_STATS_COLLECTION};
use crate::utils::pocketbase::PocketbaseClient;
use tracing::debug;

/// Orchestrates the process of fetching Discord data and generating a summary image.
pub async fn get_discord_summary_action(id: &str) -> Result<ImageResponse, String> {
    let user = fetch_discord_data(id).await?;
    let response = services::generate_discord_profile(user).await;
    Ok(response)
}

/// Private helper to aggregate Discord user info and stats from PocketBase.
async fn fetch_discord_data(id: &str) -> Result<DiscordUser, String> {
    debug!("Action: Fetching data from PocketBase for id: {}", id);

    let mut pb = PocketbaseClient::new();
    pb.login().await?;

    // 1. Fetch user info (using PocketBase internal ID)
    let mut user: DiscordUser = pb.get_record(DISCORD_USERS_COLLECTION, id, ()).await?;

    // 2. Fetch user stats (filtering by the 'discord_user' field)
    // Based on the model, we filter stats using the discord_id (Snowflake)
    let filter = format!("discord_user = '{}'", user.discord_id);
    user.stats = pb
        .list_records(DISCORD_USER_STATS_COLLECTION, &filter)
        .await?;

    Ok(user)
}

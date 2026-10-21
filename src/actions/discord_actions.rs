//! Actions related to Discord data orchestration.
//!
//! This module handles the fetching of Discord user data and statistics
//! and coordinates the generation of profile images.

use crate::models::{DiscordBadge, DiscordStats, DiscordUser, ImageResponse};
use crate::services;
use crate::utils::constants::{
    DISCORD_USER_BADGES_COLLECTION, DISCORD_USER_STATS_COLLECTION, DISCORD_USERS_COLLECTION,
};
use crate::utils::pocketbase::PocketbaseClient;
use tracing::{debug, error, warn};

/// Orchestrates the process of fetching Discord data and generating a summary image.
pub async fn get_discord_summary_action(id: &str) -> Result<ImageResponse, String> {
    let snapshot = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        super::discord_activity::snapshot(Some(id)),
    )
    .await
    .map_err(|_| "Statistics timeout")??;
    let user = snapshot.user.clone().ok_or("Missing Discord profile")?;
    services::generate_discord_profile(user, &snapshot.activity).await
}

/// Private helper to aggregate Discord user info and stats from PocketBase.
pub(super) async fn fetch_discord_data(discord_id: &str) -> Result<DiscordUser, String> {
    debug!(
        "Action: Fetching Discord data from PocketBase for discord_id: {}",
        discord_id
    );

    let mut pb = PocketbaseClient::new();
    pb.login().await?;

    // 1. Fetch user info by Discord ID
    let user_filter = format!("discord_id = '{}'", discord_id);
    let users: Vec<DiscordUser> = pb
        .list_bounded_records(DISCORD_USERS_COLLECTION, &user_filter)
        .await?;
    let mut user = users
        .into_iter()
        .next()
        .ok_or_else(|| format!("User not found with discord_id: {}", discord_id))?;

    if user.discord_id != discord_id
        || user.id.is_empty()
        || !user.id.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err("Invalid Discord account relation".into());
    }

    // 2. Fetch user stats
    let filter = format!(
        "discord_user = '{}' || discord_user = '{}'",
        user.id, user.discord_id
    );

    // 3. Fetch Badges (the 3 latest)
    debug!(
        "Fetching badges from {} with filter: {}",
        DISCORD_USER_BADGES_COLLECTION, filter
    );
    let mut badge_params = std::collections::HashMap::new();
    badge_params.insert("filter", filter.clone());
    badge_params.insert("expand", "badge".to_string());
    badge_params.insert("sort", "-created".to_string());
    badge_params.insert("perPage", "3".to_string());

    let raw_badges: Vec<serde_json::Value> = pb
        .list_records_with_params(DISCORD_USER_BADGES_COLLECTION, badge_params)
        .await
        .map(|resp| resp.items)
        .unwrap_or_else(|e| {
            error!("Failed to list badge records: {}", e);
            Vec::new()
        });

    debug!("Raw badges count from PocketBase: {}", raw_badges.len());

    let mut processed_badges = Vec::new();
    for mut val in raw_badges {
        if let Some(obj) = val.as_object_mut() {
            // Flatten expand.badge to badge_info to match the model
            if let Some(badge_info) = obj.get("expand").and_then(|e| e.get("badge")).cloned() {
                obj.insert("badge_info".to_string(), badge_info);
            } else {
                warn!("Badge record missing 'expand.badge': {:?}", obj.get("id"));
            }
        }
        match serde_json::from_value::<DiscordBadge>(val.clone()) {
            Ok(badge) => {
                debug!(
                    "Successfully processed badge: {} ({})",
                    badge.badge_info.name, badge.badge_info.id
                );
                processed_badges.push(badge);
            }
            Err(e) => {
                error!("Failed to deserialize badge: {}", e);
            }
        }
    }

    debug!("Final processed badges count: {}", processed_badges.len());

    // 4. Fetch bounded history across all pages.
    let raw_stats: Vec<serde_json::Value> = pb
        .list_bounded_records(DISCORD_USER_STATS_COLLECTION, &filter)
        .await?;

    let processed_stats = super::discord_activity::parse_stats(raw_stats)?;

    user.stats = processed_stats;

    // Attach the 3 latest badges to the first stat entry if available
    if !processed_badges.is_empty() {
        if let Some(first_stat) = user.stats.get_mut(0) {
            first_stat.badges = Some(processed_badges);
        } else {
            // If no stats exist, create a dummy stat entry to hold the badges
            let dummy_stat = DiscordStats {
                id: "dummy".to_string(),
                discord_user: user.id.clone(),
                message_count: 0,
                vocal_time: "0".to_string(),
                date_stats: "".to_string(),
                voice_channels: None,
                text_channels: None,
                vocal_with: None,
                badges: Some(processed_badges),
            };
            user.stats.push(dummy_stat);
        }
    }

    Ok(user)
}

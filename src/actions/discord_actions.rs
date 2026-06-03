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
    let user = fetch_discord_data(id).await?;
    let response = services::generate_discord_profile(user).await;
    Ok(response)
}

/// Private helper to aggregate Discord user info and stats from PocketBase.
async fn fetch_discord_data(id: &str) -> Result<DiscordUser, String> {
    debug!("Action: Fetching data from PocketBase for id: {}", id);

    let mut pb = PocketbaseClient::new();
    pb.login().await?;

    // 1. Fetch user info
    let mut user: DiscordUser = pb.get_record(DISCORD_USERS_COLLECTION, id, ()).await?;

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
                error!("Failed to deserialize badge: {}. Raw value: {:?}", e, val);
            }
        }
    }

    debug!("Final processed badges count: {}", processed_badges.len());

    // 4. Fetch user stats (using list_all_records to avoid 100-item limit)
    let raw_stats: Vec<serde_json::Value> = pb
        .list_all_records(DISCORD_USER_STATS_COLLECTION, &filter)
        .await?;

    let mut processed_stats = Vec::new();
    for mut val in raw_stats {
        if let Some(obj) = val.as_object_mut() {
            // PocketBase peut renvoyer un nombre pour vocal_time,
            // on le convertit en String pour correspondre au modèle
            if let Some(vocal_val) = obj.get("vocal_time").filter(|v| v.is_number()) {
                let as_string = vocal_val.to_string();
                obj.insert("vocal_time".to_string(), serde_json::json!(as_string));
            }
        }

        // Désérialisation finale vers le modèle de l'utilisateur
        match serde_json::from_value::<DiscordStats>(val) {
            Ok(stat) => processed_stats.push(stat),
            Err(e) => {
                error!("Failed to parse stat record: {}", e);
                return Err(format!("Stat parsing error: {}", e));
            }
        }
    }

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

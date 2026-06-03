//! Actions related to Minecraft data orchestration.

use crate::models::{ImageResponse, MinecraftPlayer, MinecraftStats, PlayerBadge, Server};
use crate::services;
use crate::utils::constants::{
    DISCORD_USER_BADGES_COLLECTION, MINECRAFT_PLAYERS_COLLECTION, MINECRAFT_SERVERS_COLLECTION,
    MINECRAFT_STATS_COLLECTION,
};
use crate::utils::pocketbase::PocketbaseClient;
use std::collections::HashMap;
use tracing::{debug, error, warn};

/// Orchestrates the process of fetching Minecraft data and generating a summary image.
pub async fn get_minecraft_summary_action(id: &str) -> Result<ImageResponse, String> {
    let player = fetch_minecraft_data(id).await?;
    let response = services::generate_minecraft_profile(player).await;
    Ok(response)
}

/// Private helper to aggregate Minecraft player info and stats from PocketBase.
async fn fetch_minecraft_data(uuid: &str) -> Result<MinecraftPlayer, String> {
    debug!(
        "Action: Fetching Minecraft data from PocketBase for UUID: {}",
        uuid
    );

    let mut pb = PocketbaseClient::new();
    pb.login().await?;

    // 1. Fetch player info by Minecraft UUID
    let filter_user = format!("account_id = '{}'", uuid);
    let players: Vec<MinecraftPlayer> = pb
        .list_all_records(MINECRAFT_PLAYERS_COLLECTION, &filter_user)
        .await?;
    let mut player = players
        .into_iter()
        .next()
        .ok_or_else(|| format!("Player not found with UUID: {}", uuid))?;

    // 2. Fetch player stats
    let filter_stats = format!(
        "account_id = '{}' || account_id = '{}'",
        player.id, player.account_id
    );
    let stats: Vec<MinecraftStats> = pb
        .list_all_records(MINECRAFT_STATS_COLLECTION, &filter_stats)
        .await?;

    // 3. Fetch Badges (the 3 latest)
    let badge_filter = format!(
        "player = '{}' || player = '{}'",
        player.id, player.account_id
    );
    debug!(
        "Fetching Minecraft badges from {} with filter: {}",
        DISCORD_USER_BADGES_COLLECTION, badge_filter
    );
    let mut badge_params = std::collections::HashMap::new();
    badge_params.insert("filter", badge_filter.clone());
    badge_params.insert("expand", "badge".to_string());
    badge_params.insert("sort", "-created".to_string());
    badge_params.insert("perPage", "3".to_string());

    let raw_badges: Vec<serde_json::Value> = pb
        .list_records_with_params(DISCORD_USER_BADGES_COLLECTION, badge_params)
        .await
        .map(|resp| resp.items)
        .unwrap_or_else(|e| {
            error!("Failed to list badge records for Minecraft: {}", e);
            Vec::new()
        });

    let mut processed_badges = Vec::new();
    for mut val in raw_badges {
        if let Some(obj) = val.as_object_mut() {
            if let Some(badge_info) = obj.get("expand").and_then(|e| e.get("badge")).cloned() {
                obj.insert("badge_info".to_string(), badge_info);
            } else {
                warn!(
                    "Badge record missing 'expand.badge' for Minecraft: {:?}",
                    obj.get("id")
                );
            }
        }
        match serde_json::from_value::<PlayerBadge>(val.clone()) {
            Ok(badge) => processed_badges.push(badge),
            Err(e) => {
                error!(
                    "Failed to deserialize Minecraft badge: {}. Raw value: {:?}",
                    e, val
                );
            }
        }
    }

    // 4. Fetch all servers to resolve names and colors
    let servers: Vec<Server> = pb
        .list_all_records(MINECRAFT_SERVERS_COLLECTION, "")
        .await?;
    let mut server_names = HashMap::new();
    let mut server_colors = HashMap::new();
    for s in servers {
        server_names.insert(s.id.clone(), s.name);
        if let Some(color) = s.embed_color {
            server_colors.insert(s.id, color);
        }
    }

    player.stats = stats;
    player.server_names = server_names;
    player.server_colors = server_colors;
    player.badges = if processed_badges.is_empty() {
        None
    } else {
        Some(processed_badges)
    };

    Ok(player)
}

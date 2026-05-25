//! Actions related to Discord data orchestration.
//!
//! This module handles the fetching of Discord user data and statistics
//! and coordinates the generation of profile images.

use crate::models::{DiscordStats, DiscordUser, ImageResponse};
use crate::services;
use crate::utils::constants::{DISCORD_USERS_COLLECTION, DISCORD_USER_STATS_COLLECTION};
use crate::utils::pocketbase::PocketbaseClient;
use tracing::{debug, error};

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
    let filter = format!("discord_user = '{}' || discord_user = '{}'", user.id, user.discord_id);
    
    // On récupère les données en JSON brut pour gérer les types de manière flexible
    let raw_stats: Vec<serde_json::Value> = pb
        .list_records(DISCORD_USER_STATS_COLLECTION, &filter)
        .await?;

    let mut processed_stats = Vec::new();
    for mut val in raw_stats {
        if let Some(obj) = val.as_object_mut() {
            // PocketBase peut renvoyer un nombre pour vocal_time, 
            // on le convertit en String pour correspondre au modèle
            if let Some(vocal_val) = obj.get("vocal_time") {
                if vocal_val.is_number() {
                    let as_string = vocal_val.to_string();
                    obj.insert("vocal_time".to_string(), serde_json::json!(as_string));
                }
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
    Ok(user)
}

use crate::models::{DiscordUser, ImageResponse};
use crate::services;
use tracing::{debug, error};
use serde::Deserialize;

#[derive(Deserialize)]
struct UserWrapper {
    data: DiscordUser,
}

#[derive(Deserialize)]
struct StatsWrapper {
    data: Vec<crate::models::DiscordStats>,
}

/// Action to fetch data and generate a discord summary
pub async fn get_discord_summary_action(discord_id: &str) -> Result<ImageResponse, String> {
    // 1. Fetch data (Logic moved from handler)
    let user = fetch_discord_data(discord_id).await?;

    // 2. We could here "structure" or "filter" what we want to keep 
    // for the final response if we were returning the object itself.
    // For now, we only return the image URL via the service.
    
    // 3. Orchestrate business logic
    let response = services::generate_discord_profile(user).await;
    
    Ok(response)
}

/// Helper function (Private to the action layer)
async fn fetch_discord_data(discord_id: &str) -> Result<DiscordUser, String> {
    debug!("Action: Fetching external data for discord_id: {}", discord_id);
    let client = reqwest::Client::new();
    
    // Fetch user info
    let user_url = format!("https://otterlyapi.antredesloutres.fr/api/utilisateurs_discord/{}", discord_id);
    let user_resp = client.get(&user_url).send().await
        .map_err(|e| format!("Failed to fetch user info: {}", e))?;

    if !user_resp.status().is_success() {
        return Err(format!("External API returned error {} for user info", user_resp.status()));
    }

    let text = user_resp.text().await
        .map_err(|e| format!("Failed to get user info text: {}", e))?;
    
    debug!("Raw user info response: {}", text);

    let wrapper: UserWrapper = serde_json::from_str(&text)
        .map_err(|e| {
            error!("Failed to parse user info JSON: {}. Raw body: {}", e, text);
            format!("Failed to parse user info: {}", e)
        })?;
    let mut user = wrapper.data;
        
    // Fetch user stats
    let stats_url = format!("https://otterlyapi.antredesloutres.fr/api/utilisateurs_discord/stats/{}", discord_id);
    let stats_resp = client.get(&stats_url).send().await
        .map_err(|e| format!("Failed to fetch stats: {}", e))?;

    if !stats_resp.status().is_success() {
        return Err(format!("External API returned error {} for stats", stats_resp.status()));
    }

    let stats_text = stats_resp.text().await
        .map_err(|e| format!("Failed to get stats text: {}", e))?;
        
    debug!("Raw stats response: {}", stats_text);

    let stats_wrapper: StatsWrapper = serde_json::from_str(&stats_text)
        .map_err(|e| {
            error!("Failed to parse stats JSON: {}. Raw body: {}", e, stats_text);
            format!("Failed to parse stats: {}", e)
        })?;
        
    user.stats = stats_wrapper.data;
    Ok(user)
}

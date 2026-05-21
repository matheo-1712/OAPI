use crate::models::{DiscordUser, ImageResponse};
use crate::services;
use crate::utils::api_fetch::fetch_api_data;
use crate::utils::api_endpoints::{discord_stats_url, discord_user_url};
use tracing::debug;

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
    
    // Fetch user info
    let user_url = format!("{}/{}", discord_user_url(), discord_id);
    let mut user: DiscordUser = fetch_api_data(&user_url, "user info").await?;
        
    // Fetch user stats
    let stats_url = format!("{}/{}", discord_stats_url(), discord_id);
    user.stats = fetch_api_data(&stats_url, "stats").await?;

    Ok(user)
}

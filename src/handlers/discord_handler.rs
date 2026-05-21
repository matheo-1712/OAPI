//! HTTP request handlers for Discord-related endpoints.
//! 
//! This module acts as the entry point for HTTP requests, extracting parameters 
//! and delegating the business logic to the action layer.

use axum::{Json, extract::Path, http::StatusCode};
use crate::models::ImageResponse;
use crate::actions::discord_actions;
use tracing::{info, error};

/// Generates a Discord profile image summary for a specific user ID.
/// 
/// This handler extracts the `discord_id` from the URL path and invokes 
/// the corresponding action to fetch data and generate the image.
#[utoipa::path(
    post,
    path = "/api/discord-summary/{discord_id}",
    responses(
        (status = 200, description = "Discord summary image generated successfully", body = ImageResponse),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("discord_id" = String, Path, description = "Discord User ID")
    )
)]
pub async fn create_discord_summary_by_id(Path(discord_id): Path<String>) -> Result<Json<ImageResponse>, (StatusCode, String)> {
    info!("Handler: Requesting discord summary via Action for ID: {}", discord_id);
    
    // Delegation to the Action layer
    match discord_actions::get_discord_summary_action(&discord_id).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Action failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e))
        }
    }
}

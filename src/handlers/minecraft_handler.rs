//! HTTP request handlers for Minecraft-related endpoints.

use crate::actions::minecraft_actions;
use crate::models::ImageResponse;
use axum::{Json, extract::Path, http::StatusCode};
use tracing::{error, info};

/// Generates a Minecraft summary image for a specific player.
#[utoipa::path(
    post,
    path = "/minecraft-summary/{id}",
    responses(
        (status = 200, description = "Minecraft summary image generated successfully", body = ImageResponse),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = String, Path, description = "Unique internal database ID of the player")
    ),
    tag = "Minecraft"
)]
pub async fn create_minecraft_summary_by_id(
    Path(id): Path<String>,
) -> Result<Json<ImageResponse>, (StatusCode, String)> {
    info!(
        "Handler: Requesting minecraft summary via Action for ID: {}",
        id
    );

    // Delegation to the Action layer
    match minecraft_actions::get_minecraft_summary_action(&id).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Action failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e))
        }
    }
}

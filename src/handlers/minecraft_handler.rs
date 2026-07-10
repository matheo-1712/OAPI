//! HTTP request handlers for Minecraft-related endpoints.

use crate::actions::minecraft_actions;
use crate::models::ImageResponse;
use axum::{Json, extract::Path, http::StatusCode};
use tracing::{error, info};

/// Generates a Minecraft summary image for a specific player.
#[utoipa::path(
    post,
    path = "/minecraft-summary/{uuid}",
    responses(
        (status = 200, description = "Minecraft summary image generated successfully", body = ImageResponse),
        (status = 400, description = "Invalid UUID or not a Minecraft player"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("uuid" = String, Path, description = "Minecraft UUID of the player")
    ),
    tag = "Minecraft"
)]
pub async fn create_minecraft_summary_by_id(
    Path(uuid): Path<String>,
) -> Result<Json<ImageResponse>, (StatusCode, String)> {
    info!(
        "Handler: Requesting minecraft summary via Action for UUID: {}",
        uuid
    );

    let clean_uuid = uuid.replace("-", "");
    if clean_uuid.len() != 32 || !clean_uuid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid Minecraft UUID format".to_string(),
        ));
    }

    // Delegation to the Action layer (which verifies existence in PocketBase)
    match minecraft_actions::get_minecraft_summary_action(&uuid).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Action failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e))
        }
    }
}

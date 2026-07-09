//! HTTP request handlers for Discord-related endpoints.
//!
//! This module acts as the entry point for HTTP requests, extracting parameters
//! and delegating the business logic to the action layer.

use crate::actions::discord_actions;
use crate::models::ImageResponse;
use axum::{Extension, Json, extract::Path, http::StatusCode};
use tracing::{error, info};
use crate::models::{JwtClaims, Role};

/// Generates a Discord summary image for a specific user.
///
/// This endpoint fetches user data and statistics from external APIs and
/// generates a high-fidelity profile summary card as a PNG image.
#[utoipa::path(
    post,
    path = "/discord-summary/{id}",
    responses(
        (status = 200, description = "Discord summary image generated successfully", body = ImageResponse),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("id" = String, Path, description = "Discord Snowflake ID of the user")
    ),
    tag = "Discord"
)]
pub async fn create_discord_summary_by_id(
    Path(id): Path<String>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<ImageResponse>, (StatusCode, String)> {
    if claims.sub != id && claims.role != Role::Admin {
        error!("User {} (ID: {}) attempted to generate Discord summary for another user (ID: {})", claims.username, claims.sub, id);
        return Err((
            StatusCode::FORBIDDEN,
            "Vous ne pouvez générer l'image que pour votre propre compte Discord.".to_string(),
        ));
    }

    info!(
        "Handler: Requesting discord summary via Action for ID: {}",
        id
    );

    // Delegation to the Action layer
    match discord_actions::get_discord_summary_action(&id).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Action failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e))
        }
    }
}

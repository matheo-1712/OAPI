//! HTTP request handlers for Minecraft-related endpoints.

use crate::actions::minecraft_actions;
use crate::models::ImageResponse;
use crate::models::JwtClaims;
use axum::{Extension, Json, extract::Path, http::StatusCode};
use tracing::{error, info};

/// Generates a Minecraft summary image for a specific player.
#[utoipa::path(
    post,
    path = "/minecraft-summary/{uuid}",
    responses(
        (status = 200, description = "Minecraft summary image generated successfully", body = ImageResponse),
        (status = 400, description = "Invalid UUID or not a Minecraft player"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Player belongs to another account"),
        (status = 503, description = "Account verification unavailable"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("uuid" = String, Path, description = "Minecraft UUID of the player")
    ),
    tag = "Minecraft"
)]
pub async fn create_minecraft_summary_by_id(
    Path(uuid): Path<String>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<ImageResponse>, (StatusCode, String)> {
    info!(
        "Handler: Requesting minecraft summary via Action for UUID: {}",
        uuid
    );

    if !crate::services::image_service::valid_uuid(&uuid) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid Minecraft UUID format".to_string(),
        ));
    }

    match crate::actions::image_access::can_access_minecraft(&uuid, &claims).await {
        Ok(true) => {}
        Ok(false) => {
            return Err((
                StatusCode::FORBIDDEN,
                "Ce joueur ne vous appartient pas.".into(),
            ));
        }
        Err(error) => {
            error!("Unable to verify Minecraft ownership: {}", error);
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "Vérification du compte indisponible.".into(),
            ));
        }
    }
    // Delegation to the Action layer (which verifies existence in PocketBase)
    match minecraft_actions::get_minecraft_summary_action(&uuid).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Action failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Impossible de générer l’image.".into(),
            ))
        }
    }
}

//! HTTP request handlers for Discord-related endpoints.
//!
//! This module acts as the entry point for HTTP requests, extracting parameters
//! and delegating the business logic to the action layer.

use crate::actions::discord_actions;
use crate::models::ImageResponse;
use crate::models::{JwtClaims, Role};
use axum::{Extension, Json, extract::Path, http::StatusCode};
use tracing::{error, info};

/// Generate the connected user's Discord image without supplying an ID.
#[utoipa::path(
    post,
    path = "/discord-summary/me",
    tag = "Discord",
    responses(
        (status = 200, description = "Personal image generated; URL requires the owner's or administrator's session", body = ImageResponse),
        (status = 400, description = "Invalid session Discord ID"),
        (status = 401, description = "Authentication required"),
        (status = 429, description = "Route quota exceeded (10 requests per minute); Retry-After in seconds"),
        (status = 500, description = "Image generation unavailable")
    )
)]
pub async fn create_my_discord_summary(
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<ImageResponse>, (StatusCode, String)> {
    create_discord_summary_by_id(Path(claims.sub.clone()), Extension(claims)).await
}

/// Generates a Discord summary image for a specific user.
///
/// This endpoint fetches user data and statistics from external APIs and
/// generates a high-fidelity profile summary card as a PNG image.
#[utoipa::path(
    post,
    path = "/discord-summary/{id}",
    responses(
        (status = 200, description = "Discord summary image generated successfully", body = ImageResponse),
        (status = 400, description = "Invalid Discord ID"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "User belongs to another account"),
        (status = 429, description = "Route quota exceeded (10 requests per minute); Retry-After in seconds"),
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
    if !crate::services::image_service::valid_discord_id(&id) {
        return Err((StatusCode::BAD_REQUEST, "Invalid Discord ID".into()));
    }
    if claims.sub != id && claims.role != Role::Admin {
        error!(
            "User {} (ID: {}) attempted to generate Discord summary for another user (ID: {})",
            claims.username, claims.sub, id
        );
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
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Impossible de générer l’image.".into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn personal_image_requires_a_valid_session() {
        crate::config::init();
        let path = crate::config::Config::global()
            .server
            .routes
            .discord_summary
            .replace(":id", "me");
        for cookie in ["", "auth_token=forged"] {
            let response = crate::routes::api_routes()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(&path)
                        .header("cookie", cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(response.headers()["cache-control"], "no-store");
        }
    }
}

use crate::models::{JwtClaims, discord_activity::DiscordActivity};
use axum::{Extension, Json, http::StatusCode};

async fn respond(id: Option<&str>) -> Result<Json<DiscordActivity>, (StatusCode, String)> {
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        crate::actions::discord_activity::snapshot(id),
    )
    .await
    .unwrap_or_else(|_| Err("Statistics timeout".into()))
    .map(|snapshot| Json(snapshot.activity.clone()))
    .map_err(|error| {
        tracing::error!("Discord statistics unavailable: {error}");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Statistiques temporairement indisponibles.".into(),
        )
    })
}

#[utoipa::path(get, path = "/discord/stats/me", tag = "Discord", responses(
    (status = 200, body = DiscordActivity, description = "All-time personal activity; auth_token session cookie required"),
    (status = 401, description = "Authentication required"),
    (status = 429, description = "Route quota exceeded (10 requests per minute); Retry-After in seconds"),
    (status = 503, description = "Statistics unavailable")
))]
pub async fn personal(
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<DiscordActivity>, (StatusCode, String)> {
    respond(Some(&claims.sub)).await
}

#[utoipa::path(get, path = "/admin/discord/stats", tag = "Discord", responses(
    (status = 200, body = DiscordActivity, description = "All-time global activity; Admin auth_token session required"),
    (status = 401, description = "Authentication required"),
    (status = 403, description = "Admin required"),
    (status = 429, description = "Route quota exceeded (10 requests per minute); Retry-After in seconds"),
    (status = 503, description = "Statistics unavailable")
))]
pub async fn global() -> Result<Json<DiscordActivity>, (StatusCode, String)> {
    respond(None).await
}

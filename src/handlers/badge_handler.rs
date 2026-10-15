use crate::{
    services::badge_service::{self, AwardRequest, AwardResult},
    utils::pocketbase::PocketbaseClient,
};
use axum::{Extension, Json, extract::Path, http::StatusCode};

#[utoipa::path(post, path = "/badges/me/refresh", tag = "badges",
    responses((status = 200, body = badge_service::RefreshResponse), (status = 401, description = "Authentication required"), (status = 502, description = "Refresh unavailable")))]
pub async fn refresh_mine(
    Extension(claims): Extension<crate::models::JwtClaims>,
) -> Result<Json<badge_service::RefreshResponse>, (StatusCode, String)> {
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(failure)?;
    badge_service::refresh_mine(&pb, &claims.sub)
        .await
        .map(Json)
        .map_err(failure)
}

async fn refresh_account(
    recipient: String,
    kind: badge_service::RecipientKind,
    claims: crate::models::JwtClaims,
) -> Result<Json<Vec<AwardResult>>, (StatusCode, String)> {
    if !badge_service::valid_id(&recipient) {
        return Err((StatusCode::BAD_REQUEST, "Invalid record ID".into()));
    }
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(failure)?;
    if !badge_service::can_refresh(&pb, &recipient, kind, &claims)
        .await
        .map_err(failure)?
    {
        return Err((
            StatusCode::FORBIDDEN,
            "Ce compte ne vous appartient pas.".into(),
        ));
    }
    let results = match kind {
        badge_service::RecipientKind::Player => badge_service::evaluate(&pb, &recipient).await,
        badge_service::RecipientKind::DiscordUser => {
            badge_service::evaluate_discord(&pb, &recipient).await
        }
    };
    results.map(Json).map_err(failure)
}

#[utoipa::path(post, path = "/badges/players/{player}/refresh", tag = "badges",
    params(("player" = String, Path, description = "PocketBase player record ID")),
    responses((status = 200, body = [AwardResult]), (status = 400, description = "Invalid ID"), (status = 401, description = "Authentication required"), (status = 403, description = "Account belongs to another user"), (status = 502, description = "Refresh failed")))]
pub async fn refresh_player(
    Path(player): Path<String>,
    Extension(claims): Extension<crate::models::JwtClaims>,
) -> Result<Json<Vec<AwardResult>>, (StatusCode, String)> {
    refresh_account(player, badge_service::RecipientKind::Player, claims).await
}

#[utoipa::path(post, path = "/badges/discord/{user}/refresh", tag = "badges",
    params(("user" = String, Path, description = "PocketBase Discord user record ID")),
    responses((status = 200, body = [AwardResult]), (status = 400, description = "Invalid ID"), (status = 401, description = "Authentication required"), (status = 403, description = "Account belongs to another user"), (status = 502, description = "Refresh failed")))]
pub async fn refresh_discord(
    Path(user): Path<String>,
    Extension(claims): Extension<crate::models::JwtClaims>,
) -> Result<Json<Vec<AwardResult>>, (StatusCode, String)> {
    refresh_account(user, badge_service::RecipientKind::DiscordUser, claims).await
}

fn failure(error: String) -> (StatusCode, String) {
    tracing::error!("Badge attribution failed: {}", error);
    (
        StatusCode::BAD_GATEWAY,
        "Badge attribution failed; check API logs and configuration".into(),
    )
}

#[utoipa::path(post, path = "/admin/badges/discord/{user}/evaluate", tag = "badges", params(("user" = String, Path, description = "PocketBase Discord user record ID")),
    responses((status = 200, body = [AwardResult]), (status = 400, description = "Invalid ID"), (status = 401, description = "Authentication required"), (status = 403, description = "Admin required"), (status = 502, description = "Evaluation failed")))]
pub async fn evaluate_discord(
    Path(user): Path<String>,
) -> Result<Json<Vec<AwardResult>>, (StatusCode, String)> {
    if !badge_service::valid_id(&user) {
        return Err((StatusCode::BAD_REQUEST, "Invalid record ID".into()));
    }
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(failure)?;
    badge_service::evaluate_discord(&pb, &user)
        .await
        .map(Json)
        .map_err(failure)
}

#[utoipa::path(post, path = "/admin/badges/award", tag = "badges", request_body = AwardRequest,
    responses((status = 200, body = AwardResult), (status = 400, description = "Invalid ID"), (status = 401, description = "Authentication required"), (status = 403, description = "Admin required"), (status = 502, description = "Attribution failed")))]
pub async fn award(
    Json(request): Json<AwardRequest>,
) -> Result<Json<AwardResult>, (StatusCode, String)> {
    if !badge_service::valid_id(&request.recipient) || !badge_service::valid_id(&request.badge) {
        return Err((StatusCode::BAD_REQUEST, "Invalid record ID".into()));
    }
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(failure)?;
    badge_service::award(&pb, request)
        .await
        .map(Json)
        .map_err(failure)
}

#[utoipa::path(post, path = "/admin/badges/players/{player}/evaluate", tag = "badges", params(("player" = String, Path, description = "PocketBase player record ID")),
    responses((status = 200, body = [AwardResult]), (status = 400, description = "Invalid ID"), (status = 401, description = "Authentication required"), (status = 403, description = "Admin required"), (status = 502, description = "Evaluation failed")))]
pub async fn evaluate(
    Path(player): Path<String>,
) -> Result<Json<Vec<AwardResult>>, (StatusCode, String)> {
    if !badge_service::valid_id(&player) {
        return Err((StatusCode::BAD_REQUEST, "Invalid record ID".into()));
    }
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(failure)?;
    badge_service::evaluate(&pb, &player)
        .await
        .map(Json)
        .map_err(failure)
}

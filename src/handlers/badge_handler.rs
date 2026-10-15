use crate::{
    services::badge_service::{self, AwardRequest, AwardResult},
    utils::pocketbase::PocketbaseClient,
};
use axum::{Json, extract::Path, http::StatusCode};

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

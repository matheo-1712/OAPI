use crate::{
    actions::{
        friends_actions::{self, FriendError},
        rom_auth_actions::{self, RomAuthError},
    },
    models::{
        JwtClaims,
        rom_auth::{RomCodeResponse, RomDeviceList, RomTokenRequest, RomTokenResponse},
    },
    utils::pocketbase::PocketbaseClient,
};
use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct RomCodeQuery {
    pub label: Option<String>,
}

fn unavailable(error: impl std::fmt::Display) -> (StatusCode, String) {
    tracing::error!("ROM link operation failed: {}", error);
    (
        StatusCode::BAD_GATEWAY,
        "Liaison ROM temporairement indisponible.".into(),
    )
}

fn failure(error: RomAuthError) -> (StatusCode, String) {
    match error {
        RomAuthError::Invalid(message) => (StatusCode::BAD_REQUEST, message.into()),
        RomAuthError::NotFound(message) => (StatusCode::NOT_FOUND, message.into()),
        RomAuthError::Upstream(error) => unavailable(error),
    }
}

fn friend_failure(error: FriendError) -> (StatusCode, String) {
    match error {
        FriendError::Invalid(message) => (StatusCode::BAD_REQUEST, message.into()),
        FriendError::NotFound(message) => (StatusCode::NOT_FOUND, message.into()),
        FriendError::Conflict(message) => (StatusCode::CONFLICT, message.into()),
        FriendError::Forbidden => (
            StatusCode::FORBIDDEN,
            "Ce compte ne vous appartient pas.".into(),
        ),
        FriendError::Upstream(error) => unavailable(error),
    }
}

/// Authenticated PocketBase client plus the session's own Discord account.
async fn session(claims: &JwtClaims) -> Result<(PocketbaseClient, String), (StatusCode, String)> {
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(unavailable)?;
    let account = friends_actions::resolve_account(&pb, "me", claims)
        .await
        .map_err(friend_failure)?;
    Ok((pb, account))
}

#[utoipa::path(post, path = "/auth/rom/code", tag = "ROM",
    params(("label" = Option<String>, Query, description = "Device name (trimmed, max 32 characters)")),
    responses((status = 200, body = RomCodeResponse), (status = 401, description = "Authentication required"), (status = 403, description = "Service tokens cannot link a ROM or cross-origin request"), (status = 502, description = "ROM link unavailable")))]
pub async fn create_code(
    Query(query): Query<RomCodeQuery>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<RomCodeResponse>, (StatusCode, String)> {
    let (pb, account) = session(&claims).await?;
    let label = rom_auth_actions::sanitize_label(query.label.as_deref());
    rom_auth_actions::create_code(&pb, &account, &label)
        .await
        .map(Json)
        .map_err(failure)
}

#[utoipa::path(post, path = "/auth/rom/token", tag = "ROM",
    request_body = RomTokenRequest,
    responses((status = 200, body = RomTokenResponse, description = "Token shown once"), (status = 400, description = "Invalid code"), (status = 404, description = "Unknown or expired code"), (status = 502, description = "ROM link unavailable")))]
pub async fn exchange(
    Json(body): Json<RomTokenRequest>,
) -> Result<Json<RomTokenResponse>, (StatusCode, String)> {
    if !rom_auth_actions::valid_code(&body.code) {
        return Err((StatusCode::BAD_REQUEST, "Code invalide.".into()));
    }
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(unavailable)?;
    rom_auth_actions::exchange_code(&pb, &body.code)
        .await
        .map(Json)
        .map_err(failure)
}

#[utoipa::path(get, path = "/auth/rom/devices", tag = "ROM",
    responses((status = 200, body = RomDeviceList), (status = 401, description = "Authentication required"), (status = 403, description = "Service tokens have no devices"), (status = 502, description = "ROM link unavailable")))]
pub async fn devices(
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<RomDeviceList>, (StatusCode, String)> {
    let (pb, account) = session(&claims).await?;
    rom_auth_actions::list_devices(&pb, &account)
        .await
        .map(Json)
        .map_err(failure)
}

#[utoipa::path(post, path = "/auth/rom/devices/{device}/revoke", tag = "ROM",
    params(("device" = String, Path, description = "PocketBase discord_identity_token record ID")),
    responses((status = 204, description = "Device revoked"), (status = 400, description = "Invalid ID"), (status = 401, description = "Authentication required"), (status = 403, description = "Cross-origin request or service token"), (status = 404, description = "Device not found"), (status = 502, description = "ROM link unavailable")))]
pub async fn revoke(
    Path(device): Path<String>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<StatusCode, (StatusCode, String)> {
    let (pb, account) = session(&claims).await?;
    rom_auth_actions::revoke_device(&pb, &account, &device)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::middleware::SERVICE_SUBJECT;
    use crate::models::Role;

    #[tokio::test]
    async fn service_token_cannot_create_a_link_code() {
        let pb = PocketbaseClient::for_test("http://127.0.0.1:9".into(), "t");
        let claims = JwtClaims {
            sub: SERVICE_SUBJECT.into(),
            username: "astro".into(),
            role: Role::Admin,
            exp: 0,
            iat: 0,
            profile: None,
        };
        let error = friends_actions::resolve_account(&pb, "me", &claims)
            .await
            .unwrap_err();
        assert!(matches!(error, FriendError::Forbidden));
        assert_eq!(friend_failure(error).0, StatusCode::FORBIDDEN);
    }
}

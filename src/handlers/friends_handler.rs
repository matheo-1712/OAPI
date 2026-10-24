use crate::{
    actions::friends_actions::{self, FriendError},
    models::{
        JwtClaims,
        discord_friends::{FriendList, FriendshipResponse},
    },
    utils::pocketbase::PocketbaseClient,
};
use axum::{Extension, Json, extract::Path, http::StatusCode};

fn failure(error: FriendError) -> (StatusCode, String) {
    match error {
        FriendError::Invalid(message) => (StatusCode::BAD_REQUEST, message.into()),
        FriendError::NotFound(message) => (StatusCode::NOT_FOUND, message.into()),
        FriendError::Conflict(message) => (StatusCode::CONFLICT, message.into()),
        FriendError::Forbidden => (
            StatusCode::FORBIDDEN,
            "Ce compte ne vous appartient pas.".into(),
        ),
        FriendError::Upstream(error) => {
            tracing::error!("Friendship operation failed: {}", error);
            (
                StatusCode::BAD_GATEWAY,
                "Amis temporairement indisponibles.".into(),
            )
        }
    }
}

/// Authenticated PocketBase client plus the account the caller acts as.
async fn session(
    account: &str,
    claims: &JwtClaims,
) -> Result<(PocketbaseClient, String), (StatusCode, String)> {
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(|e| failure(e.into()))?;
    let account = friends_actions::resolve_account(&pb, account, claims)
        .await
        .map_err(failure)?;
    Ok((pb, account))
}

/// Lists the account's friendships.
///
/// Also accepts `Authorization: Bearer rom_...` (ROM token): read-only, own account only.
#[utoipa::path(get, path = "/friends/{account}", tag = "Friends",
    params(("account" = String, Path, description = "`me` (session only) or a PocketBase discord_users record ID")),
    responses((status = 200, body = FriendList), (status = 400, description = "Invalid ID or service used /me"), (status = 401, description = "Authentication required"), (status = 403, description = "Account belongs to another user"), (status = 404, description = "Account not found"), (status = 502, description = "Friends unavailable")))]
pub async fn list(
    Path(account): Path<String>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<FriendList>, (StatusCode, String)> {
    let (pb, account) = session(&account, &claims).await?;
    friends_actions::list(&pb, &account)
        .await
        .map(Json)
        .map_err(|e| failure(e.into()))
}

#[utoipa::path(post, path = "/friends/{account}/{user}/request", tag = "Friends",
    params(("account" = String, Path, description = "`me` (session only) or a PocketBase discord_users record ID"), ("user" = String, Path, description = "Exact Discord username/tag (e.g. rerebleue), optionally prefixed with @; never a PocketBase ID")),
    responses((status = 200, body = FriendshipResponse, description = "Request sent, or accepted when the other member had already asked"), (status = 400, description = "Invalid tag or self request"), (status = 401, description = "Authentication required"), (status = 403, description = "Account belongs to another user or cross-origin request"), (status = 404, description = "Discord tag not found"), (status = 409, description = "Already friends, request already sent, or ambiguous tag"), (status = 502, description = "Friends unavailable")))]
pub async fn request(
    Path((account, user)): Path<(String, String)>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<FriendshipResponse>, (StatusCode, String)> {
    let user = friends_actions::normalize_discord_tag(&user).map_err(failure)?;
    let (pb, account) = session(&account, &claims).await?;
    friends_actions::request(&pb, &account, &user)
        .await
        .map(Json)
        .map_err(failure)
}

#[utoipa::path(post, path = "/friends/{account}/{user}/accept", tag = "Friends",
    params(("account" = String, Path, description = "`me` (session only) or a PocketBase discord_users record ID"), ("user" = String, Path, description = "PocketBase discord_users record ID of the requester")),
    responses((status = 200, body = FriendshipResponse), (status = 400, description = "Invalid ID"), (status = 401, description = "Authentication required"), (status = 403, description = "Account belongs to another user or cross-origin request"), (status = 404, description = "No pending request from this user"), (status = 502, description = "Friends unavailable")))]
pub async fn accept(
    Path((account, user)): Path<(String, String)>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<FriendshipResponse>, (StatusCode, String)> {
    let (pb, account) = session(&account, &claims).await?;
    friends_actions::accept(&pb, &account, &user)
        .await
        .map(Json)
        .map_err(failure)
}

#[utoipa::path(post, path = "/friends/{account}/{user}/remove", tag = "Friends",
    params(("account" = String, Path, description = "`me` (session only) or a PocketBase discord_users record ID"), ("user" = String, Path, description = "PocketBase discord_users record ID of the other member")),
    responses((status = 204, description = "Friend removed, request declined or cancelled"), (status = 400, description = "Invalid ID"), (status = 401, description = "Authentication required"), (status = 403, description = "Account belongs to another user or cross-origin request"), (status = 404, description = "No friendship with this user"), (status = 502, description = "Friends unavailable")))]
pub async fn remove(
    Path((account, user)): Path<(String, String)>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<StatusCode, (StatusCode, String)> {
    let (pb, account) = session(&account, &claims).await?;
    friends_actions::remove(&pb, &account, &user)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(failure)
}

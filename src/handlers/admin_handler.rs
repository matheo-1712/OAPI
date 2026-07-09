use crate::models::DiscordUser;
use crate::utils::constants::DISCORD_USERS_COLLECTION;
use crate::utils::pocketbase::PocketbaseClient;
use axum::{Json, http::StatusCode};

/// Admin endpoint to retrieve all Discord users stored in PocketBase.
/// Requires the `Admin` role via `require_admin` middleware.
#[utoipa::path(
    get,
    path = "/admin/users",
    tag = "admin",
    responses(
        (status = 200, description = "List of all Discord users", body = [DiscordUser]),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_all_users() -> Result<Json<Vec<DiscordUser>>, (StatusCode, String)> {
    let mut pb_client = PocketbaseClient::new();
    pb_client
        .login()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let users: Vec<DiscordUser> = pb_client
        .list_all_records(DISCORD_USERS_COLLECTION, "")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(users))
}

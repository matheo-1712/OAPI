use crate::models::{DiscordUser, JwtClaims, MinecraftPlayer, Role};
use crate::services::image_service::{valid_discord_id, valid_uuid};
use crate::utils::constants::{DISCORD_USERS_COLLECTION, MINECRAFT_PLAYERS_COLLECTION};
use crate::utils::pocketbase::PocketbaseClient;

pub async fn can_access_minecraft(uuid: &str, claims: &JwtClaims) -> Result<bool, String> {
    if !valid_uuid(uuid) || !valid_discord_id(&claims.sub) {
        return Ok(false);
    }
    if claims.role == Role::Admin {
        return Ok(true);
    }
    let mut pb = PocketbaseClient::new();
    pb.login().await?;
    authorized_with_client(&pb, uuid, claims).await
}

pub(crate) async fn authorized_with_client(
    pb: &PocketbaseClient,
    uuid: &str,
    claims: &JwtClaims,
) -> Result<bool, String> {
    let players: Vec<MinecraftPlayer> = pb
        .list_all_records(
            MINECRAFT_PLAYERS_COLLECTION,
            &format!("account_id = '{}'", uuid),
        )
        .await?;
    let Some(player) = players.first() else {
        return Ok(false);
    };
    // Accept a Discord snowflake or the related PocketBase user record ID.
    if player.discord_user == claims.sub {
        return Ok(true);
    }
    let users: Vec<DiscordUser> = pb
        .list_all_records(
            DISCORD_USERS_COLLECTION,
            &format!("discord_id = '{}'", claims.sub),
        )
        .await?;
    Ok(users.iter().any(|user| user.id == player.discord_user))
}

//! Idempotent ADL onboarding: the `discord_users` record of a Discord account
//! and its `DS Games` player, shared with the other ADL services.
//!
//! Existing records are reused as they are (roles, admin flags, `account_id`
//! and connection dates are never written here). Creation is serialised per
//! key in this process and reconciled afterwards, so a concurrent creation by
//! another service or instance never leaves two rows for the same account.

use crate::services::badge_service::valid_id;
use crate::utils::constants::{
    DISCORD_USERS_COLLECTION, MINECRAFT_PLAYERS_COLLECTION as PLAYERS_COLLECTION,
    PLATFORMS_COLLECTION,
};
use crate::utils::keyed_lock;
use crate::utils::records::Records;
use serde_json::{Value, json};
use tracing::warn;

/// Exact `platforms.name` of the Nintendo DS platform.
pub const DS_GAMES_PLATFORM: &str = "DS Games";
/// ADL's `account_id` for DS Games players: no external account exists for them.
pub const DS_GAMES_ACCOUNT_ID: &str = "NA";

/// Discord identity of an authenticated session (never taken from the browser).
#[derive(Debug, Clone)]
pub struct DiscordIdentity {
    pub discord_id: String,
    /// Display name (`global_name`, or the username when unset).
    pub username: String,
    /// Discord username, used by ADL as `discord_tag`.
    pub discord_tag: String,
    pub avatar_url: Option<String>,
}

/// PocketBase records of a member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub discord_id: String,
    pub discord_user_id: String,
    pub player_id: String,
}

fn id_of(row: &Value) -> Option<String> {
    row.get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
        .map(str::to_owned)
}

/// Returns the oldest row matching `equals`, creating `body` when there is none.
///
/// A failed or uncertain create is reconciled by reading again; when another
/// writer raced us without a unique index, the oldest row wins and the row this
/// call just created is removed.
async fn ensure_record<R: Records>(
    records: &R,
    collection: &str,
    equals: &[(&str, &str)],
    body: Value,
) -> Result<String, String> {
    let key = format!(
        "onboarding:{collection}:{}",
        crate::utils::records::equality_filter(equals)
    );
    let _guard = keyed_lock::lock(&key).await;
    if let Some(id) = records
        .find(collection, equals)
        .await?
        .first()
        .and_then(id_of)
    {
        return Ok(id);
    }
    let created = records.create(collection, &body).await;
    let rows = records.find(collection, equals).await?;
    let Some(winner) = rows.first().and_then(id_of) else {
        return Err(match created {
            Err(error) => error,
            Ok(_) => format!("{collection} record missing after creation"),
        });
    };
    if let Ok(row) = &created
        && let Some(mine) = id_of(row)
        && mine != winner
    {
        // Only the row created by this call is ever removed.
        if let Err(error) = records.delete(collection, &mine).await {
            warn!("Duplicate {collection} record {mine} could not be removed: {error}");
        }
    }
    Ok(winner)
}

/// Ensures the `discord_users` record of `identity` exists and returns its ID.
pub async fn ensure_discord_user<R: Records>(
    records: &R,
    identity: &DiscordIdentity,
) -> Result<String, String> {
    if !valid_id(&identity.discord_id) || !identity.discord_id.bytes().all(|c| c.is_ascii_digit()) {
        return Err("Invalid Discord ID".into());
    }
    let mut body = json!({
        "discord_id": identity.discord_id,
        "username": identity.username,
        "discord_tag": identity.discord_tag,
    });
    if let Some(avatar) = &identity.avatar_url {
        body["avatar_url"] = json!(avatar);
    }
    let id = ensure_record(
        records,
        DISCORD_USERS_COLLECTION,
        &[("discord_id", &identity.discord_id)],
        body,
    )
    .await?;
    crate::actions::auth_actions::forget_account_state(&identity.discord_id);
    Ok(id)
}

/// ID of the single platform named exactly `DS Games`.
pub async fn ds_games_platform<R: Records>(records: &R) -> Result<String, String> {
    let rows = records
        .find(PLATFORMS_COLLECTION, &[("name", DS_GAMES_PLATFORM)])
        .await?;
    let exact: Vec<_> = rows
        .iter()
        .filter(|row| row.get("name").and_then(Value::as_str) == Some(DS_GAMES_PLATFORM))
        .filter_map(id_of)
        .collect();
    match exact.as_slice() {
        [id] => Ok(id.clone()),
        [] => Err("No 'DS Games' platform in PocketBase".into()),
        _ => Err("Several 'DS Games' platforms in PocketBase".into()),
    }
}

/// Ensures the DS Games player of `discord_user_id` exists and returns its ID.
/// An existing player is returned untouched, `account_id` included.
pub async fn ensure_ds_player<R: Records>(
    records: &R,
    discord_user_id: &str,
    platform_id: &str,
    playername: &str,
) -> Result<String, String> {
    ensure_record(
        records,
        PLAYERS_COLLECTION,
        &[("discord_user", discord_user_id), ("platform", platform_id)],
        json!({
            "discord_user": discord_user_id,
            "platform": platform_id,
            "account_id": DS_GAMES_ACCOUNT_ID,
            "playername": playername,
        }),
    )
    .await
}

/// Resolves (creating when missing) the member's Discord user and DS Games player.
pub async fn ensure_member<R: Records>(
    records: &R,
    identity: &DiscordIdentity,
) -> Result<Member, String> {
    let discord_user_id = ensure_discord_user(records, identity).await?;
    let platform = ds_games_platform(records).await?;
    let player_id =
        ensure_ds_player(records, &discord_user_id, &platform, &identity.username).await?;
    Ok(Member {
        discord_id: identity.discord_id.clone(),
        discord_user_id,
        player_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::records::fake::{CreateFault, FakeRecords};

    pub(crate) fn identity(discord_id: &str) -> DiscordIdentity {
        DiscordIdentity {
            discord_id: discord_id.into(),
            username: "Cocow".into(),
            discord_tag: "cocow".into(),
            avatar_url: Some("https://cdn.discordapp.com/avatars/1/a.png".into()),
        }
    }

    pub(crate) fn store_with_platforms() -> FakeRecords {
        let store = FakeRecords::default();
        store.insert(
            PLATFORMS_COLLECTION,
            json!({"id":"mcplatform00001","name":"Minecraft"}),
        );
        store.insert(
            PLATFORMS_COLLECTION,
            json!({"id":"dsgames00000001","name":"DS Games"}),
        );
        store.insert(
            PLATFORMS_COLLECTION,
            json!({"id":"dsgameslower001","name":"ds games"}),
        );
        store
    }

    #[tokio::test]
    async fn creates_a_member_with_the_onboarding_fields_only() {
        let store = store_with_platforms();
        let member = ensure_member(&store, &identity("111")).await.unwrap();
        let users = store.rows(DISCORD_USERS_COLLECTION);
        assert_eq!(users.len(), 1);
        assert_eq!(users[0]["discord_id"], "111");
        assert_eq!(users[0]["username"], "Cocow");
        assert_eq!(users[0]["discord_tag"], "cocow");
        assert!(users[0].get("is_admin").is_none() && users[0].get("roles").is_none());
        let players = store.rows(PLAYERS_COLLECTION);
        assert_eq!(players.len(), 1);
        assert_eq!(players[0]["platform"], "dsgames00000001");
        assert_eq!(players[0]["discord_user"], member.discord_user_id.as_str());
        assert_eq!(players[0]["account_id"], DS_GAMES_ACCOUNT_ID);
        assert!(players[0].get("first_connected_at").is_none());
        assert!(players[0].get("last_connected_at").is_none());
        assert_eq!(member.player_id, players[0]["id"]);
    }

    #[tokio::test]
    async fn existing_records_and_account_ids_are_reused_untouched() {
        let store = store_with_platforms();
        store.insert(
            DISCORD_USERS_COLLECTION,
            json!({"id":"user00000000001","discord_id":"222","is_admin":true,"roles":[{"id":"9"}],"username":"Old"}),
        );
        store.insert(
            PLAYERS_COLLECTION,
            json!({"id":"0s90t9j1i8zxpwr","discord_user":"user00000000001","platform":"dsgames00000001","account_id":"ADL-123"}),
        );
        // Another platform's player for the same user is not the DS Games player.
        store.insert(
            PLAYERS_COLLECTION,
            json!({"id":"mcplayer0000001","discord_user":"user00000000001","platform":"mcplatform00001","account_id":"uuid"}),
        );
        let member = ensure_member(&store, &identity("222")).await.unwrap();
        assert_eq!(member.discord_user_id, "user00000000001");
        assert_eq!(member.player_id, "0s90t9j1i8zxpwr");
        assert!(store.creates.lock().unwrap().is_empty());
        let user = &store.rows(DISCORD_USERS_COLLECTION)[0];
        assert_eq!(user["is_admin"], true);
        assert_eq!(user["username"], "Old");
        assert_eq!(store.rows(PLAYERS_COLLECTION)[0]["account_id"], "ADL-123");
    }

    #[tokio::test]
    async fn concurrent_sign_ins_create_a_single_user_and_player() {
        let store = store_with_platforms();
        let id = identity("333");
        let results = futures_join(&store, &id).await;
        assert!(
            results
                .iter()
                .all(|r| r.as_ref().unwrap() == results[0].as_ref().unwrap())
        );
        assert_eq!(store.rows(DISCORD_USERS_COLLECTION).len(), 1);
        assert_eq!(store.rows(PLAYERS_COLLECTION).len(), 1);
    }

    async fn futures_join(
        store: &FakeRecords,
        id: &DiscordIdentity,
    ) -> Vec<Result<Member, String>> {
        let (a, b, c, d) = tokio::join!(
            ensure_member(store, id),
            ensure_member(store, id),
            ensure_member(store, id),
            ensure_member(store, id)
        );
        vec![a, b, c, d]
    }

    #[tokio::test]
    async fn a_race_with_another_service_keeps_the_oldest_row() {
        // Without a unique index: both rows land, ours is removed.
        let store = store_with_platforms();
        store.fail_next_create(DISCORD_USERS_COLLECTION, CreateFault::ConcurrentDuplicate);
        store.fail_next_create(PLAYERS_COLLECTION, CreateFault::ConcurrentDuplicate);
        let member = ensure_member(&store, &identity("444")).await.unwrap();
        assert_eq!(store.rows(DISCORD_USERS_COLLECTION).len(), 1);
        assert_eq!(store.rows(PLAYERS_COLLECTION).len(), 1);
        assert_eq!(
            store.rows(PLAYERS_COLLECTION)[0]["id"],
            member.player_id.as_str()
        );
        assert_eq!(store.deletes.lock().unwrap().len(), 2);

        // With a unique index: our create is rejected and the other row is reused.
        let store = store_with_platforms();
        store.unique(DISCORD_USERS_COLLECTION, &["discord_id"]);
        store.unique(PLAYERS_COLLECTION, &["discord_user", "platform"]);
        store.fail_next_create(DISCORD_USERS_COLLECTION, CreateFault::ConcurrentDuplicate);
        store.fail_next_create(PLAYERS_COLLECTION, CreateFault::ConcurrentDuplicate);
        ensure_member(&store, &identity("444")).await.unwrap();
        assert_eq!(store.rows(DISCORD_USERS_COLLECTION).len(), 1);
        assert_eq!(store.rows(PLAYERS_COLLECTION).len(), 1);
        assert!(store.deletes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn uncertain_creates_are_reconciled_without_a_second_row() {
        let store = store_with_platforms();
        store.fail_next_create(DISCORD_USERS_COLLECTION, CreateFault::StoredThenTimeout);
        store.fail_next_create(PLAYERS_COLLECTION, CreateFault::StoredThenTimeout);
        ensure_member(&store, &identity("555")).await.unwrap();
        assert_eq!(store.rows(DISCORD_USERS_COLLECTION).len(), 1);
        assert_eq!(store.rows(PLAYERS_COLLECTION).len(), 1);

        let store = store_with_platforms();
        store.fail_next_create(PLAYERS_COLLECTION, CreateFault::Lost);
        assert!(ensure_member(&store, &identity("555")).await.is_err());
        // The retry creates the player once.
        ensure_member(&store, &identity("555")).await.unwrap();
        assert_eq!(store.rows(PLAYERS_COLLECTION).len(), 1);
    }

    #[tokio::test]
    async fn platform_must_match_exactly_once_and_ids_are_validated() {
        let store = FakeRecords::default();
        assert!(ds_games_platform(&store).await.is_err());
        store.insert(
            PLATFORMS_COLLECTION,
            json!({"name":"DS Games","id":"a00000000000001"}),
        );
        store.insert(
            PLATFORMS_COLLECTION,
            json!({"name":"DS Games","id":"a00000000000002"}),
        );
        assert!(ds_games_platform(&store).await.is_err());
        let store = store_with_platforms();
        for bad in ["", "12a", "1' || '1'='1"] {
            assert!(ensure_discord_user(&store, &identity(bad)).await.is_err());
        }
        assert!(store.creates.lock().unwrap().is_empty());
    }
}

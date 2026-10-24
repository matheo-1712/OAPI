//! Permanent purge of Discord accounts whose deletion date has passed.
//!
//! Linked data is removed first and the `discord_users` record last, so a failed
//! step leaves the account in place and the purge is retried on the next run.
//! Logs only carry record IDs, never usernames, tags or Discord snowflakes.

use crate::services::badge_service::valid_id;
use crate::services::scheduler::TaskSummary;
use crate::utils::constants::{
    DISCORD_FRIENDSHIPS_COLLECTION, DISCORD_IDENTITY_TOKEN_COLLECTION,
    DISCORD_USER_BADGES_COLLECTION, DISCORD_USER_STATS_COLLECTION, DISCORD_USERS_COLLECTION,
    MINECRAFT_PLAYERS_COLLECTION,
};
use crate::utils::pocketbase::PocketbaseClient;
use serde_json::{Value, json};

/// Filter matching `field` against the record ID and, when known, the Discord snowflake.
fn account_filter(field: &str, record: &str, snowflake: Option<&str>) -> String {
    match snowflake {
        Some(s) => format!("{field} = '{record}' || {field} = '{s}'"),
        None => format!("{field} = '{record}'"),
    }
}

/// Deletes every record of `collection` matching `filter`.
async fn delete_matching(
    pb: &PocketbaseClient,
    collection: &str,
    filter: &str,
) -> Result<(), String> {
    for row in pb.list_all_records::<Value>(collection, filter).await? {
        let id = row["id"].as_str().ok_or("record without id")?;
        pb.delete_record(collection, id).await?;
    }
    Ok(())
}

/// Removes one account's data, then the account itself. Stops at the first error.
async fn purge_account(
    pb: &PocketbaseClient,
    record: &str,
    snowflake: Option<&str>,
) -> Result<(), String> {
    if !valid_id(record) {
        return Err("invalid record id".into());
    }
    let by_user = account_filter("discord_user", record, snowflake);
    delete_matching(pb, DISCORD_USER_STATS_COLLECTION, &by_user).await?;
    delete_matching(pb, DISCORD_USER_BADGES_COLLECTION, &by_user).await?;
    let friends = format!("user_a = '{record}' || user_b = '{record}'");
    delete_matching(pb, DISCORD_FRIENDSHIPS_COLLECTION, &friends).await?;
    let tokens = account_filter("discord_user", record, None);
    delete_matching(pb, DISCORD_IDENTITY_TOKEN_COLLECTION, &tokens).await?;
    // Minecraft players are kept: only the link to the Discord account is cleared.
    for row in pb
        .list_all_records::<Value>(MINECRAFT_PLAYERS_COLLECTION, &by_user)
        .await?
    {
        let id = row["id"].as_str().ok_or("record without id")?;
        pb.update_record(MINECRAFT_PLAYERS_COLLECTION, id, &json!({"discord_user": ""}))
            .await?;
    }
    pb.delete_record(DISCORD_USERS_COLLECTION, record).await
}

/// Purges all accounts whose `delete_at` is set and already past.
///
/// Returns `Err` only if the list of accounts cannot be read; per-account failures
/// are reported in [`TaskSummary::failed`].
pub async fn purge_deleted_accounts(pb: &PocketbaseClient) -> Result<TaskSummary, String> {
    let accounts = pb
        .list_all_records::<Value>(DISCORD_USERS_COLLECTION, "delete_at != '' && delete_at <= @now")
        .await?;
    let mut summary = TaskSummary::default();
    for account in accounts {
        summary.processed += 1;
        let record = account["id"].as_str().unwrap_or_default();
        let snowflake = account["discord_id"].as_str().filter(|s| valid_id(s));
        match purge_account(pb, record, snowflake).await {
            Ok(()) => summary.succeeded += 1,
            Err(e) => {
                tracing::warn!("Purge of Discord account {record} failed: {e}");
                summary.failed.push(record.to_string());
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    const USERS: &str = "/api/collections/discord_users/records";
    const STATS: &str = "/api/collections/discord_user_stats/records";
    const BADGES: &str = "/api/collections/badges_earned/records";
    const FRIENDS: &str = "/api/collections/discord_friendships/records";
    const TOKENS: &str = "/api/collections/discord_identity_token/records";
    const PLAYERS: &str = "/api/collections/players/records";
    const DUE: &str = "delete_at != '' && delete_at <= @now";

    fn page(items: Value) -> String {
        let n = items.as_array().map_or(0, |a| a.len());
        json!({"page":1,"perPage":500,"totalPages":1,"totalItems":n,"items":items}).to_string()
    }

    async fn list_mock(server: &mut Server, path: &str, f: &str, items: Value) -> mockito::Mock {
        server
            .mock("GET", path)
            .match_query(Matcher::UrlEncoded("filter".into(), f.into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(page(items))
            .create_async()
            .await
    }

    async fn delete_mock(server: &mut Server, path: &str, status: usize, n: usize) -> mockito::Mock {
        server
            .mock("DELETE", path)
            .with_status(status)
            .expect(n)
            .create_async()
            .await
    }

    #[test]
    fn account_filter_matches_record_and_snowflake() {
        assert_eq!(
            account_filter("discord_user", "rec1", Some("123")),
            "discord_user = 'rec1' || discord_user = '123'"
        );
    }

    #[test]
    fn account_filter_ignores_invalid_snowflake() {
        assert_eq!(account_filter("discord_user", "rec1", None), "discord_user = 'rec1'");
    }

    #[tokio::test]
    async fn purges_linked_data_in_order_then_deletes_account() {
        let mut server = Server::new_async().await;
        let both = "discord_user = 'rec1' || discord_user = '123'";
        let mut mocks = vec![
            list_mock(&mut server, USERS, DUE, json!([{"id":"rec1","discord_id":"123"}])).await,
            list_mock(&mut server, STATS, both, json!([{"id":"s1"}])).await,
            list_mock(&mut server, BADGES, both, json!([{"id":"b1"}])).await,
            list_mock(&mut server, FRIENDS, "user_a = 'rec1' || user_b = 'rec1'", json!([{"id":"f1"}])).await,
            list_mock(&mut server, TOKENS, "discord_user = 'rec1'", json!([{"id":"t1"}])).await,
            list_mock(&mut server, PLAYERS, both, json!([{"id":"p1"}])).await,
        ];
        for path in [
            format!("{STATS}/s1"),
            format!("{BADGES}/b1"),
            format!("{FRIENDS}/f1"),
            format!("{TOKENS}/t1"),
            format!("{USERS}/rec1"),
        ] {
            mocks.push(delete_mock(&mut server, &path, 204, 1).await);
        }
        mocks.push(
            server
                .mock("PATCH", format!("{PLAYERS}/p1").as_str())
                .match_body(Matcher::Json(json!({"discord_user": ""})))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(json!({"id":"p1"}).to_string())
                .expect(1)
                .create_async()
                .await,
        );
        let pb = PocketbaseClient::for_test(server.url(), "t");
        let summary = purge_deleted_accounts(&pb).await.unwrap();
        assert_eq!(
            summary,
            TaskSummary { processed: 1, succeeded: 1, failed: vec![] }
        );
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn failed_step_keeps_account_and_continues() {
        let mut server = Server::new_async().await;
        let mut mocks = vec![
            list_mock(&mut server, USERS, DUE, json!([{"id":"rec1"},{"id":"rec2"}])).await,
        ];
        for (rec, stat) in [("rec1", "s1"), ("rec2", "s2")] {
            let own = format!("discord_user = '{rec}'");
            let friends = format!("user_a = '{rec}' || user_b = '{rec}'");
            mocks.push(list_mock(&mut server, STATS, &own, json!([{"id":stat}])).await);
            if rec == "rec1" {
                continue; // stops at the failed stats deletion: no later list call
            }
            mocks.push(list_mock(&mut server, BADGES, &own, json!([])).await);
            mocks.push(list_mock(&mut server, FRIENDS, &friends, json!([])).await);
            mocks.push(list_mock(&mut server, TOKENS, &own, json!([])).await);
            mocks.push(list_mock(&mut server, PLAYERS, &own, json!([])).await);
        }
        mocks.push(delete_mock(&mut server, &format!("{STATS}/s1"), 500, 1).await);
        mocks.push(delete_mock(&mut server, &format!("{STATS}/s2"), 204, 1).await);
        mocks.push(delete_mock(&mut server, &format!("{USERS}/rec1"), 204, 0).await);
        mocks.push(delete_mock(&mut server, &format!("{USERS}/rec2"), 204, 1).await);
        let pb = PocketbaseClient::for_test(server.url(), "t");
        let summary = purge_deleted_accounts(&pb).await.unwrap();
        assert_eq!(
            summary,
            TaskSummary { processed: 2, succeeded: 1, failed: vec!["rec1".into()] }
        );
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn invalid_record_id_is_failed_without_writes() {
        let mut server = Server::new_async().await;
        let users = list_mock(&mut server, USERS, DUE, json!([{"id":"bad id"}])).await;
        let other = server
            .mock("GET", Matcher::Regex("^/api/collections/(discord_user_stats|badges_earned|discord_friendships|discord_identity_token|players)/".into()))
            .expect(0)
            .create_async()
            .await;
        let writes = server
            .mock("DELETE", Matcher::Any)
            .expect(0)
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        let summary = purge_deleted_accounts(&pb).await.unwrap();
        assert_eq!(
            summary,
            TaskSummary { processed: 1, succeeded: 0, failed: vec!["bad id".into()] }
        );
        users.assert_async().await;
        other.assert_async().await;
        writes.assert_async().await;
    }
}

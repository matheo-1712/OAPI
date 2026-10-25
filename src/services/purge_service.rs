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
use crate::utils::pocketbase::quote;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::{Value, json};

/// Safety cap: above this many due accounts, a run purges nothing.
const MAX_PURGE_PER_RUN: usize = 100;

/// Filter matching `field` against the record ID and, when known, the Discord snowflake.
fn account_filter(field: &str, record: &str, snowflake: Option<&str>) -> String {
    match snowflake {
        Some(s) => format!("{} = {} || {} = {}", field, quote(record), field, quote(s)),
        None => format!("{} = {}", field, quote(record)),
    }
}

/// Whether `value` is a deletion date (PocketBase or RFC 3339 format) that is not after `now`.
/// Anything else (empty, missing, unparseable) is not due.
fn deletion_due(value: &Value, now: DateTime<Utc>) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    let date = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.fZ")
        .map(|naive| naive.and_utc())
        .or_else(|_| DateTime::parse_from_rfc3339(text).map(|d| d.with_timezone(&Utc)));
    date.is_ok_and(|d| d <= now)
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
    let friends = format!("user_a = {} || user_b = {}", quote(record), quote(record));
    delete_matching(pb, DISCORD_FRIENDSHIPS_COLLECTION, &friends).await?;
    let tokens = account_filter("discord_user", record, None);
    delete_matching(pb, DISCORD_IDENTITY_TOKEN_COLLECTION, &tokens).await?;
    // Minecraft players are kept: only the link to the Discord account is cleared.
    for row in pb
        .list_all_records::<Value>(MINECRAFT_PLAYERS_COLLECTION, &by_user)
        .await?
    {
        let id = row["id"].as_str().ok_or("record without id")?;
        pb.update_record(
            MINECRAFT_PLAYERS_COLLECTION,
            id,
            &json!({"discord_user": ""}),
        )
        .await?;
    }
    pb.delete_record(DISCORD_USERS_COLLECTION, record).await?;
    // Summary images show the member's Discord profile and statistics.
    if let Some(snowflake) = snowflake {
        crate::services::image_cleanup::remove_discord_images(snowflake).await;
    }
    Ok(())
}

/// Purges all accounts whose `delete_at` is set and already past.
///
/// The date is re-checked here: an account whose `delete_at` is not a past date is
/// reported as failed and left untouched. More than [`MAX_PURGE_PER_RUN`] due accounts
/// purges nothing.
///
/// Returns `Err` if the list of accounts cannot be read or exceeds the cap; per-account
/// failures are reported in [`TaskSummary::failed`].
pub async fn purge_deleted_accounts(pb: &PocketbaseClient) -> Result<TaskSummary, String> {
    let accounts = pb
        .list_all_records::<Value>(
            DISCORD_USERS_COLLECTION,
            "delete_at != '' && delete_at <= @now",
        )
        .await?;
    if accounts.len() > MAX_PURGE_PER_RUN {
        return Err(format!(
            "{} accounts due for deletion, above the limit of {MAX_PURGE_PER_RUN}; nothing purged",
            accounts.len()
        ));
    }
    let now = Utc::now();
    let mut summary = TaskSummary::default();
    for account in accounts {
        summary.processed += 1;
        let record = account["id"].as_str().unwrap_or_default();
        if !deletion_due(&account["delete_at"], now) {
            tracing::warn!("Discord account {record} not purged: delete_at is not a past date");
            summary.failed.push(record.to_string());
            continue;
        }
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
    const PAST: &str = "2020-01-01 00:00:00.000Z";
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

    async fn delete_mock(
        server: &mut Server,
        path: &str,
        status: usize,
        n: usize,
    ) -> mockito::Mock {
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
        assert_eq!(
            account_filter("discord_user", "rec1", None),
            "discord_user = 'rec1'"
        );
    }

    #[tokio::test]
    async fn purges_linked_data_in_order_then_deletes_account() {
        let mut server = Server::new_async().await;
        let both = "discord_user = 'rec1' || discord_user = '123'";
        let mut mocks = vec![
            list_mock(
                &mut server,
                USERS,
                DUE,
                json!([{"id":"rec1","discord_id":"123","delete_at":PAST}]),
            )
            .await,
            list_mock(&mut server, STATS, both, json!([{"id":"s1"}])).await,
            list_mock(&mut server, BADGES, both, json!([{"id":"b1"}])).await,
            list_mock(
                &mut server,
                FRIENDS,
                "user_a = 'rec1' || user_b = 'rec1'",
                json!([{"id":"f1"}]),
            )
            .await,
            list_mock(
                &mut server,
                TOKENS,
                "discord_user = 'rec1'",
                json!([{"id":"t1"}]),
            )
            .await,
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
            TaskSummary {
                processed: 1,
                succeeded: 1,
                failed: vec![]
            }
        );
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn failed_step_keeps_account_and_continues() {
        let mut server = Server::new_async().await;
        let mut mocks = vec![
            list_mock(
                &mut server,
                USERS,
                DUE,
                json!([{"id":"rec1","delete_at":PAST},{"id":"rec2","delete_at":PAST}]),
            )
            .await,
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
            TaskSummary {
                processed: 2,
                succeeded: 1,
                failed: vec!["rec1".into()]
            }
        );
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn invalid_record_id_is_failed_without_writes() {
        let mut server = Server::new_async().await;
        let users = list_mock(
            &mut server,
            USERS,
            DUE,
            json!([{"id":"bad id","delete_at":PAST}]),
        )
        .await;
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
            TaskSummary {
                processed: 1,
                succeeded: 0,
                failed: vec!["bad id".into()]
            }
        );
        users.assert_async().await;
        other.assert_async().await;
        writes.assert_async().await;
    }

    #[test]
    fn deletion_due_accepts_both_formats_and_rejects_the_rest() {
        let now = DateTime::parse_from_rfc3339("2026-10-24T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        for due in [
            "2026-10-24 11:59:59.000Z",
            "2026-10-24 12:00:00.000Z",
            "2026-10-24T11:00:00Z",
            "2020-01-01 00:00:00Z",
        ] {
            assert!(deletion_due(&json!(due), now), "{due}");
        }
        for not_due in [
            "2026-10-24 12:15:00.000Z",
            "2026-10-24T12:15:00Z",
            "",
            "01/12/2020",
            "garbage",
        ] {
            assert!(!deletion_due(&json!(not_due), now), "{not_due}");
        }
        assert!(!deletion_due(&Value::Null, now));
        assert!(!deletion_due(&json!(42), now));
    }

    #[tokio::test]
    async fn future_or_unparseable_delete_at_is_never_purged() {
        let mut server = Server::new_async().await;
        let future = (Utc::now() + chrono::Duration::days(365))
            .format("%Y-%m-%d %H:%M:%S%.3fZ")
            .to_string();
        let users = list_mock(
            &mut server,
            USERS,
            DUE,
            json!([
                {"id":"rec1","delete_at":future},
                {"id":"rec2","delete_at":""},
                {"id":"rec3","delete_at":"01/12/2026"}
            ]),
        )
        .await;
        let deletes = server
            .mock("DELETE", Matcher::Any)
            .expect(0)
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        let summary = purge_deleted_accounts(&pb).await.unwrap();
        assert_eq!(
            summary,
            TaskSummary {
                processed: 3,
                succeeded: 0,
                failed: vec!["rec1".into(), "rec2".into(), "rec3".into()]
            }
        );
        users.assert_async().await;
        deletes.assert_async().await;
    }

    #[tokio::test]
    async fn too_many_due_accounts_purges_nothing() {
        let mut server = Server::new_async().await;
        let items: Vec<Value> = (0..=MAX_PURGE_PER_RUN)
            .map(|i| json!({"id": format!("rec{i}"), "delete_at": PAST}))
            .collect();
        let users = list_mock(&mut server, USERS, DUE, Value::Array(items)).await;
        let deletes = server
            .mock("DELETE", Matcher::Any)
            .expect(0)
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        let error = purge_deleted_accounts(&pb).await.unwrap_err();
        assert!(
            error.contains(&format!("{} accounts", MAX_PURGE_PER_RUN + 1)),
            "{error}"
        );
        assert!(!error.contains("rec0"), "{error}");
        users.assert_async().await;
        deletes.assert_async().await;
    }
}

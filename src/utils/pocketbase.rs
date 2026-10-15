//! Utility for interacting with Pocketbase.
//!
//! Provides a client to authenticate and fetch records from Pocketbase collections.

use crate::config::Config;
use serde::Deserialize;
use tracing::{debug, error};

/// Pocketbase authentication response for admins.
#[derive(Deserialize)]
struct AdminAuthResponse {
    token: String,
}

/// Pocketbase list response for collections.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResponse<T> {
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
    pub total_items: u32,
    pub items: Vec<T>,
}

/// A client for interacting with Pocketbase.
pub struct PocketbaseClient {
    base_url: String,
    token: Option<String>,
}

impl PocketbaseClient {
    /// Creates a record using the API's authenticated PocketBase identity.
    pub async fn create_record(
        &self,
        collection: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let token = self
            .token
            .as_ref()
            .ok_or("Pocketbase client not authenticated")?;
        let response = reqwest::Client::new()
            .post(format!(
                "{}/api/collections/{}/records",
                self.base_url, collection
            ))
            .header("Authorization", token)
            .timeout(std::time::Duration::from_secs(10))
            .json(body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("PocketBase create failed: {}", response.status()));
        }
        response.json().await.map_err(|e| e.to_string())
    }

    /// Creates a new Pocketbase client using configuration.
    pub fn new() -> Self {
        let auth = &Config::global().auth;
        Self {
            base_url: auth.pb_url.clone(),
            token: None,
        }
    }

    /// Authenticates as an admin using credentials from config.
    pub async fn login(&mut self) -> Result<(), String> {
        let auth = &Config::global().auth;
        let base_url = reqwest::Url::parse(&self.base_url)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
            .ok_or_else(|| {
                "Invalid PB_URL: set the PocketBase HTTP/HTTPS base URL in .env (for example http://127.0.0.1:8090)".to_string()
            })?;
        let url = format!(
            "{}/api/collections/_superusers/auth-with-password",
            base_url.as_str().trim_end_matches('/')
        );

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .timeout(std::time::Duration::from_secs(10))
            .json(&serde_json::json!({
                "identity": auth.pb_email,
                "password": auth.pb_password,
            }))
            .send()
            .await
            .map_err(|e| format!("Pocketbase login request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let error_text = resp.text().await.unwrap_or_default();
            error!("Pocketbase login failed: {}", error_text);
            return Err(format!("Pocketbase login failed with status {}", status));
        }

        let auth_data: AdminAuthResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Pocketbase auth response: {}", e))?;

        self.token = Some(auth_data.token);
        debug!("Pocketbase authenticated successfully");
        Ok(())
    }

    /// Lists records from a collection with custom parameters.
    pub async fn list_records_with_params<T, P>(
        &self,
        collection: &str,
        params: P,
    ) -> Result<ListResponse<T>, String>
    where
        T: for<'de> Deserialize<'de>,
        P: serde::Serialize,
    {
        let url = format!("{}/api/collections/{}/records", self.base_url, collection);
        self.fetch_with_auth_and_params(url, &format!("records from {}", collection), params)
            .await
    }

    /// Counts records without downloading the collection.
    pub async fn count_records(&self, collection: &str) -> Result<u32, String> {
        // Only request one ID; PocketBase returns the total in its pagination metadata.
        let response: ListResponse<serde_json::Value> = self
            .list_records_with_params(
                collection,
                [
                    ("page", "1"),
                    ("perPage", "1"),
                    ("fields", "id"),
                    ("skipTotal", "false"),
                ],
            )
            .await?;
        Ok(response.total_items)
    }

    /// Lists ALL records from a collection by automatically paginating.
    pub async fn list_all_records<T>(
        &self,
        collection: &str,
        filter: &str,
    ) -> Result<Vec<T>, String>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut all_items = Vec::new();
        let mut current_page = 1;

        loop {
            let mut params = std::collections::HashMap::new();
            if !filter.is_empty() {
                params.insert("filter", filter.to_string());
            }
            params.insert("page", current_page.to_string());
            params.insert("perPage", "500".to_string()); // PocketBase max is 500

            let response: ListResponse<T> =
                self.list_records_with_params(collection, params).await?;

            debug!(
                "Fetched page {}/{} from {} (perPage: {}, totalItems: {})",
                response.page,
                response.total_pages,
                collection,
                response.per_page,
                response.total_items
            );

            all_items.extend(response.items);

            if current_page >= response.total_pages || response.total_pages == 0 {
                break;
            }
            current_page += 1;
        }

        debug!(
            "Fetched total of {} records from {}",
            all_items.len(),
            collection
        );
        Ok(all_items)
    }

    async fn fetch_with_auth_and_params<T, P>(
        &self,
        url: String,
        description: &str,
        params: P,
    ) -> Result<T, String>
    where
        T: for<'de> Deserialize<'de>,
        P: serde::Serialize,
    {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| "Pocketbase client not authenticated".to_string())?;

        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .header("Authorization", token)
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch {}: {}", description, e))?;

        let final_url = resp.url().to_string();
        let status = resp.status();

        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to get {} text: {}", description, e))?;

        if !status.is_success() {
            error!(
                "Pocketbase fetch failed for {} at {}: {} - {}",
                description, final_url, status, text
            );
            return Err(format!(
                "Pocketbase returned error {} for {}",
                status, description
            ));
        }

        let data = serde_json::from_str(&text).map_err(|e| {
            error!("Failed to parse {} JSON: {}", description, e);
            format!("Failed to parse {}: {}", description, e)
        })?;

        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::investor_actions::fetch_stats;
    use mockito::{Matcher, Server};

    #[tokio::test]
    async fn discord_badge_evaluation_reads_stored_stats_and_retries_without_duplicate_writes() {
        let mut server = Server::new_async().await;
        let pb = PocketbaseClient {
            base_url: server.url(),
            token: Some("api-token".into()),
        };
        let page = |items: Vec<serde_json::Value>| {
            serde_json::json!({
                "page":1, "perPage":500, "totalPages":1, "totalItems":items.len(), "items":items
            })
            .to_string()
        };
        let user = server
            .mock("GET", "/api/collections/discord_users/records")
            .match_query(Matcher::UrlEncoded("filter".into(), "id = 'user'".into()))
            .with_status(200)
            .with_body(page(vec![
                serde_json::json!({"id":"user", "discord_id":"123", "joined_at":"2026-01-01"}),
            ]))
            .expect(2)
            .create_async()
            .await;
        let stats = server
            .mock("GET", "/api/collections/discord_user_stats/records")
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "discord_user = 'user' || discord_user = '123'".into(),
            ))
            .with_status(200)
            .with_body(page(vec![
                serde_json::json!({"message_count":1000, "vocal_time":"100"}),
            ]))
            .expect(2)
            .create_async()
            .await;
        let names = [
            "Message I",
            "Message II",
            "Message III",
            "Message IV",
            "Message V",
            "Vocal I",
            "Vocal II",
            "Vocal III",
            "Vocal IV",
            "Vocal V",
            "Bienvenue dans l'Antre",
        ];
        let badges = server.mock("GET", "/api/collections/badges/records")
            .match_query(Matcher::Any).with_status(200)
            .with_body(page(names.iter().enumerate().map(|(i, name)| serde_json::json!({"id":format!("badge{i}"), "name":name, "plateform":"Discord", "is_enabled":true})).collect()))
            .expect(2).create_async().await;
        let mut reads = Vec::new();
        let mut writes = Vec::new();
        let expected = [0, 1, 5, 6, 10];
        for i in expected {
            reads.push(
                server
                    .mock("GET", "/api/collections/badges_earned/records")
                    .match_query(Matcher::UrlEncoded(
                        "filter".into(),
                        format!(
                            "(discord_user = 'user' || discord_user = '123') && badge = 'badge{i}'"
                        ),
                    ))
                    .with_status(200)
                    .with_body(page(vec![]))
                    .create_async()
                    .await,
            );
            writes.push(
                server
                    .mock("POST", "/api/collections/badges_earned/records")
                    .match_header("authorization", "api-token")
                    .match_body(Matcher::Json(
                        serde_json::json!({"discord_user":"user", "badge":format!("badge{i}")}),
                    ))
                    .with_status(200)
                    .with_header("content-type", "application/json")
                    .with_body(r#"{"id":"earned"}"#)
                    .create_async()
                    .await,
            );
        }
        let result = crate::services::badge_service::evaluate_discord(&pb, "user")
            .await
            .unwrap();
        assert_eq!(result.len(), expected.len());
        assert!(result.iter().all(|r| r.created));
        for read in &reads {
            read.assert_async().await;
            read.remove_async().await;
        }
        reads.clear();
        for i in expected {
            reads.push(
                server
                    .mock("GET", "/api/collections/badges_earned/records")
                    .match_query(Matcher::UrlEncoded(
                        "filter".into(),
                        format!(
                            "(discord_user = 'user' || discord_user = '123') && badge = 'badge{i}'"
                        ),
                    ))
                    .with_status(200)
                    .with_body(page(vec![serde_json::json!({"id":"earned"})]))
                    .create_async()
                    .await,
            );
        }
        let result = crate::services::badge_service::evaluate_discord(&pb, "user")
            .await
            .unwrap();
        assert_eq!(result.len(), expected.len());
        assert!(result.iter().all(|r| !r.created));
        for mock in reads.iter().chain(writes.iter()) {
            mock.assert_async().await;
        }
        user.assert_async().await;
        stats.assert_async().await;
        badges.assert_async().await;
    }

    #[tokio::test]
    async fn badge_creation_authenticates_and_propagates_write_failures() {
        let mut server = Server::new_async().await;
        let pb = PocketbaseClient {
            base_url: server.url(),
            token: Some("api-token".into()),
        };
        for status in [200, 400] {
            let write = server
                .mock("POST", "/api/collections/badges_earned/records")
                .match_header("authorization", "api-token")
                .match_body(Matcher::Json(
                    serde_json::json!({"player":"player", "badge":"badge"}),
                ))
                .with_status(status)
                .with_header("content-type", "application/json")
                .with_body(r#"{"id":"earned"}"#)
                .create_async()
                .await;
            let result = pb
                .create_record(
                    "badges_earned",
                    &serde_json::json!({"player":"player", "badge":"badge"}),
                )
                .await;
            assert_eq!(result.is_ok(), status == 200);
            write.assert_async().await;
        }
    }

    #[tokio::test]
    async fn minecraft_ownership_resolves_relations_and_denies_other_accounts() {
        let uuid = "123456781234123412341234567890ab";
        for (linked_user, actual_user, expected) in [
            ("123", "unused", true),
            ("user-record", "user-record", true),
            ("someone-else", "user-record", false),
        ] {
            let mut server = Server::new_async().await;
            let player = server
                .mock("GET", "/api/collections/players/records")
                .match_query(Matcher::UrlEncoded(
                    "filter".into(),
                    format!("account_id = '{}'", uuid),
                ))
                .with_status(200)
                .with_body(
                    serde_json::json!({
                        "page": 1, "perPage": 500, "totalPages": 1, "totalItems": 1,
                        "items": [{"id": "player", "discord_user": linked_user,
                            "account_id": uuid, "platform": "java", "playername": "Test",
                            "first_connected_at": "2025-01-01", "last_connected_at": "2026-01-01"}]
                    })
                    .to_string(),
                )
                .create_async()
                .await;
            let _user = server
                .mock("GET", "/api/collections/discord_users/records")
                .match_query(Matcher::UrlEncoded(
                    "filter".into(),
                    "discord_id = '123'".into(),
                ))
                .with_status(200)
                .with_body(
                    serde_json::json!({
                        "page": 1, "perPage": 500, "totalPages": 1, "totalItems": 1,
                        "items": [{"id": actual_user, "discord_id": "123", "username": "Test",
                            "discord_tag": "Test", "joined_at": "2025-01-01"}]
                    })
                    .to_string(),
                )
                .create_async()
                .await;
            let client = PocketbaseClient {
                base_url: server.url(),
                token: Some("test-token".into()),
            };
            let claims = crate::models::JwtClaims {
                sub: "123".into(),
                username: "Test".into(),
                role: crate::models::Role::Normal,
                exp: usize::MAX,
                iat: 0,
            };
            assert_eq!(
                crate::actions::image_access::authorized_with_client(&client, uuid, &claims)
                    .await
                    .unwrap(),
                expected,
            );
            player.assert_async().await;
        }
    }

    async fn mock_count(server: &mut Server, collection: &str, total: u32) -> mockito::Mock {
        server
            .mock(
                "GET",
                format!("/api/collections/{}/records", collection).as_str(),
            )
            .match_header("authorization", "test-token")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("perPage".into(), "1".into()),
                Matcher::UrlEncoded("fields".into(), "id".into()),
                Matcher::UrlEncoded("skipTotal".into(), "false".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "page": 1, "perPage": 1, "totalPages": total,
                    "totalItems": total, "items": []
                })
                .to_string(),
            )
            .create_async()
            .await
    }

    #[tokio::test]
    async fn investor_stats_use_database_totals_including_empty_collections() {
        let mut server = Server::new_async().await;
        let users = mock_count(&mut server, "discord_users", 1234).await;
        let players = mock_count(&mut server, "players", 17).await;
        let servers = mock_count(&mut server, "servers", 0).await;
        let badges = mock_count(&mut server, "badges_earned", 51).await;
        let client = PocketbaseClient {
            base_url: server.url(),
            token: Some("test-token".into()),
        };

        let stats = fetch_stats(&client).await.unwrap();
        assert_eq!(stats.discord_users, 1234);
        assert_eq!(stats.minecraft_players, 17);
        assert_eq!(stats.minecraft_servers, 0);
        assert_eq!(stats.badges_awarded, 51);
        assert!(chrono::DateTime::parse_from_rfc3339(&stats.fetched_at).is_ok());
        users.assert_async().await;
        players.assert_async().await;
        servers.assert_async().await;
        badges.assert_async().await;
    }

    #[tokio::test]
    async fn investor_stats_fail_when_a_collection_is_unavailable() {
        let mut server = Server::new_async().await;
        let _users = mock_count(&mut server, "discord_users", 3).await;
        let _players = mock_count(&mut server, "players", 2).await;
        let _servers = mock_count(&mut server, "servers", 1).await;
        let failure = server
            .mock("GET", "/api/collections/badges_earned/records")
            .match_query(Matcher::Any)
            .with_status(503)
            .create_async()
            .await;
        let client = PocketbaseClient {
            base_url: server.url(),
            token: Some("test-token".into()),
        };

        assert!(fetch_stats(&client).await.is_err());
        failure.assert_async().await;
    }

    #[tokio::test]
    async fn count_rejects_missing_total_instead_of_reporting_zero() {
        let mut server = Server::new_async().await;
        let invalid = server
            .mock("GET", "/api/collections/players/records")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_body(r#"{"page":1,"perPage":1,"totalPages":0,"items":[]}"#)
            .create_async()
            .await;
        let client = PocketbaseClient {
            base_url: server.url(),
            token: Some("test-token".into()),
        };

        assert!(client.count_records("players").await.is_err());
        invalid.assert_async().await;
    }
}

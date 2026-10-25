//! Utility for interacting with Pocketbase.
//!
//! Provides a client to authenticate and fetch records from Pocketbase collections.

use crate::config::Config;
use serde::Deserialize;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tracing::{debug, error};

/// Preserve transport causes (DNS, TLS, connection, timeout) without logging query data.
fn transport_error(context: &str, error: reqwest::Error) -> String {
    let kind = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connection"
    } else {
        "transport"
    };
    let error = error.without_url();
    let mut message = format!("{context} [{kind}]: {error}");
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        message.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    message
}

/// Quotes a value as a PocketBase filter string literal, as the official SDKs do.
///
/// Every `'` in the value is escaped, so none can close the literal early; a value
/// ending in `\` leaves the literal unterminated and PocketBase rejects the filter.
/// Callers still validate identifiers: this is the last line against filter injection.
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "\\'"))
}

/// Superuser tokens are reused well within PocketBase's default lifetime.
const TOKEN_TTL: Duration = Duration::from_secs(30 * 60);

struct CachedToken {
    base_url: String,
    token: String,
    obtained: Instant,
}

fn token_cache() -> &'static Mutex<Option<CachedToken>> {
    static CACHE: OnceLock<Mutex<Option<CachedToken>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn cached_token(base_url: &str) -> Option<String> {
    let cache = token_cache().lock().ok()?;
    cache
        .as_ref()
        .filter(|c| c.base_url == base_url && c.obtained.elapsed() < TOKEN_TTL)
        .map(|c| c.token.clone())
}

fn store_token(base_url: &str, token: &str) {
    if let Ok(mut cache) = token_cache().lock() {
        *cache = Some(CachedToken {
            base_url: base_url.to_owned(),
            token: token.to_owned(),
            obtained: Instant::now(),
        });
    }
}

fn forget_token(token: &str) {
    if let Ok(mut cache) = token_cache().lock()
        && cache.as_ref().is_some_and(|c| c.token == token)
    {
        *cache = None;
    }
}

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
    /// Builds a client with a fixed token against an arbitrary base URL (tests only).
    #[cfg(test)]
    pub fn for_test(base_url: String, token: &str) -> Self {
        Self {
            base_url,
            token: Some(token.to_string()),
        }
    }

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
        let response = crate::utils::http::shared()
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
            self.forget_rejected_token(response.status());
            return Err(format!("PocketBase create failed: {}", response.status()));
        }
        response.json().await.map_err(|e| e.to_string())
    }

    /// Partially updates a record using the API's authenticated PocketBase identity.
    pub async fn update_record(
        &self,
        collection: &str,
        id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let token = self
            .token
            .as_ref()
            .ok_or("Pocketbase client not authenticated")?;
        let response = crate::utils::http::shared()
            .patch(format!(
                "{}/api/collections/{}/records/{}",
                self.base_url, collection, id
            ))
            .header("Authorization", token)
            .timeout(std::time::Duration::from_secs(10))
            .json(body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            self.forget_rejected_token(response.status());
            return Err(format!("PocketBase update failed: {}", response.status()));
        }
        response.json().await.map_err(|e| e.to_string())
    }

    /// Partially updates a record with text fields and PNG file fields (multipart).
    pub async fn update_record_multipart(
        &self,
        collection: &str,
        id: &str,
        fields: &[(&str, String)],
        files: Vec<(&str, &str, Vec<u8>)>,
    ) -> Result<serde_json::Value, String> {
        let token = self
            .token
            .as_ref()
            .ok_or("Pocketbase client not authenticated")?;
        let mut form = reqwest::multipart::Form::new();
        for (name, value) in fields {
            form = form.text(name.to_string(), value.clone());
        }
        for (name, file_name, bytes) in files {
            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(file_name.to_string())
                .mime_str("image/png")
                .map_err(|e| e.to_string())?;
            form = form.part(name.to_string(), part);
        }
        let response = crate::utils::http::shared()
            .patch(format!(
                "{}/api/collections/{}/records/{}",
                self.base_url, collection, id
            ))
            .header("Authorization", token)
            .timeout(std::time::Duration::from_secs(30))
            .multipart(form)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            self.forget_rejected_token(response.status());
            return Err(format!("PocketBase update failed: {}", response.status()));
        }
        response.json().await.map_err(|e| e.to_string())
    }

    /// Deletes a record using the API's authenticated PocketBase identity.
    pub async fn delete_record(&self, collection: &str, id: &str) -> Result<(), String> {
        let token = self
            .token
            .as_ref()
            .ok_or("Pocketbase client not authenticated")?;
        let response = crate::utils::http::shared()
            .delete(format!(
                "{}/api/collections/{}/records/{}",
                self.base_url, collection, id
            ))
            .header("Authorization", token)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            self.forget_rejected_token(response.status());
            return Err(format!("PocketBase delete failed: {}", response.status()));
        }
        Ok(())
    }

    /// A rejected token is dropped from the cache so the next `login` authenticates again.
    fn forget_rejected_token(&self, status: reqwest::StatusCode) {
        if status == reqwest::StatusCode::UNAUTHORIZED
            && let Some(token) = &self.token
        {
            forget_token(token);
        }
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
    /// Reuses a recent superuser token for this PocketBase instance instead of
    /// re-authenticating (a password hash on PocketBase's side) on every request.
    pub async fn login(&mut self) -> Result<(), String> {
        if let Some(token) = cached_token(&self.base_url) {
            self.token = Some(token);
            return Ok(());
        }
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

        let resp = crate::utils::http::shared()
            .post(&url)
            .timeout(std::time::Duration::from_secs(10))
            .json(&serde_json::json!({
                "identity": auth.pb_email,
                "password": auth.pb_password,
            }))
            .send()
            .await
            .map_err(|e| transport_error("Pocketbase login request failed", e))?;

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

        store_token(&self.base_url, &auth_data.token);
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

    /// First record matching `filter`, fetched alone instead of in a 500-row page.
    pub async fn first_record<T>(&self, collection: &str, filter: &str) -> Result<Option<T>, String>
    where
        T: for<'de> Deserialize<'de>,
    {
        let response: ListResponse<T> = self
            .list_records_with_params(
                collection,
                [("filter", filter), ("page", "1"), ("perPage", "1")],
            )
            .await?;
        Ok(response.items.into_iter().next())
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

    /// Bounded history reads for statistics: reject excessive histories, never truncate.
    pub async fn list_bounded_records<T: for<'de> Deserialize<'de>>(
        &self,
        collection: &str,
        filter: &str,
    ) -> Result<Vec<T>, String> {
        let mut items = Vec::new();
        for page in 1..=200u32 {
            let response: ListResponse<T> = self
                .list_records_with_params(
                    collection,
                    [
                        ("filter", filter.to_string()),
                        ("page", page.to_string()),
                        ("perPage", "500".to_string()),
                        ("sort", "id".to_string()),
                    ],
                )
                .await?;
            if response.total_items > 100_000
                || response.total_pages > 200
                || response.items.len() > 500
                || response.page != page
                || items.len() + response.items.len() > 100_000
            {
                return Err("Statistics history exceeds processing limit".into());
            }
            items.extend(response.items);
            if page >= response.total_pages {
                return Ok(items);
            }
        }
        Err("Statistics history exceeds processing limit".into())
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

        let resp = crate::utils::http::shared()
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .header("Authorization", token)
            .query(&params)
            .send()
            .await
            .map_err(|e| transport_error(&format!("Failed to fetch {}", description), e))?;

        // Path only: the query string may carry filters with user identifiers.
        let final_path = resp.url().path().to_string();
        let status = resp.status();

        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to get {} text: {}", description, e))?;

        if !status.is_success() {
            self.forget_rejected_token(status);
            error!(
                "Pocketbase fetch failed for {} at {}: {} - {}",
                description, final_path, status, text
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

    #[tokio::test]
    async fn first_record_fetches_a_single_row() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/collections/discord_users/records")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("filter".into(), "id = 'abc'".into()),
                Matcher::UrlEncoded("perPage".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"page":1,"perPage":1,"totalPages":1,"totalItems":1,"items":[{"id":"abc"}]}"#,
            )
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "token");
        let record: Option<serde_json::Value> = pb
            .first_record("discord_users", &format!("id = {}", quote("abc")))
            .await
            .unwrap();
        assert_eq!(record.unwrap()["id"], "abc");
        mock.assert_async().await;
    }

    #[test]
    fn quoted_values_cannot_close_the_filter_literal() {
        assert_eq!(quote("abc123"), "'abc123'");
        assert_eq!(quote("x' || id != '"), r"'x\' || id != \''");
        assert_eq!(quote(""), "''");
    }
    use crate::actions::investor_actions::fetch_stats;
    use mockito::{Matcher, Server};

    #[tokio::test]
    async fn discord_refresh_checks_discord_and_all_linked_minecraft_accounts() {
        let mut server = Server::new_async().await;
        let pb = PocketbaseClient {
            base_url: server.url(),
            token: Some("token".into()),
        };
        let page = |items: Vec<serde_json::Value>| {
            serde_json::json!({"page":1,"perPage":500,"totalPages":1,"totalItems":items.len(),"items":items}).to_string()
        };
        let user_record =
            serde_json::json!({"id":"owner", "discord_id":"123", "username":"Loutre"});
        let user = server
            .mock("GET", "/api/collections/discord_users/records")
            .match_query(Matcher::UrlEncoded("filter".into(), "id = 'owner'".into()))
            .with_status(200)
            .with_body(page(vec![user_record.clone()]))
            .expect(2)
            .create_async()
            .await;
        let session_user = server
            .mock("GET", "/api/collections/discord_users/records")
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "discord_id = '123'".into(),
            ))
            .with_status(200)
            .with_body(page(vec![user_record]))
            .create_async()
            .await;
        let uuid = "12345678-1234-1234-1234-1234567890ab";
        let player_records = vec![
            serde_json::json!({"id":"java", "account_id":uuid, "discord_user":"owner", "playername":"JavaLoutre"}),
            serde_json::json!({"id":"bedrock", "account_id":"456", "discord_user":"123", "playername":"BedrockLoutre"}),
        ];
        let players = server
            .mock("GET", "/api/collections/players/records")
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "discord_user = '123' || discord_user = 'owner'".into(),
            ))
            .with_status(200)
            .with_body(page(player_records.clone()))
            .create_async()
            .await;
        let mut player_lookups = Vec::new();
        for player in player_records {
            player_lookups.push(
                server
                    .mock("GET", "/api/collections/players/records")
                    .match_query(Matcher::UrlEncoded(
                        "filter".into(),
                        format!("id = '{}'", player["id"].as_str().unwrap()),
                    ))
                    .with_status(200)
                    .with_body(page(vec![player]))
                    .create_async()
                    .await,
            );
        }
        // Already acquired badges still require all linked accounts to be visited.
        let catalogue: serde_json::Value = config::Config::builder()
            .add_source(config::File::new("badges.yaml", config::FileFormat::Yaml))
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();
        let owned = catalogue["badges"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|b| b["id"].as_str().filter(|id| !id.is_empty()))
            .map(|id| serde_json::json!({"badge":id}))
            .collect::<Vec<_>>();
        let mut ownership_reads = Vec::new();
        for filter in [
            "discord_user = 'owner' || discord_user = '123'".to_owned(),
            format!("player = 'java' || player = '{uuid}'"),
            "player = 'bedrock' || player = '456'".into(),
        ] {
            ownership_reads.push(
                server
                    .mock("GET", "/api/collections/badges_earned/records")
                    .match_query(Matcher::UrlEncoded("filter".into(), filter))
                    .with_status(200)
                    .with_body(page(owned.clone()))
                    .create_async()
                    .await,
            );
        }
        let badges = server
            .mock("GET", "/api/collections/badges/records")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_body(page(vec![]))
            .create_async()
            .await;
        let response = crate::services::badge_service::refresh_discord_account(&pb, "owner")
            .await
            .unwrap();
        assert_eq!(response.accounts_checked, 3);
        assert!(response.failed_accounts.is_empty());
        assert!(response.new_badges.is_empty());
        for mock in [&user, &session_user, &players, &badges]
            .into_iter()
            .chain(player_lookups.iter())
            .chain(ownership_reads.iter())
        {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn self_refresh_resolves_only_session_accounts_and_reports_partial_failures() {
        let mut server = Server::new_async().await;
        let pb = PocketbaseClient {
            base_url: server.url(),
            token: Some("token".into()),
        };
        let page = |items: Vec<serde_json::Value>| {
            serde_json::json!({"page":1,"perPage":500,"totalPages":1,"totalItems":items.len(),"items":items}).to_string()
        };
        let user = server
            .mock("GET", "/api/collections/discord_users/records")
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "discord_id = '123'".into(),
            ))
            .with_status(200)
            .with_body(page(vec![
                serde_json::json!({"id":"own", "username":"Loutre"}),
            ]))
            .create_async()
            .await;
        let players = server
            .mock("GET", "/api/collections/players/records")
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "discord_user = '123' || discord_user = 'own'".into(),
            ))
            .with_status(200)
            .with_body(page(vec![
                serde_json::json!({"id":"ownplayer", "playername":"MineLoutre"}),
            ]))
            .create_async()
            .await;
        let badges = server
            .mock("GET", "/api/collections/badges/records")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_body(page(vec![]))
            .create_async()
            .await;
        let failed_user = server
            .mock("GET", "/api/collections/discord_users/records")
            .match_query(Matcher::UrlEncoded("filter".into(), "id = 'own'".into()))
            .with_status(503)
            .create_async()
            .await;
        let failed_player = server
            .mock("GET", "/api/collections/players/records")
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "id = 'ownplayer'".into(),
            ))
            .with_status(503)
            .create_async()
            .await;
        let response = crate::services::badge_service::refresh_mine(&pb, "123")
            .await
            .unwrap();
        assert_eq!(response.accounts_checked, 0);
        assert_eq!(response.failed_accounts, vec!["Loutre", "MineLoutre"]);
        assert!(response.new_badges.is_empty());
        assert!(
            crate::services::badge_service::refresh_mine(&pb, "bad'")
                .await
                .is_err()
        );
        for mock in [&user, &players, &badges, &failed_user, &failed_player] {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn badge_refresh_checks_account_ownership_before_awarding() {
        use crate::models::{JwtClaims, Role};
        use crate::services::badge_service::{RecipientKind, can_refresh};
        for (kind, account, expected, resolve_relation) in [
            (
                RecipientKind::DiscordUser,
                serde_json::json!({"discord_id":"123"}),
                true,
                false,
            ),
            (
                RecipientKind::DiscordUser,
                serde_json::json!({"discord_id":"456"}),
                false,
                false,
            ),
            (
                RecipientKind::Player,
                serde_json::json!({"discord_user":"123"}),
                true,
                false,
            ),
            (
                RecipientKind::Player,
                serde_json::json!({"discord_user":"linked"}),
                true,
                true,
            ),
            (
                RecipientKind::Player,
                serde_json::json!({"discord_user":"other"}),
                false,
                true,
            ),
            (
                RecipientKind::Player,
                serde_json::json!({"discord_user":""}),
                false,
                false,
            ),
        ] {
            let mut server = Server::new_async().await;
            let pb = PocketbaseClient {
                base_url: server.url(),
                token: Some("token".into()),
            };
            let collection = if kind == RecipientKind::Player {
                "players"
            } else {
                "discord_users"
            };
            let lookup = server.mock("GET", format!("/api/collections/{collection}/records").as_str())
                .match_query(Matcher::UrlEncoded("filter".into(), "id = 'account'".into()))
                .with_status(200).with_body(serde_json::json!({"page":1,"perPage":500,"totalPages":1,"totalItems":1,"items":[account]}).to_string()).create_async().await;
            let relation = server.mock("GET", "/api/collections/discord_users/records")
                .match_query(Matcher::UrlEncoded("filter".into(), "discord_id = '123'".into()))
                .with_status(200).with_body(serde_json::json!({"page":1,"perPage":500,"totalPages":1,"totalItems":1,"items":[{"id":"linked"}]}).to_string())
                .expect(usize::from(resolve_relation)).create_async().await;
            let mut claims = JwtClaims {
                sub: "123".into(),
                username: "Test".into(),
                role: Role::Normal,
                exp: usize::MAX,
                iat: 0,
                profile: None,
            };
            assert_eq!(
                can_refresh(&pb, "account", kind, &claims).await.unwrap(),
                expected
            );
            claims.role = Role::Admin;
            assert!(can_refresh(&pb, "account", kind, &claims).await.unwrap());
            assert!(!can_refresh(&pb, "invalid'", kind, &claims).await.unwrap());
            lookup.assert_async().await;
            relation.assert_async().await;
        }
    }

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
            .expect(1).create_async().await;
        let mut reads = Vec::new();
        let mut writes = Vec::new();
        let expected = [0, 1, 5, 6, 10];
        let owned = server
            .mock("GET", "/api/collections/badges_earned/records")
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "discord_user = 'user' || discord_user = '123'".into(),
            ))
            .with_status(200)
            .with_body(page(vec![]))
            .create_async()
            .await;
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
                    .match_body(Matcher::AllOf(vec![
                        Matcher::PartialJson(
                            serde_json::json!({"discord_user":"user", "badge":format!("badge{i}")}),
                        ),
                        Matcher::Regex(
                            r#""date_received":"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}Z""#
                                .into(),
                        ),
                    ]))
                    .with_status(200)
                    .with_header("content-type", "application/json")
                    .with_body(r#"{"id":"earned"}"#)
                    .create_async()
                    .await,
            );
        }
        let result = crate::services::badge_service::evaluate_discord_fixture(&pb, "user")
            .await
            .unwrap();
        assert_eq!(result.len(), expected.len());
        assert!(result.iter().all(|r| r.created));
        badges.assert_async().await;
        badges.remove_async().await;
        owned.assert_async().await;
        owned.remove_async().await;
        for read in &reads {
            read.assert_async().await;
            read.remove_async().await;
        }
        reads.clear();
        // Already owned badges can even be absent from the catalogue without blocking refresh.
        let badges = server.mock("GET", "/api/collections/badges/records")
            .match_query(Matcher::Any).with_status(200)
            .with_body(page(names.iter().enumerate().filter(|(i, _)| !expected.contains(i)).map(|(i, name)| serde_json::json!({"id":format!("badge{i}"), "name":name, "is_enabled":true})).collect()))
            .create_async().await;
        let owned = server
            .mock("GET", "/api/collections/badges_earned/records")
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "discord_user = 'user' || discord_user = '123'".into(),
            ))
            .with_status(200)
            .with_body(page(
                expected
                    .iter()
                    .map(|i| serde_json::json!({"badge":format!("badge{i}")}))
                    .collect(),
            ))
            .create_async()
            .await;
        let result = crate::services::badge_service::evaluate_discord_fixture(&pb, "user")
            .await
            .unwrap();
        assert_eq!(result.len(), expected.len());
        assert!(result.iter().all(|r| !r.created));
        owned.assert_async().await;
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
                profile: None,
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

#[cfg(test)]
mod bounded_statistics_tests {
    #[tokio::test]
    async fn incorrect_page_metadata_is_rejected_instead_of_duplicating_records() {
        let mut server = mockito::Server::new_async().await;
        let client = super::PocketbaseClient {
            base_url: server.url(),
            token: Some("test-token".into()),
        };
        let invalid = server
            .mock("GET", "/api/collections/discord_user_stats/records")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_body(
                r#"{"page":2,"perPage":500,"totalPages":2,"totalItems":2,"items":[{"id":"a"}]}"#,
            )
            .create_async()
            .await;
        assert!(
            client
                .list_bounded_records::<serde_json::Value>("discord_user_stats", "")
                .await
                .is_err()
        );
        invalid.assert_async().await;
    }
    use super::*;
    use mockito::{Matcher, Server};
    #[tokio::test]
    async fn reads_all_pages_with_filter_and_authentication_and_rejects_oversize() {
        let mut server = Server::new_async().await;
        let client = PocketbaseClient {
            base_url: server.url(),
            token: Some("stats-token".into()),
        };
        let filter = "discord_user = 'owner'";
        let mut mocks = Vec::new();
        for page in 1..=2 {
            mocks.push(server.mock("GET", "/api/collections/discord_user_stats/records")
                .match_header("authorization", "stats-token")
                .match_query(Matcher::AllOf(vec![Matcher::UrlEncoded("filter".into(), filter.into()), Matcher::UrlEncoded("page".into(), page.to_string()), Matcher::UrlEncoded("sort".into(), "id".into())]))
                .with_status(200).with_body(serde_json::json!({"page":page,"perPage":500,"totalPages":2,"totalItems":2,"items":[{"id":page}]}).to_string()).create_async().await);
        }
        let rows: Vec<serde_json::Value> = client
            .list_bounded_records("discord_user_stats", filter)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        for mock in &mut mocks {
            mock.assert_async().await;
            mock.remove_async().await;
        }
        let excessive = server
            .mock("GET", "/api/collections/discord_user_stats/records")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_body(
                r#"{"page":1,"perPage":500,"totalPages":201,"totalItems":100001,"items":[]}"#,
            )
            .create_async()
            .await;
        assert!(
            client
                .list_bounded_records::<serde_json::Value>("discord_user_stats", filter)
                .await
                .is_err()
        );
        excessive.assert_async().await;
    }

    #[tokio::test]
    async fn update_record_multipart_sends_files_and_fields() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("PATCH", "/api/collections/players/records/p1")
            .match_header(
                "content-type",
                Matcher::Regex("^multipart/form-data".into()),
            )
            .match_body(Matcher::AllOf(vec![
                Matcher::Regex(r#"name="skin_hash""#.into()),
                Matcher::Regex("abc:classic".into()),
                Matcher::Regex(r#"name="head"; filename="head.png""#.into()),
                Matcher::Regex(r#"name="skin"; filename="skin.png""#.into()),
            ]))
            .with_status(200)
            .with_body(r#"{"id":"p1"}"#)
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "token");

        let result = pb
            .update_record_multipart(
                "players",
                "p1",
                &[("skin_hash", "abc:classic".to_string())],
                vec![
                    ("head", "head.png", vec![1, 2, 3]),
                    ("skin", "skin.png", vec![4, 5, 6]),
                ],
            )
            .await
            .unwrap();

        assert_eq!(result["id"], "p1");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn update_record_multipart_reports_failure() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("PATCH", "/api/collections/players/records/p1")
            .with_status(400)
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "token");

        let error = pb
            .update_record_multipart("players", "p1", &[], vec![("head", "head.png", vec![1])])
            .await
            .unwrap_err();

        assert!(error.contains("400"), "{error}");
    }
}

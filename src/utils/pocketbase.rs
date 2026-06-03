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
        let url = format!(
            "{}/api/collections/_superusers/auth-with-password",
            self.base_url
        );

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
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

    /// Lists ALL records from a collection by automatically paginating.
    pub async fn list_all_records<T>(&self, collection: &str, filter: &str) -> Result<Vec<T>, String>
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

            let response: ListResponse<T> = self.list_records_with_params(collection, params).await?;

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

        debug!("Raw {} response: {}", description, text);

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
            error!(
                "Failed to parse {} JSON: {}. Raw body: {}",
                description, e, text
            );
            format!("Failed to parse {}: {}", description, e)
        })?;

        Ok(data)
    }
}

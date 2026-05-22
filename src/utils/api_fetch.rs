//! Utility for fetching data from external APIs.
//!
//! Provides a generic fetcher that handles health checks, HTTP requests,
//! and standard JSON wrapper unwrapping.

use serde::Deserialize;
use tracing::{debug, error};

/// Standard API response wrapper used by external Otterly APIs.
#[derive(Deserialize)]
pub struct ApiResponse<T> {
    /// The actual data contained in the response.
    pub data: T,
}

/// Generic helper to fetch data from an external API and extract the 'data' field.
///
/// This function automatically performs a health check before the actual request.
///
/// # Arguments
///
/// * `url` - The full URL to fetch.
/// * `description` - A human-readable description of the data (used for logging).
///
/// # Errors
///
/// Returns an error if the health check fails (as a warning), the request fails,
/// or the response body cannot be parsed into the expected type `T`.
pub async fn fetch_api_data<T>(url: &str, description: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    debug!("Fetching {}: {}", description, url);
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch {}: {}", description, e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "External API returned error {} for {}",
            resp.status(),
            description
        ));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to get {} text: {}", description, e))?;

    debug!("Raw {} response: {}", description, text);

    let wrapper: ApiResponse<T> = serde_json::from_str(&text).map_err(|e| {
        error!(
            "Failed to parse {} JSON: {}. Raw body: {}",
            description, e, text
        );
        format!("Failed to parse {}: {}", description, e)
    })?;

    Ok(wrapper.data)
}

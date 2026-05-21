use serde::Deserialize;
use tracing::{debug, error, warn};
use crate::utils::api_endpoints::api_base_url;

#[derive(Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

/// Check if the API base URL is responsive
pub async fn check_api_health() -> Result<(), String> {
    let client = reqwest::Client::new();
    let base_url = api_base_url();
    let resp = client.get(base_url).send().await
        .map_err(|e| format!("API Health Check failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("API Health Check returned error {}", resp.status()));
    }
    
    debug!("API Health Check successful for {}", base_url);
    Ok(())
}

/// Generic helper to fetch data from an external API and extract the 'data' field
pub async fn fetch_api_data<T>(url: &str, description: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    // Check health before proceeding
    if let Err(e) = check_api_health().await {
        warn!("API health check failed: {}. Attempting fetch anyway.", e);
    }

    debug!("Fetching {}: {}", description, url);
    let client = reqwest::Client::new();
    let resp = client.get(url).send().await
        .map_err(|e| format!("Failed to fetch {}: {}", description, e))?;

    if !resp.status().is_success() {
        return Err(format!("External API returned error {} for {}", resp.status(), description));
    }

    let text = resp.text().await
        .map_err(|e| format!("Failed to get {} text: {}", description, e))?;

    debug!("Raw {} response: {}", description, text);

    let wrapper: ApiResponse<T> = serde_json::from_str(&text)
        .map_err(|e| {
            error!("Failed to parse {} JSON: {}. Raw body: {}", description, e, text);
            format!("Failed to parse {}: {}", description, e)
        })?;
    
    Ok(wrapper.data)
}

use axum::Json;
use serde_json::{Value, json};

/// Investor endpoint to retrieve user statistics.
/// Requires `LoutreInvesti` or `Admin` role via `require_investor` middleware.
#[utoipa::path(
    get,
    path = "/investor/stats",
    tag = "investor",
    responses(
        (status = 200, description = "Mock statistics for investors"),
        (status = 403, description = "Forbidden")
    )
)]
pub async fn get_stats() -> Json<Value> {
    // Temporary mock stats
    Json(json!({
        "images_generated_today": 124,
        "total_active_users": 42,
        "server_uptime_days": 15,
        "performance_score": 98
    }))
}

use crate::actions::investor_actions;
use crate::models::investor::InvestorStats;
use axum::{
    Json,
    http::{StatusCode, header},
    response::IntoResponse,
};
use std::time::Duration;

/// Investor endpoint to retrieve user statistics.
/// Requires `LoutreInvesti` or `Admin` role via `require_investor` middleware.
#[utoipa::path(
    get,
    path = "/investor/stats",
    tag = "investor",
    responses(
        (status = 200, description = "Current PocketBase record counts", body = InvestorStats),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
        (status = 503, description = "Statistics temporarily unavailable")
    )
)]
pub async fn get_stats() -> impl IntoResponse {
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        investor_actions::get_investor_stats(),
    )
    .await
    .unwrap_or_else(|_| Err("PocketBase statistics timed out".to_string()))
    .map(Json)
    .map_err(|error| {
        tracing::error!("Unable to retrieve investor statistics: {}", error);
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Statistiques temporairement indisponibles.",
        )
    });
    ([(header::CACHE_CONTROL, "no-store")], result)
}

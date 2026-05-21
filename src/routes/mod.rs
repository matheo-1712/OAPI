//! Route definitions for the OAPI server.
//! 
//! This module aggregates all available API endpoints and sets up the 
//! internal routing table using configuration values.

use axum::{routing::post, Router};
use crate::handlers::discord_handler;
use crate::config::Config;

/// Aggregates all internal API routes into a single Router.
/// 
/// The paths used here are retrieved from the global configuration.
pub fn api_routes() -> Router {
    let routes = &Config::global().server.routes;
    Router::new()
        .route(&routes.discord_summary, post(discord_handler::create_discord_summary_by_id))
}

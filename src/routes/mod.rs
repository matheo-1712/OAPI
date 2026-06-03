//! Route definitions for the OAPI server.
//!
//! This module aggregates all available API endpoints and sets up the
//! internal routing table using configuration values.

use crate::config::Config;
use crate::handlers::{discord_handler, minecraft_handler, monitoring_handler};
use axum::{
    Router,
    routing::{get, post},
};

/// Aggregates all internal API routes into a single Router.
///
/// The paths used here are retrieved from the global configuration.
pub fn api_routes() -> Router {
    let routes = &Config::global().server.routes;
    Router::new()
        .route(
            &routes.discord_summary,
            post(discord_handler::create_discord_summary_by_id),
        )
        .route(
            &routes.minecraft_summary,
            post(minecraft_handler::create_minecraft_summary_by_id),
        )
        .route(
            &routes.monitoring,
            get(monitoring_handler::get_monitoring_status),
        )
        .route(
            &format!("{}/check/:type/:name", routes.monitoring),
            get(monitoring_handler::check_single_service),
        )
}

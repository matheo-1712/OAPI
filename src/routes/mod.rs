use axum::{routing::post, Router};
use crate::handlers::discord_handler;
use crate::config::Config;

pub fn api_routes() -> Router {
    let routes = &Config::global().server.routes;
    Router::new()
        .route(&routes.discord_summary, post(discord_handler::create_discord_summary_by_id))
}

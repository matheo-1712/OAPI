use axum::{routing::post, Router};
use crate::handlers::discord_handler;

pub fn api_routes() -> Router {
    Router::new()
        .route("/discord-summary/:discord_id", post(discord_handler::create_discord_summary_by_id))
}

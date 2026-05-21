use axum::{routing::post, Router};
use crate::handlers::{image_handler, discord_handler};

pub fn api_routes() -> Router {
    Router::new()
        .route("/images", post(image_handler::create_image))
        .route("/discord-summary/:discord_id", post(discord_handler::create_discord_summary_by_id))
}

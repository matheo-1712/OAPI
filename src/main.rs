//! OAPI - Otter API
//!
//! A high-performance Rust API based on Axum, designed to automate profile image
//! generation and statistics summaries for the Otter community.

mod actions;
mod config;
mod handlers;
mod models;
mod routes;
mod services;
mod utils;

use axum::Router;
use std::net::SocketAddr;
use tower_http::{services::ServeDir, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::discord_handler::create_discord_summary_by_id,
        handlers::monitoring_handler::get_monitoring_status,
        handlers::monitoring_handler::check_single_service
    ),
    components(schemas(
        models::ImageRequest,
        models::ImageResponse,
        models::DiscordUser,
        models::DiscordRole,
        models::DiscordStats,
        models::DiscordChannel,
        models::DiscordVoiceConnection,
        models::monitoring::MonitoringResponse,
        models::monitoring::ServiceResult,
        models::monitoring::ServiceStatus,
        models::monitoring::ServiceMetadata,
        models::monitoring::DiscordBotMetadata,
        models::monitoring::DiscordBotUptime,
        models::monitoring::DiscordBotDiscordData,
        models::monitoring::MinecraftMetadata
    ))
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    // Initialize config
    config::init();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,OAPI=debug,tower_http=debug".into()),
        )
        .compact()
        .init();

    let banner = r#"
  ____               ____    ___ 
 / __ \     /\      |  _ \  |_ _|
| |  | |   /  \     | |_) |  | | 
| |  | |  / /\ \    |  __/   | | 
| |__| | / ____ \   | |     _| |_
 \____/ /_/    \_\  |_|    |_____|
                                  
    "#;
    println!("{}", banner);

    // Build app
    let config = config::Config::global();

    // Modify OpenAPI spec at runtime to match configured routes
    let mut openapi = ApiDoc::openapi();
    update_openapi_paths(&mut openapi, config);

    let app = Router::new()
        .nest(&config.server.routes.base, routes::api_routes())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi))
        .fallback_service(ServeDir::new("public"))
        .layer(TraceLayer::new_for_http());

    let addr_str = format!("{}:{}", config.server.host, config.server.port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid address/port in config");

    tracing::info!("🚀 OAPI Server starting up...");
    tracing::info!("📡 Listening on http://{}", addr);
    tracing::info!("📖 Swagger UI available at http://{}/swagger-ui", addr);
    tracing::info!("🌐 Web interface available at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Update OpenAPI paths to match dynamic configuration
fn update_openapi_paths(openapi: &mut utoipa::openapi::OpenApi, config: &config::Config) {
    let mut new_paths = utoipa::openapi::path::PathsBuilder::new();
    let base = &config.server.routes.base;

    // For each path in the original spec, try to find a matching config and remap it
    for (path, item) in openapi.paths.paths.iter() {
        let new_path = if path.contains("discord-summary") {
            // Convert Axum :param to OpenAPI {param}
            let dynamic_path = config.server.routes.discord_summary.replace(":", "{");

            // If the parameter in YAML doesn't end with }, add it
            let final_dynamic = if dynamic_path.contains('{') && !dynamic_path.ends_with('}') {
                format!("{}}}", dynamic_path)
            } else {
                dynamic_path
            };

            format!("{}{}", base, final_dynamic)
        } else if path.starts_with("/monitoring/check") {
            format!(
                "{}{}/check/{{type}}/{{name}}",
                base, config.server.routes.monitoring
            )
        } else if path == "/monitoring" {
            format!("{}{}", base, config.server.routes.monitoring)
        } else {
            // Default: just prefix with base if it doesn't already have it and starts with /
            if path.starts_with('/') && !path.starts_with(base) {
                format!("{}{}", base, path)
            } else {
                path.clone()
            }
        };

        new_paths = new_paths.path(new_path, item.clone());
    }

    openapi.paths = new_paths.build();
}

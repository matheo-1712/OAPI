mod handlers;
mod models;
mod services;
mod routes;
mod actions;
mod utils;
mod config;

use axum::Router;
use std::net::SocketAddr;
use tower_http::{services::ServeDir, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::discord_handler::create_discord_summary_by_id
    ),
    components(schemas(
        models::ImageRequest, 
        models::ImageResponse, 
        models::DiscordUser, 
        models::DiscordRole,
        models::DiscordStats,
        models::DiscordChannel,
        models::DiscordVoiceConnection
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
    let app = Router::new()
        .nest(&config.server.routes.base, routes::api_routes())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
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

mod handlers;
mod models;

use axum::{
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tower_http::{services::ServeDir, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(handlers::get_items, handlers::create_item),
    components(schemas(models::Item, models::CreateItem))
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    // Initialize tracing with a simplified and compact format
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,OAPI=debug,tower_http=debug".into()),
        )
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new("%Y-%m-%d %H:%M:%S".to_string()))
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

    // Build our application with routes
    let app = Router::new()
        // API routes
        .route("/api/items", get(handlers::get_items).post(handlers::create_item))
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Serve static files from the "public" directory
        .fallback_service(ServeDir::new("public"))
        // Add logging middleware
        .layer(TraceLayer::new_for_http());

    // Run it with hyper
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    
    tracing::info!("🚀 OAPI Server starting up...");
    tracing::info!("📡 Listening on http://{}", addr);
    tracing::info!("📖 Swagger UI available at http://{}/swagger-ui", addr);
    tracing::info!("🌐 Web interface available at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

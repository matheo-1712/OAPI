mod handlers;
mod models;

use axum::{
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tower_http::services::ServeDir;
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
    // Build our application with routes
    let app = Router::new()
        // API routes
        .route("/api/items", get(handlers::get_items).post(handlers::create_item))
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Serve static files from the "public" directory
        .fallback_service(ServeDir::new("public"));

    // Run it with hyper
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("listening on http://{}", addr);
    println!("Swagger UI available at http://{}/swagger-ui", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

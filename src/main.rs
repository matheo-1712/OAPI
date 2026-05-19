use axum::{
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Serialize, Deserialize, ToSchema, Clone)]
struct Item {
    id: i32,
    name: String,
    description: Option<String>,
}

#[derive(OpenApi)]
#[openapi(
    paths(get_items),
    components(schemas(Item))
)]
struct ApiDoc;

/// Get a list of items
#[utoipa::path(
    get,
    path = "/api/items",
    responses(
        (status = 200, description = "List all items", body = [Item])
    )
)]
async fn get_items() -> Json<Vec<Item>> {
    let items = vec![
        Item {
            id: 1,
            name: "First Item".to_string(),
            description: Some("This is the first item".to_string()),
        },
        Item {
            id: 2,
            name: "Second Item".to_string(),
            description: None,
        },
    ];
    Json(items)
}

#[tokio::main]
async fn main() {
    // Build our application with a single route
    let app = Router::new()
        // API routes
        .route("/api/items", get(get_items))
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

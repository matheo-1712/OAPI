use axum::{Json, http::StatusCode};
use crate::models::{Item, CreateItem};
use tracing::{info, debug};

/// Get a list of items
#[utoipa::path(
    get,
    path = "/api/items",
    responses(
        (status = 200, description = "List all items", body = [Item])
    )
)]
pub async fn get_items() -> Json<Vec<Item>> {
    info!("Fetching all items");
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
    debug!("Returning {} items", items.len());
    Json(items)
}

/// Create a new item
#[utoipa::path(
    post,
    path = "/api/items",
    request_body = CreateItem,
    responses(
        (status = 201, description = "Item created successfully", body = Item)
    )
)]
pub async fn create_item(Json(payload): Json<CreateItem>) -> (StatusCode, Json<Item>) {
    info!("Creating a new item: {}", payload.name);
    let item = Item {
        id: 3,
        name: payload.name,
        description: payload.description,
    };
    debug!("Item created with id: {}", item.id);
    (StatusCode::CREATED, Json(item))
}

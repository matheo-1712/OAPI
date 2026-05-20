use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct Item {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct CreateItem {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct ImageRequest {
    pub prompt: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct ImageResponse {
    pub id: String,
    pub url: String,
    pub status: String,
}

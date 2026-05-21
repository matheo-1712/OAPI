use axum::Json;
use crate::models::{ImageRequest, ImageResponse};
use crate::services;
use tracing::info;

/// Generate a mock image and return its URL
#[utoipa::path(
    post,
    path = "/api/images",
    request_body = ImageRequest,
    responses(
        (status = 200, description = "Mock image generated successfully", body = ImageResponse)
    )
)]
pub async fn create_image(Json(payload): Json<ImageRequest>) -> Json<ImageResponse> {
    info!("Requesting image generation for: {}", payload.prompt);
    let response = services::generate_image_mock(payload);
    Json(response)
}

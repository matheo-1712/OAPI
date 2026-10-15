use crate::models::{JwtClaims, Role};
use crate::services::image_service::{valid_discord_id, valid_uuid};
use crate::utils::paths;
use axum::{
    Extension,
    body::Body,
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

pub async fn get_image(
    Path((kind, owner, filename)): Path<(String, String, String)>,
    Extension(claims): Extension<JwtClaims>,
) -> Response {
    let Some(hash) = filename.strip_suffix(".png") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let root = match kind.as_str() {
        "discord_summary" if valid_discord_id(&owner) => {
            if claims.role != Role::Admin && claims.sub != owner {
                return StatusCode::FORBIDDEN.into_response();
            }
            paths::DISCORD_SUMMARY_DIR
        }
        "minecraft_summary" if valid_uuid(&owner) => {
            match crate::actions::image_access::can_access_minecraft(&owner, &claims).await {
                Ok(true) => {}
                Ok(false) => return StatusCode::FORBIDDEN.into_response(),
                Err(error) => {
                    tracing::error!("Unable to authorize image download: {}", error);
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
            }
            paths::MINECRAFT_SUMMARY_DIR
        }
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let path = std::path::Path::new(root).join(owner).join(filename);
    match tokio::fs::read(path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "image/png")], Body::from(bytes)).into_response(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => {
            tracing::error!("Unable to read generated image: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn another_users_image_is_forbidden_even_with_a_valid_session() {
        let claims = JwtClaims {
            sub: "123".into(),
            username: "test".into(),
            role: Role::Normal,
            exp: usize::MAX,
            iat: 0,
        };
        let response = get_image(
            Path((
                "discord_summary".into(),
                "456".into(),
                format!("{}.png", "a".repeat(64)),
            )),
            Extension(claims),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn invalid_image_paths_are_never_read() {
        let claims = JwtClaims {
            sub: "123".into(),
            username: "test".into(),
            role: Role::Admin,
            exp: usize::MAX,
            iat: 0,
        };
        for (kind, owner, file) in [
            ("discord_summary", "..", format!("{}.png", "a".repeat(64))),
            ("discord_summary", "123", "../secret.png".into()),
            ("unknown", "123", format!("{}.png", "a".repeat(64))),
        ] {
            let response = get_image(
                Path((kind.into(), owner.into(), file)),
                Extension(claims.clone()),
            )
            .await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }
}

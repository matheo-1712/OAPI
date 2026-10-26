//! `POST /saves/mkds/link`: returns a copy of the uploaded Mario Kart DS save
//! linked to the session's DS Games player.

use crate::actions::adl_onboarding::{self, DiscordIdentity};
use crate::actions::save_link_actions::{self, LinkError, LinkOutcome};
use crate::actions::save_link_journal::LinkJournal;
use crate::handlers::middleware::{SERVICE_SUBJECT, trusted_origin};
use crate::models::JwtClaims;
use crate::services::adln_device::WrappingKey;
use crate::services::mkds_save::{SAVE_LEN, SaveError};
use crate::utils::pocketbase::PocketbaseClient;
use axum::{
    Extension,
    extract::Multipart,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use tracing::error;

/// Request body limit: one save plus multipart framing.
pub const MAX_UPLOAD_BYTES: usize = SAVE_LEN + 16 * 1024;
const LINK_HEADER: &str = "x-adln-link";

type Failure = (StatusCode, &'static str);

fn unavailable(context: &str, error: impl std::fmt::Display) -> Failure {
    error!("Save link failed ({context}): {error}");
    (
        StatusCode::BAD_GATEWAY,
        "Liaison de sauvegarde temporairement indisponible. Réessayez avec le même fichier.",
    )
}

fn failure(error: LinkError) -> Failure {
    match error {
        LinkError::Save(SaveError::InvalidSave) => (
            StatusCode::BAD_REQUEST,
            "Fichier refusé : envoyez la sauvegarde brute de Mario Kart DS (.sav de 262 144 octets).",
        ),
        LinkError::Save(SaveError::UnsupportedCredential) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "La zone réservée de cette sauvegarde contient des données inconnues.",
        ),
        LinkError::Save(SaveError::DamagedCopies) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Les deux copies de liaison de cette sauvegarde sont endommagées.",
        ),
        LinkError::Save(SaveError::ConflictingCopies) => (
            StatusCode::CONFLICT,
            "Les deux copies de liaison de cette sauvegarde se contredisent.",
        ),
        LinkError::ForeignAccount => (
            StatusCode::FORBIDDEN,
            "Cette sauvegarde est liée à un autre compte.",
        ),
        LinkError::Revoked => (
            StatusCode::CONFLICT,
            "L’appareil lié à cette sauvegarde a été révoqué.",
        ),
        LinkError::Unregistered => (
            StatusCode::CONFLICT,
            "Cette sauvegarde contient un appareil inconnu.",
        ),
        LinkError::Mismatch => (
            StatusCode::CONFLICT,
            "Les identifiants de cette sauvegarde ne correspondent pas à l’appareil enregistré.",
        ),
        LinkError::Unavailable(error) => unavailable("link", error),
    }
}

/// Reads the single `save` field; anything else is refused.
async fn read_upload(mut multipart: Multipart) -> Result<Vec<u8>, Failure> {
    const INVALID: Failure = (
        StatusCode::BAD_REQUEST,
        "Envoyez un seul fichier, dans le champ « save ».",
    );
    let mut upload = None;
    while let Some(mut field) = multipart.next_field().await.map_err(|_| INVALID)? {
        if field.name() != Some("save") || upload.is_some() {
            return Err(INVALID);
        }
        let mut bytes = Vec::with_capacity(SAVE_LEN);
        while let Some(chunk) = field.chunk().await.map_err(|_| INVALID)? {
            if bytes.len() + chunk.len() > SAVE_LEN {
                return Err(failure(LinkError::Save(SaveError::InvalidSave)));
            }
            bytes.extend_from_slice(&chunk);
        }
        upload = Some(bytes);
    }
    upload.ok_or(INVALID)
}

/// Identity from the verified session, never from the request.
fn session_identity(claims: &JwtClaims) -> DiscordIdentity {
    DiscordIdentity {
        discord_id: claims.sub.clone(),
        username: claims.username.clone(),
        discord_tag: claims.username.clone(),
        avatar_url: claims.profile.as_ref().and_then(|p| p.avatar_url.clone()),
    }
}

async fn link(
    claims: &JwtClaims,
    headers: &HeaderMap,
    multipart: Multipart,
) -> Result<Response, Failure> {
    if claims.sub == SERVICE_SUBJECT {
        return Err((
            StatusCode::FORBIDDEN,
            "Le token de service ne peut pas lier de sauvegarde.",
        ));
    }
    // Stricter than the shared CSRF check: a provisioning upload must name its origin.
    let origin = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok());
    if !trusted_origin(origin) {
        return Err((StatusCode::FORBIDDEN, "Cross-origin request rejected."));
    }
    let upload = read_upload(multipart).await?;
    let key = WrappingKey::from_env().map_err(|error| {
        error!("Save link disabled: {error}");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "La liaison de sauvegarde n’est pas configurée sur ce serveur.",
        )
    })?;
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(|e| unavailable("login", e))?;
    let member = adl_onboarding::ensure_member(&pb, &session_identity(claims))
        .await
        .map_err(|e| unavailable("onboarding", e))?;
    let linked =
        save_link_actions::link_save(&pb, &key, &LinkJournal::from_env(), &member, &upload)
            .await
            .map_err(failure)?;
    let outcome = match linked.outcome {
        LinkOutcome::Created => "created",
        LinkOutcome::Reused => "reused",
    };
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            ),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment; filename=\"mario-kart-ds-adl.sav\""),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                HeaderValue::from_static(LINK_HEADER),
            ),
        ],
        [(LINK_HEADER, outcome)],
        linked.bytes,
    )
        .into_response())
}

#[utoipa::path(post, path = "/saves/mkds/link", tag = "ROM",
    request_body(content = Vec<u8>, content_type = "multipart/form-data", description = "Field `save`: raw Mario Kart DS EUR save (262144 bytes, starts with MKDSSV10)"),
    responses(
        (status = 200, description = "Linked copy of the save (application/octet-stream); `X-ADLN-Link: created|reused`"),
        (status = 400, description = "Not a raw MKDS save, or malformed upload"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Cross-origin request, service token, or save linked to another account"),
        (status = 409, description = "Conflicting copies, revoked, unregistered or mismatched device"),
        (status = 422, description = "Unknown data or two damaged copies in the reserved pages"),
        (status = 502, description = "PocketBase or the recovery journal is unavailable; retry with the same file"),
        (status = 503, description = "ADLN_DEVICE_WRAPPING_KEY is not configured")))]
pub async fn link_mkds_save(
    Extension(claims): Extension<JwtClaims>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Response {
    match link(&claims, &headers, multipart).await {
        Ok(response) => response,
        Err(failure) => failure.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::post};
    use tower::ServiceExt;

    const BOUNDARY: &str = "adlboundary";

    fn claims(sub: &str) -> JwtClaims {
        JwtClaims {
            sub: sub.into(),
            username: "cocow".into(),
            role: crate::models::Role::Normal,
            exp: 0,
            iat: 0,
            profile: None,
        }
    }

    fn form(fields: &[(&str, &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (name, bytes) in fields {
            body.extend_from_slice(
                format!(
                    "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"x.sav\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(bytes);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
        body
    }

    async fn send(sub: &str, origin: Option<&str>, body: Vec<u8>) -> (StatusCode, String) {
        crate::config::init();
        let claims = claims(sub);
        let app = Router::new()
            .route("/saves/mkds/link", post(link_mkds_save))
            .layer(axum::extract::DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
            .layer(Extension(claims));
        let mut request = Request::builder()
            .method("POST")
            .uri("/saves/mkds/link")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={BOUNDARY}"),
            );
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = app
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    fn site() -> String {
        crate::config::init();
        crate::config::Config::global()
            .site_origin()
            .expect("site_auth.url must be configured")
            .to_string()
    }

    #[tokio::test]
    async fn provisioning_requires_a_trusted_origin_and_a_personal_session() {
        let save = vec![0u8; SAVE_LEN];
        let body = form(&[("save", &save)]);
        for origin in [None, Some("https://evil.example"), Some("null")] {
            let (status, _) = send("111", origin, body.clone()).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{origin:?}");
        }
        let (status, _) = send(SERVICE_SUBJECT, Some(&site()), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn browser_supplied_identifiers_and_extra_files_are_refused() {
        let save = vec![0u8; SAVE_LEN];
        for fields in [
            vec![
                ("save", save.as_slice()),
                ("player_id", b"0s90t9j1i8zxpwr".as_slice()),
            ],
            vec![("discord_id", b"999".as_slice()), ("save", save.as_slice())],
            vec![("save", save.as_slice()), ("save", save.as_slice())],
            vec![],
        ] {
            let (status, _) = send("111", Some(&site()), form(&fields)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        let oversized = vec![0u8; SAVE_LEN + 1];
        let (status, message) = send("111", Some(&site()), form(&[("save", &oversized)])).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(message.contains("262 144"));
    }

    #[test]
    fn identity_comes_from_the_session_claims_only() {
        let identity = session_identity(&claims("111"));
        assert_eq!(identity.discord_id, "111");
        assert_eq!(identity.discord_tag, "cocow");
    }

    #[test]
    fn error_messages_never_echo_upstream_details() {
        let (status, message) = failure(LinkError::Unavailable("token=abc secret".into()));
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert!(!message.contains("abc") && !message.contains("secret"));
    }
}

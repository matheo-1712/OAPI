use crate::actions::AuthAction;
use crate::models::Role;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use axum_extra::extract::cookie::CookieJar;
use tracing::error;

pub const SERVICE_SUBJECT: &str = "service:astro";

fn service_claims(key: &str, expected: &str) -> Option<crate::models::JwtClaims> {
    use sha2::{Digest, Sha256};
    // Compare fixed-size hashes so comparisons never expose a prefix of the secret.
    if expected.trim().len() < 32
        || key.len() > 4096
        || Sha256::digest(key.as_bytes()) != Sha256::digest(expected.as_bytes())
    {
        return None;
    }
    Some(crate::models::JwtClaims {
        sub: SERVICE_SUBJECT.into(),
        username: "Astro service".into(),
        role: Role::Admin,
        exp: 0,
        iat: 0,
    })
}

/// An explicit Authorization header takes precedence over the browser session.
pub fn request_claims(headers: &axum::http::HeaderMap) -> Option<crate::models::JwtClaims> {
    request_claims_with_key(
        headers,
        &crate::config::Config::global().auth.service_api_key,
    )
}

fn request_claims_with_key(
    headers: &axum::http::HeaderMap,
    service_key: &str,
) -> Option<crate::models::JwtClaims> {
    if headers.contains_key(axum::http::header::AUTHORIZATION) {
        if headers
            .get_all(axum::http::header::AUTHORIZATION)
            .iter()
            .count()
            != 1
        {
            return None;
        }
        let key = headers
            .get(axum::http::header::AUTHORIZATION)?
            .to_str()
            .ok()?
            .strip_prefix("Bearer ")?;
        return service_claims(key, service_key);
    }
    CookieJar::from_headers(headers)
        .get("auth_token")
        .and_then(|cookie| AuthAction::verify_token(cookie.value()))
}

pub async fn private_response(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    response
}

/// Base logic to verify token and optionally check minimum role
async fn verify_and_check_role(
    _jar: CookieJar,
    mut request: Request,
    next: Next,
    required_roles: &[Role],
    service_key: &str,
) -> Result<Response, (StatusCode, String)> {
    let claims = request_claims_with_key(request.headers(), service_key);
    let is_service = claims
        .as_ref()
        .is_some_and(|claims| claims.sub == SERVICE_SUBJECT);
    if !is_service
        && !request.method().is_safe()
        && let Some(origin) = request.headers().get(axum::http::header::ORIGIN)
    {
        let expected =
            reqwest::Url::parse(&crate::config::Config::global().auth.discord_redirect_url)
                .map(|url| url.origin().ascii_serialization())
                .unwrap_or_default();
        if origin.to_str().ok() != Some(expected.as_str()) {
            return Err((
                StatusCode::FORBIDDEN,
                "Cross-origin request rejected.".into(),
            ));
        }
    }
    if let Some(claims) = claims {
        if is_service
            && request
                .uri()
                .path()
                .split('/')
                .any(|segment| segment == "me")
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Service requests must use an explicit account ID instead of /me.".into(),
            ));
        }
        if required_roles.is_empty() || required_roles.contains(&claims.role) {
            request.extensions_mut().insert(claims);
            Ok(next.run(request).await)
        } else {
            error!(
                "Forbidden attempt to access protected route: {} by user with role {:?}",
                request.uri().path(),
                claims.role
            );
            Err((
                StatusCode::FORBIDDEN,
                "Forbidden. Insufficient permissions.".to_string(),
            ))
        }
    } else {
        error!(
            "Unauthorized attempt to access protected route: {}",
            request.uri()
        );
        Err((
            StatusCode::UNAUTHORIZED,
            "Unauthorized. Please log in.".to_string(),
        ))
    }
}

/// Middleware to require ANY valid authentication (Normal, LoutreInvesti, or Admin)
/// Middleware to ensure the request has a valid JWT `auth_token` cookie.
/// Returns 401 Unauthorized if missing or invalid.
pub async fn require_auth(
    jar: CookieJar,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    verify_and_check_role(
        jar,
        request,
        next,
        &[],
        &crate::config::Config::global().auth.service_api_key,
    )
    .await
}

/// Middleware to require LoutreInvesti or Admin accreditation
#[allow(dead_code)]
/// Middleware to ensure the user has at least the `LoutreInvesti` role.
/// Returns 401 if unauthenticated, and 403 Forbidden if not an investor or admin.
pub async fn require_investor(
    jar: CookieJar,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    verify_and_check_role(
        jar,
        request,
        next,
        &[Role::Admin, Role::LoutreInvesti],
        &crate::config::Config::global().auth.service_api_key,
    )
    .await
}

/// Middleware to require strict Admin accreditation
#[allow(dead_code)]
/// Middleware to ensure the user has the `Admin` role strictly.
/// Returns 401 if unauthenticated, and 403 Forbidden if not an admin.
pub async fn require_admin(
    jar: CookieJar,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    verify_and_check_role(
        jar,
        request,
        next,
        &[Role::Admin],
        &crate::config::Config::global().auth.service_api_key,
    )
    .await
}

#[cfg(test)]
mod activity_tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest};
    use tower::ServiceExt;

    const TEST_KEY: &str = "test-service-key-with-at-least-32-bytes";

    #[tokio::test]
    async fn service_passes_role_checks_but_never_impersonates_a_personal_account() {
        for roles in [
            vec![],
            vec![Role::Admin],
            vec![Role::Admin, Role::LoutreInvesti],
        ] {
            let app =
                axum::Router::new()
                    .route(
                        "/admin/action",
                        axum::routing::post(|| async { StatusCode::NO_CONTENT }),
                    )
                    .route(
                        "/badges/me/refresh",
                        axum::routing::post(|| async { StatusCode::NO_CONTENT }),
                    )
                    .layer(axum::middleware::from_fn(
                        move |jar: CookieJar, request: Request, next: Next| {
                            let roles = roles.clone();
                            async move {
                                verify_and_check_role(jar, request, next, &roles, TEST_KEY).await
                            }
                        },
                    ));
            for (path, key, status) in [
                ("/admin/action", TEST_KEY, StatusCode::NO_CONTENT),
                ("/admin/action", "forged", StatusCode::UNAUTHORIZED),
                ("/badges/me/refresh", TEST_KEY, StatusCode::BAD_REQUEST),
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        HttpRequest::builder()
                            .method("POST")
                            .uri(path)
                            .header("authorization", format!("Bearer {key}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), status);
            }
        }
    }

    #[test]
    fn service_credentials_are_opt_in_and_require_an_exact_key() {
        assert!(service_claims(TEST_KEY, "").is_none());
        assert!(service_claims("short", "short").is_none());
        assert!(service_claims("incorrect-key", TEST_KEY).is_none());
        let claims = service_claims(TEST_KEY, TEST_KEY).unwrap();
        assert_eq!(claims.role, Role::Admin);
        assert_eq!(claims.sub, SERVICE_SUBJECT);
    }

    #[test]
    fn authorization_rejects_wrong_schemes_and_duplicate_headers() {
        use axum::http::{HeaderMap, HeaderValue, header::AUTHORIZATION};
        let mut headers = HeaderMap::new();
        for value in [
            "Basic invalid".into(),
            "Bearer invalid".into(),
            format!("Bearer {TEST_KEY}"),
        ] {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&value).unwrap());
            assert_eq!(
                request_claims_with_key(&headers, TEST_KEY).is_some(),
                value == format!("Bearer {TEST_KEY}")
            );
        }
        headers.append(AUTHORIZATION, HeaderValue::from_static("Bearer invalid"));
        assert!(request_claims_with_key(&headers, TEST_KEY).is_none());
    }

    #[tokio::test]
    async fn service_can_access_explicit_accounts_without_a_discord_subject() {
        let claims = service_claims(TEST_KEY, TEST_KEY).unwrap();
        assert!(
            crate::actions::image_access::can_access_minecraft(
                "123456781234123412341234567890ab",
                &claims
            )
            .await
            .unwrap()
        );
        assert!(
            !crate::actions::image_access::can_access_minecraft("../invalid", &claims)
                .await
                .unwrap()
        );
        let pb = crate::utils::pocketbase::PocketbaseClient::new();
        assert!(
            crate::services::badge_service::can_refresh(
                &pb,
                "account123",
                crate::services::badge_service::RecipientKind::Player,
                &claims
            )
            .await
            .unwrap()
        );
        assert!(
            !crate::services::badge_service::can_refresh(
                &pb,
                "../invalid",
                crate::services::badge_service::RecipientKind::Player,
                &claims
            )
            .await
            .unwrap()
        );
    }
    #[tokio::test]
    async fn statistics_routes_reject_missing_and_forged_sessions() {
        crate::config::init();
        for path in ["/discord/stats/me", "/admin/discord/stats"] {
            for cookie in ["", "auth_token=forged"] {
                let response = crate::routes::api_routes()
                    .oneshot(
                        HttpRequest::builder()
                            .uri(path)
                            .header("cookie", cookie)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                assert_eq!(response.headers()["cache-control"], "no-store");
            }
        }
    }
}

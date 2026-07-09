use crate::actions::AuthAction;
use crate::models::Role;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use axum_extra::extract::cookie::CookieJar;
use tracing::error;

/// Base logic to verify token and optionally check minimum role
async fn verify_and_check_role(
    jar: CookieJar,
    mut request: Request,
    next: Next,
    required_roles: &[Role],
) -> Result<Response, (StatusCode, String)> {
    if let Some(claims) = jar
        .get("auth_token")
        .and_then(|c| AuthAction::verify_token(c.value()))
    {
        if required_roles.is_empty() || required_roles.contains(&claims.role) {
            request.extensions_mut().insert(claims);
            Ok(next.run(request).await)
        } else {
            error!(
                "Forbidden attempt to access protected route: {} by user with role {:?}",
                request.uri(),
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
    verify_and_check_role(jar, request, next, &[]).await
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
    verify_and_check_role(jar, request, next, &[Role::Admin, Role::LoutreInvesti]).await
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
    verify_and_check_role(jar, request, next, &[Role::Admin]).await
}

use crate::actions::AuthAction;
use axum::{
    Json,
    extract::Query,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct AuthLoginQuery {
    pub return_to: Option<String>,
}

#[derive(Deserialize)]
pub struct AuthCallbackQuery {
    pub code: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct AuthLogoutQuery {
    pub return_to: Option<String>,
}

/// Ensures the return_to URL is a safe, relative path to prevent Open Redirect vulnerabilities.
fn safe_return_to(url: Option<String>) -> String {
    if let Some(url) = url.filter(|u| u.starts_with('/') && !u.starts_with("//")) {
        return url;
    }
    "/".to_string()
}

#[utoipa::path(
    get,
    path = "/auth/login",
    tag = "auth",
    params(
        ("return_to" = Option<String>, Query, description = "Optional URL to redirect to after successful login")
    ),
    responses(
        (status = 303, description = "Redirects to Discord OAuth2 login page")
    )
)]
pub async fn login(jar: CookieJar, Query(query): Query<AuthLoginQuery>) -> impl IntoResponse {
    let return_to = safe_return_to(query.return_to);

    // Redirect to home if already authenticated
    if jar
        .get("auth_token")
        .and_then(|cookie| AuthAction::verify_token(cookie.value()))
        .is_some()
    {
        return (jar, Redirect::to(&return_to));
    }

    let auth_action = AuthAction::new();
    let url = auth_action.get_login_url();

    // Save return_to in a temporary cookie
    let return_cookie = Cookie::build(("return_to", return_to.clone()))
        .path("/")
        .http_only(true)
        .build();

    (jar.add(return_cookie), Redirect::to(&url))
}

#[utoipa::path(
    get,
    path = "/auth/callback",
    tag = "auth",
    params(
        ("code" = String, Query, description = "OAuth2 authorization code")
    ),
    responses(
        (status = 303, description = "Redirects to frontend with auth_token cookie on success or login_error parameter on failure")
    )
)]
pub async fn callback(jar: CookieJar, Query(query): Query<AuthCallbackQuery>) -> impl IntoResponse {
    let auth_action = AuthAction::new();

    let return_to = jar
        .get("return_to")
        .map(|c| c.value().to_string())
        .unwrap_or_else(|| "/".to_string());

    // Remove the return_to cookie
    let jar = jar.remove(Cookie::build(("return_to", "")).path("/").build());

    // Check if the user cancelled the authorization or an error occurred
    if let Some(error) = query.error {
        tracing::warn!("Discord OAuth error: {}", error);
        return (jar, Redirect::to(&return_to));
    }

    let code = match query.code {
        Some(c) => c,
        None => return (jar, Redirect::to(&return_to)),
    };

    match auth_action.handle_callback(code).await {
        Ok(token) => {
            let cookie = Cookie::build(("auth_token", token))
                .path("/")
                .http_only(false) // False allows frontend JS to check for connection
                .build();

            // Redirect to return_to
            let redirect_url = if return_to == "/" {
                "/?login=success".to_string()
            } else {
                return_to
            };
            (jar.add(cookie), Redirect::to(&redirect_url))
        }
        Err(e) => {
            tracing::error!("Auth error: {}", e);
            // Redirect to home page with error
            let redirect_url = if return_to == "/" {
                format!("/?login_error={}", urlencoding::encode(&e))
            } else {
                return_to
            };
            (jar, Redirect::to(&redirect_url))
        }
    }
}

#[utoipa::path(
    get,
    path = "/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Returns the current user status")
    )
)]
pub async fn me(jar: CookieJar) -> impl IntoResponse {
    if let Some(claims) = jar
        .get("auth_token")
        .and_then(|cookie| AuthAction::verify_token(cookie.value()))
    {
        return Json(json!({
            "authenticated": true,
            "username": claims.username,
            "role": claims.role,
        }))
        .into_response();
    }

    Json(json!({
        "authenticated": false
    }))
    .into_response()
}

#[utoipa::path(
    get,
    path = "/auth/logout",
    tag = "auth",
    params(
        ("return_to" = Option<String>, Query, description = "Optional URL to redirect to after logout")
    ),
    responses(
        (status = 303, description = "Clears the auth_token cookie and redirects to home or return_to")
    )
)]
pub async fn logout(jar: CookieJar, Query(query): Query<AuthLogoutQuery>) -> impl IntoResponse {
    let cookie = Cookie::build(("auth_token", "")).path("/").build();

    let return_to = safe_return_to(query.return_to);

    (jar.remove(cookie), Redirect::to(&return_to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_return_to_valid() {
        assert_eq!(
            safe_return_to(Some("/admin.html".to_string())),
            "/admin.html"
        );
        assert_eq!(safe_return_to(Some("/".to_string())), "/");
        assert_eq!(
            safe_return_to(Some("/api/auth/me".to_string())),
            "/api/auth/me"
        );
    }

    #[test]
    fn test_safe_return_to_invalid() {
        assert_eq!(safe_return_to(Some("https://google.com".to_string())), "/");
        assert_eq!(safe_return_to(Some("//malicious.com".to_string())), "/");
        assert_eq!(safe_return_to(Some("admin.html".to_string())), "/");
        assert_eq!(safe_return_to(None), "/");
    }
}

use crate::actions::AuthAction;
use crate::config::Config;
use axum::{
    Json,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;
use serde_json::json;
use time::Duration;

#[derive(Deserialize)]
pub struct AuthLoginQuery {
    pub return_to: Option<String>,
}

#[derive(Deserialize)]
pub struct AuthCallbackQuery {
    pub code: Option<String>,
    pub error: Option<String>,
    pub state: Option<String>,
}

#[derive(Deserialize)]
pub struct AuthLogoutQuery {
    pub return_to: Option<String>,
}

/// Ensures the return_to URL is a safe, relative path to prevent Open Redirect vulnerabilities.
fn safe_return_to(url: Option<String>) -> String {
    if let Some(url) = url.filter(|u| {
        u.len() <= 1024
            && u.starts_with('/')
            && !u.starts_with("//")
            && !u.contains('\\')
            && !u.chars().any(char::is_control)
    }) {
        return url;
    }
    "/".to_string()
}

fn auth_cookie(name: &'static str, value: String, seconds: i64) -> Cookie<'static> {
    Cookie::build((name, value))
        .path("/")
        .http_only(true)
        .secure(Config::global().secure_cookies())
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(seconds))
        .build()
}

#[utoipa::path(
    get,
    path = "/auth/login",
    tag = "auth",
    params(
        ("return_to" = Option<String>, Query, description = "Optional URL to redirect to after successful login")
    ),
    responses(
        (status = 303, description = "Redirects to Discord OAuth2 login page"),
        (status = 503, description = "Authentication configuration unavailable")
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
        return (jar, Redirect::to(&return_to)).into_response();
    }

    let auth_action = AuthAction::new();
    match auth_action.get_login_url(return_to) {
        Ok((url, state)) => (
            jar.add(auth_cookie("oauth_state", state, 600)),
            Redirect::to(&url),
        )
            .into_response(),
        Err(error) => {
            tracing::error!("Unable to start OAuth login: {}", error);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Connexion temporairement indisponible.",
            )
                .into_response()
        }
    }
}

#[utoipa::path(
    get,
    path = "/auth/callback",
    tag = "auth",
    params(
        ("code" = Option<String>, Query, description = "OAuth2 authorization code"),
        ("state" = String, Query, description = "Signed OAuth state bound to the browser cookie")
    ),
    responses(
        (status = 303, description = "Redirects to frontend after authentication"),
        (status = 400, description = "Missing, unbound, expired or invalid OAuth state or code")
    )
)]
pub async fn callback(jar: CookieJar, Query(query): Query<AuthCallbackQuery>) -> impl IntoResponse {
    let return_to = query.state.as_deref().and_then(|state| {
        jar.get("oauth_state")
            .filter(|cookie| cookie.value() == state)
            .and_then(|_| AuthAction::verify_oauth_state(state))
    });
    let jar = jar.remove(auth_cookie("oauth_state", String::new(), 0));
    let Some(return_to) = return_to else {
        return (
            jar,
            (
                StatusCode::BAD_REQUEST,
                "Tentative de connexion invalide ou expirée.",
            ),
        )
            .into_response();
    };
    let return_to = safe_return_to(Some(return_to));

    // Check if the user cancelled the authorization or an error occurred
    if let Some(error) = query.error {
        tracing::warn!("Discord OAuth error: {}", error);
        return (jar, Redirect::to(&return_to)).into_response();
    }

    let code = match query.code {
        Some(c) => c,
        None => {
            return (
                jar,
                (StatusCode::BAD_REQUEST, "Code de connexion manquant."),
            )
                .into_response();
        }
    };

    let auth_action = AuthAction::new();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        auth_action.handle_callback(code),
    )
    .await
    .unwrap_or_else(|_| Err("Discord authentication timed out".into()));
    match result {
        Ok(token) => {
            let cookie = auth_cookie("auth_token", token, 3600 * 24 * 7);

            // Redirect to return_to
            let redirect_url = if return_to == "/" {
                "/?login=success".to_string()
            } else {
                return_to
            };
            (jar.add(cookie), Redirect::to(&redirect_url)).into_response()
        }
        Err(e) => {
            tracing::error!("Auth error: {}", e);
            // Redirect to home page with error
            let redirect_url = if return_to == "/" {
                "/?login_error=authentication_failed".to_string()
            } else {
                return_to
            };
            (jar, Redirect::to(&redirect_url)).into_response()
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
    let cookie = auth_cookie("auth_token", String::new(), 0);

    let return_to = safe_return_to(query.return_to);

    (jar.remove(cookie), Redirect::to(&return_to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_is_inaccessible_to_javascript_and_has_expiry() {
        crate::config::init();
        let cookie = auth_cookie("auth_token", "test".into(), 604800);
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.max_age(), Some(Duration::seconds(604800)));
        assert_eq!(cookie.secure(), Some(Config::global().secure_cookies()));
        assert_eq!(cookie.domain(), None);
    }

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
        assert_eq!(safe_return_to(Some("/\\evil.example".to_string())), "/");
        assert_eq!(safe_return_to(Some("/\nevil.example".to_string())), "/");
        assert_eq!(safe_return_to(None), "/");
    }
}

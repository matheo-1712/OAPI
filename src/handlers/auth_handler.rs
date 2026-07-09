use crate::actions::AuthAction;
use axum::{
    extract::Query,
    response::{IntoResponse, Redirect},
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde_json::json;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct AuthCallbackQuery {
    pub code: String,
}

#[utoipa::path(
    get,
    path = "/auth/login",
    tag = "auth",
    responses(
        (status = 303, description = "Redirects to Discord OAuth2 login page")
    )
)]
pub async fn login(jar: CookieJar) -> impl IntoResponse {
    // Redirect to home if already authenticated
    if jar.get("auth_token")
        .and_then(|cookie| AuthAction::verify_token(cookie.value()))
        .is_some()
    {
        return Redirect::to("/");
    }

    let auth_action = AuthAction::new();
    let url = auth_action.get_login_url();
    Redirect::to(&url)
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

    match auth_action.handle_callback(query.code).await {
        Ok(token) => {
            let cookie = Cookie::build(("auth_token", token))
                .path("/")
                .http_only(false) // False allows frontend JS to check for connection
                .build();

            // Redirect to home page
            (jar.add(cookie), Redirect::to("/?login=success"))
        }
        Err(e) => {
            tracing::error!("Auth error: {}", e);
            // Redirect to home page with error
            (
                jar,
                Redirect::to(&format!("/?login_error={}", urlencoding::encode(&e))),
            )
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

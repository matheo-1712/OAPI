//! Route definitions for the OAPI server.
//!
//! This module aggregates all available API endpoints and sets up the
//! internal routing table using configuration values.

use crate::config::Config;
use crate::handlers::{
    admin_handler, auth_handler, discord_handler, investor_handler, middleware, minecraft_handler,
    monitoring_handler,
};
use axum::{
    Router,
    routing::{get, post},
};
use tower_http::services::ServeFile;

pub fn static_routes() -> Router {
    // Explicit files prevent legacy public/generated_images from bypassing authorization.
    Router::new()
        .route_service("/", ServeFile::new("public/index.html"))
        .route_service("/index.html", ServeFile::new("public/index.html"))
        .route_service("/monitoring.html", ServeFile::new("public/monitoring.html"))
        .route_service("/admin.html", ServeFile::new("public/admin.html"))
        .route_service("/investor.html", ServeFile::new("public/investor.html"))
        .route_service("/font/ARIAL.TTF", ServeFile::new("public/font/ARIAL.TTF"))
}

pub fn image_routes() -> Router {
    Router::new()
        .route(
            "/generated_images/:kind/:owner/:filename",
            get(crate::handlers::image_handler::get_image),
        )
        .route_layer(axum::middleware::from_fn(middleware::require_auth))
        .layer(axum::middleware::from_fn(middleware::private_response))
}

/// Aggregates all internal API routes into a single Router.
///
/// The paths used here are retrieved from the global configuration.
pub fn api_routes() -> Router {
    let routes = &Config::global().server.routes;

    // Need authentication for these routes, so we apply the auth middleware to them.
    let protected_routes = Router::new()
        .route(
            "/badges/me/refresh",
            post(crate::handlers::badge_handler::refresh_mine),
        )
        .route(
            "/badges/players/:player/refresh",
            post(crate::handlers::badge_handler::refresh_player),
        )
        .route(
            "/badges/discord/:user/refresh",
            post(crate::handlers::badge_handler::refresh_discord),
        )
        .route(
            &routes.discord_summary,
            post(discord_handler::create_discord_summary_by_id),
        )
        .route(
            &routes.minecraft_summary,
            post(minecraft_handler::create_minecraft_summary_by_id),
        )
        .route_layer(axum::middleware::from_fn(middleware::require_auth));

    let admin_routes = Router::new()
        .route(
            "/admin/badges/discord/:user/evaluate",
            post(crate::handlers::badge_handler::evaluate_discord),
        )
        .route(
            "/admin/badges/award",
            post(crate::handlers::badge_handler::award),
        )
        .route(
            "/admin/badges/players/:player/evaluate",
            post(crate::handlers::badge_handler::evaluate),
        )
        .route("/admin/users", get(admin_handler::get_all_users))
        .route_layer(axum::middleware::from_fn(middleware::require_admin));

    let investor_routes = Router::new()
        .route("/investor/stats", get(investor_handler::get_stats))
        .route_layer(axum::middleware::from_fn(middleware::require_investor));

    Router::new()
        .route("/auth/login", get(auth_handler::login))
        .route("/auth/callback", get(auth_handler::callback))
        .route("/auth/me", get(auth_handler::me))
        .route("/auth/logout", get(auth_handler::logout))
        .merge(protected_routes)
        .merge(admin_routes)
        .merge(investor_routes)
        .route(
            &routes.monitoring,
            get(monitoring_handler::get_monitoring_status),
        )
        .route(
            &format!("{}/check/:type/:name", routes.monitoring),
            get(monitoring_handler::check_single_service),
        )
        .layer(axum::middleware::from_fn(middleware::private_response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn badge_writes_require_authentication() {
        config::init();
        for path in [
            "/badges/me/refresh",
            "/badges/players/player/refresh",
            "/badges/discord/user/refresh",
            "/admin/badges/award",
            "/admin/badges/discord/user/evaluate",
            "/admin/badges/players/player/evaluate",
        ] {
            let response = api_routes()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header("content-type", "application/json")
                        .body(Body::from(
                            r#"{"kind":"player","recipient":"player","badge":"badge"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn images_require_login_and_static_files_cannot_bypass_it() {
        config::init();
        let filename = format!("{}.png", "a".repeat(64));
        let app = static_routes().merge(image_routes());
        for path in [
            format!("/generated_images/discord_summary/123/{}", filename),
            format!(
                "/generated_images/minecraft_summary/123456781234123412341234567890ab/{}",
                filename
            ),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(response.headers()["cache-control"], "no-store");
        }
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/GENERATED_IMAGES/discord_summary/123/{}",
                        filename
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn oauth_callback_rejects_missing_and_unbound_state() {
        config::init();
        for uri in [
            "/auth/callback?code=test",
            "/auth/callback?code=test&state=forged",
        ] {
            let response = api_routes()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test]
    async fn test_auth_login_redirect() {
        let _ = dotenvy::dotenv();
        config::init();

        let app = api_routes();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let expected = if config::Config::global().validate_security().is_ok() {
            StatusCode::SEE_OTHER
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };
        assert_eq!(response.status(), expected);
    }

    #[tokio::test]
    async fn test_investor_route_rejection() {
        let _ = dotenvy::dotenv();
        config::init();

        let app = api_routes();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/investor/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_admin_route_rejection() {
        let _ = dotenvy::dotenv();
        config::init();

        let app = api_routes();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/admin/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

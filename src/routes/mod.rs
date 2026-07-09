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

/// Aggregates all internal API routes into a single Router.
///
/// The paths used here are retrieved from the global configuration.
pub fn api_routes() -> Router {
    let routes = &Config::global().server.routes;

    // Need authentication for these routes, so we apply the auth middleware to them.
    let protected_routes = Router::new()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt;
    use crate::config;

    #[tokio::test]
    async fn test_auth_login_redirect() {
        let _ = dotenvy::dotenv();
        config::init();
        
        let app = api_routes();

        let response = app
            .oneshot(Request::builder().uri("/auth/login").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn test_investor_route_rejection() {
        let _ = dotenvy::dotenv();
        config::init();
        
        let app = api_routes();

        let response = app
            .oneshot(Request::builder().uri("/investor/stats").body(Body::empty()).unwrap())
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
            .oneshot(Request::builder().uri("/admin/users").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

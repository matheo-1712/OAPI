//! Route definitions for the OAPI server.
//!
//! This module aggregates all available API endpoints and sets up the
//! internal routing table using configuration values.

use crate::config::Config;
use crate::handlers::{
    admin_handler, auth_handler, discord_handler, friends_handler, investor_handler, middleware,
    minecraft_handler, monitoring_handler,
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
        .route_service("/theme.css", ServeFile::new("public/theme.css"))
        .route_service("/api-client.js", ServeFile::new("public/api-client.js"))
        .route_service("/explorer.js", ServeFile::new("public/explorer.js"))
        .route_service("/tools.js", ServeFile::new("public/tools.js"))
        .route_service("/explorer.html", ServeFile::new("public/explorer.html"))
        .route_service("/tools.html", ServeFile::new("public/tools.html"))
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
    // Website (Astroloutre) badge refresh: site_cors wraps require_auth so errors stay readable too
    let site_protected_routes = Router::new()
        .route(
            "/badges/me/refresh",
            post(crate::handlers::badge_handler::refresh_mine),
        )
        .route_layer(axum::middleware::from_fn(middleware::require_auth))
        .route_layer(axum::middleware::from_fn(middleware::site_cors));

    // Friends: session cookie for `me` (website included), service token for explicit accounts.
    // Mutations are bodiless POSTs so the website never needs a CORS preflight.
    let friends_routes = Router::new()
        .route("/friends/:account", get(friends_handler::list))
        .route(
            "/friends/:account/:user/request",
            post(friends_handler::request),
        )
        .route(
            "/friends/:account/:user/accept",
            post(friends_handler::accept),
        )
        .route(
            "/friends/:account/:user/remove",
            post(friends_handler::remove),
        )
        .route_layer(axum::middleware::from_fn(middleware::require_auth))
        .route_layer(axum::middleware::from_fn(middleware::site_cors));

    let protected_routes = Router::new()
        .route(
            "/badges/players/:player/refresh",
            post(crate::handlers::badge_handler::refresh_player),
        )
        .route(
            "/badges/discord/:user/refresh",
            post(crate::handlers::badge_handler::refresh_discord),
        )
        .route(
            &routes.minecraft_summary,
            post(minecraft_handler::create_minecraft_summary_by_id),
        )
        .route_layer(axum::middleware::from_fn(middleware::require_auth));

    let discord_routes = Router::new()
        .route(
            &routes.discord_summary.replace(":id", "me"),
            post(discord_handler::create_my_discord_summary),
        )
        .route(
            "/discord/stats/me",
            get(crate::handlers::discord_activity::personal),
        )
        .route(
            &routes.discord_summary,
            post(discord_handler::create_discord_summary_by_id),
        )
        .route_layer(axum::middleware::from_fn(middleware::require_auth));
    let discord_admin_routes = Router::new()
        .route(
            "/admin/discord/stats",
            get(crate::handlers::discord_activity::global),
        )
        .route_layer(axum::middleware::from_fn(middleware::require_admin));

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

    // Website (Astroloutre) session, readable cross-origin by site_auth.url only
    let site_auth_routes = Router::new()
        .route("/auth/site/me", get(auth_handler::site_me))
        .route_layer(axum::middleware::from_fn(middleware::site_cors));

    Router::new()
        .route("/auth/login", get(auth_handler::login))
        .route("/auth/callback", get(auth_handler::callback))
        .route("/auth/me", get(auth_handler::me))
        .route("/auth/logout", get(auth_handler::logout))
        .route("/auth/site/login", get(auth_handler::site_login))
        .route("/auth/site/logout", get(auth_handler::site_logout))
        .merge(site_auth_routes)
        .merge(site_protected_routes)
        .merge(friends_routes)
        .merge(protected_routes)
        .merge(discord_routes)
        .merge(discord_admin_routes)
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
    async fn site_session_is_readable_by_the_configured_website_only() {
        config::init();
        let site = config::Config::global()
            .site_origin()
            .expect("site_auth.url must be configured")
            .to_string();
        for (origin, allowed) in [(site.as_str(), true), ("https://evil.example", false)] {
            let response = api_routes()
                .oneshot(
                    Request::builder()
                        .uri("/auth/site/me")
                        .header("origin", origin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let headers = response.headers();
            assert_eq!(
                headers
                    .get("access-control-allow-origin")
                    .map(|value| value.to_str().unwrap()),
                allowed.then_some(origin)
            );
            assert_eq!(
                headers.get("access-control-allow-credentials").is_some(),
                allowed
            );
            assert_eq!(headers["vary"], "Origin");
        }
    }

    #[tokio::test]
    async fn site_badge_refresh_passes_csrf_and_is_readable_by_the_website_only() {
        config::init();
        let site = config::Config::global()
            .site_origin()
            .expect("site_auth.url must be configured")
            .to_string();
        // Without a session: the website reaches the auth check (401), other origins stop at CSRF (403)
        for (origin, status, allowed) in [
            (site.as_str(), StatusCode::UNAUTHORIZED, true),
            ("https://evil.example", StatusCode::FORBIDDEN, false),
        ] {
            let response = api_routes()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/badges/me/refresh")
                        .header("origin", origin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            let headers = response.headers();
            assert_eq!(
                headers
                    .get("access-control-allow-origin")
                    .map(|value| value.to_str().unwrap()),
                allowed.then_some(origin)
            );
            assert_eq!(
                headers.get("access-control-allow-credentials").is_some(),
                allowed
            );
        }
    }

    #[tokio::test]
    async fn friends_require_authentication_and_reject_foreign_origins() {
        config::init();
        let site = config::Config::global()
            .site_origin()
            .expect("site_auth.url must be configured")
            .to_string();
        for (method, path) in [
            ("GET", "/friends/me"),
            ("GET", "/friends/account"),
            ("POST", "/friends/me/user/request"),
            ("POST", "/friends/me/user/accept"),
            ("POST", "/friends/account/user/remove"),
        ] {
            for cookie in ["", "auth_token=forged"] {
                let response = api_routes()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(path)
                            .header("cookie", cookie)
                            .header("origin", site.as_str())
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                assert_eq!(response.headers()["cache-control"], "no-store");
                assert_eq!(
                    response.headers()["access-control-allow-origin"],
                    site.as_str()
                );
            }
            if method == "POST" {
                let response = api_routes()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(path)
                            .header("origin", "https://evil.example")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::FORBIDDEN);
            }
        }
    }

    #[tokio::test]
    async fn site_logout_redirects_to_the_website_path() {
        config::init();
        let site = config::Config::global().site_origin().unwrap().to_string();
        for (query, expected) in [
            (
                "/auth/site/logout?return_to=/profil",
                format!("{site}/profil"),
            ),
            (
                "/auth/site/logout?return_to=https://evil.example",
                format!("{site}/"),
            ),
            ("/auth/site/logout", format!("{site}/")),
        ] {
            let response = api_routes()
                .oneshot(Request::builder().uri(query).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            assert_eq!(response.headers()["location"], expected.as_str());
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

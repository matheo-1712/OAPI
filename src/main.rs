//! OAPI - Otter API
//!
//! A high-performance Rust API based on Axum, designed to automate profile image
//! generation and statistics summaries for the Otter community.

mod actions;
mod config;
mod handlers;
mod models;
mod routes;
mod services;
mod utils;

use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::auth_handler::login,
        handlers::auth_handler::callback,
        handlers::auth_handler::me,
        handlers::auth_handler::logout,
        handlers::admin_handler::get_all_users,
        handlers::badge_handler::award,
        handlers::badge_handler::evaluate,
        handlers::badge_handler::evaluate_discord,
        handlers::badge_handler::refresh_player,
        handlers::badge_handler::refresh_mine,
        handlers::badge_handler::refresh_discord,
        handlers::investor_handler::get_stats,
        handlers::image_handler::get_image,
        handlers::discord_activity::personal,
        handlers::discord_activity::global,
        handlers::discord_handler::create_discord_summary_by_id,
        handlers::discord_handler::create_my_discord_summary,
        handlers::minecraft_handler::create_minecraft_summary_by_id,
        handlers::monitoring_handler::get_monitoring_status,
        handlers::monitoring_handler::check_single_service
    ),
    components(schemas(
        models::investor::InvestorStats,
        models::discord_activity::DiscordActivity,
        models::discord_activity::ActivityDay,
        models::discord_activity::ChannelTotal,
        models::discord_activity::VoiceCompanion,
        services::badge_service::AwardRequest,
        services::badge_service::RefreshResponse,
        services::badge_service::NewBadge,
        services::badge_service::AwardResult,
        services::badge_service::RecipientKind,
        models::ImageRequest,
        models::ImageResponse,
        models::DiscordUser,
        models::DiscordRole,
        models::DiscordStats,
        models::DiscordChannel,
        models::DiscordVoiceConnection,
        models::MinecraftPlayer,
        models::MinecraftStats,
        models::Server,
        models::monitoring::MonitoringResponse,
        models::monitoring::ServiceResult,
        models::monitoring::ServiceStatus,
        models::monitoring::ServiceMetadata,
        models::monitoring::DiscordBotMetadata,
        models::monitoring::DiscordBotUptime,
        models::monitoring::DiscordBotDiscordData,
        models::monitoring::MinecraftMetadata
    ))
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Determine environment (default to development)
    let env = std::env::var("OAPI_ENV").unwrap_or_else(|_| "development".to_string());

    // Determine default log level based on environment
    let default_log_level = if env == "production" {
        "info"
    } else {
        "info,OAPI=debug,tower_http=debug"
    };

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_log_level.into()),
        )
        .compact()
        .init();

    // Initialize config
    config::init();
    config::Config::global()
        .validate_security()
        .expect("Invalid authentication configuration");

    let banner = r#"
  ____               ____    ___ 
 / __ \     /\      |  _ \  |_ _|
| |  | |   /  \     | |_) |  | | 
| |  | |  / /\ \    |  __/   | | 
| |__| | / ____ \   | |     _| |_
 \____/ /_/    \_\  |_|    |_____|
                                  
    "#;
    println!("{}", banner);

    // Build app
    let config = config::Config::global();

    // Modify OpenAPI spec at runtime to match configured routes
    let mut openapi = ApiDoc::openapi();
    update_openapi_paths(&mut openapi, config);

    let app = Router::new()
        .nest(&config.server.routes.base, routes::api_routes())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi))
        .merge(routes::static_routes())
        .merge(routes::image_routes())
        .layer(axum::middleware::from_fn(
            handlers::rate_limit::limit_requests,
        ))
        .layer(axum::middleware::from_fn(
            handlers::middleware::private_response,
        ))
        .layer(TraceLayer::new_for_http());

    let addr_str = format!("{}:{}", config.server.host, config.server.port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid address/port in config");

    tracing::info!("🚀 OAPI Server starting up...");
    tracing::info!("📡 Listening on http://{}", addr);
    tracing::info!("📖 Swagger UI available at http://{}/swagger-ui", addr);
    tracing::info!("🌐 Web interface available at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

/// Update OpenAPI paths to match dynamic configuration
fn update_openapi_paths(openapi: &mut utoipa::openapi::OpenApi, config: &config::Config) {
    use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityRequirement, SecurityScheme};
    openapi
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("auth_token"))),
        );
    let mut new_paths = utoipa::openapi::path::PathsBuilder::new();
    let base = &config.server.routes.base;

    // For each path in the original spec, try to find a matching config and remap it
    for (path, item) in openapi.paths.paths.iter() {
        let route = match path.as_str() {
            "/discord-summary/{id}" => config.server.routes.discord_summary.clone(),
            "/discord-summary/me" => config.server.routes.discord_summary.replace(":id", "me"),
            "/minecraft-summary/{uuid}" => config.server.routes.minecraft_summary.clone(),
            "/monitoring" => config.server.routes.monitoring.clone(),
            "/monitoring/check/{type}/{name}" => {
                format!("{}/check/:type/:name", config.server.routes.monitoring)
            }
            _ => path.clone(),
        };
        let route = route
            .split('/')
            .map(|segment| {
                segment
                    .strip_prefix(':')
                    .map(|name| format!("{{{name}}}"))
                    .unwrap_or_else(|| segment.to_string())
            })
            .collect::<Vec<_>>()
            .join("/");
        // Images are mounted at the application root, outside the API prefix.
        let new_path = if path.starts_with("/generated_images/") {
            route
        } else {
            format!("{}{}", base.trim_end_matches('/'), route)
        };
        let mut item = item.clone();
        if path.starts_with("/admin/")
            || path.starts_with("/badges/")
            || path.starts_with("/investor/")
            || path.starts_with("/discord/")
            || path.starts_with("/generated_images/")
            || path == "/auth/me"
            || path == "/discord-summary/{id}"
            || path == "/discord-summary/me"
            || path == "/minecraft-summary/{uuid}"
        {
            for operation in item.operations.values_mut() {
                operation.security = Some(vec![SecurityRequirement::new(
                    "session_cookie",
                    Vec::<String>::new(),
                )]);
            }
        }
        for operation in item.operations.values_mut() {
            operation.responses.responses.entry("429".to_string()).or_insert_with(||
                utoipa::openapi::Response::new("10 requests per minute per route and account or anonymous IP; Retry-After in seconds").into());
        }
        new_paths = new_paths.path(new_path, item);
    }

    openapi.paths = new_paths.build();
}

#[cfg(test)]
mod openapi_tests {
    use super::*;

    #[test]
    fn swagger_contains_every_api_endpoint_and_root_image_download() {
        config::init();
        let config = config::Config::global();
        let mut spec = ApiDoc::openapi();
        update_openapi_paths(&mut spec, config);
        let json = serde_json::to_value(spec).unwrap();
        let base = config.server.routes.base.trim_end_matches('/');
        for (path, method) in [
            ("/auth/login", "get"),
            ("/auth/callback", "get"),
            ("/auth/me", "get"),
            ("/auth/logout", "get"),
            ("/admin/users", "get"),
            ("/investor/stats", "get"),
            ("/discord/stats/me", "get"),
            ("/admin/discord/stats", "get"),
            ("/badges/me/refresh", "post"),
            ("/badges/players/{player}/refresh", "post"),
            ("/badges/discord/{user}/refresh", "post"),
            ("/admin/badges/discord/{user}/evaluate", "post"),
            ("/admin/badges/players/{player}/evaluate", "post"),
            ("/admin/badges/award", "post"),
        ] {
            assert!(
                json["paths"][format!("{base}{path}")][method].is_object(),
                "Missing {method} {path}"
            );
        }
        assert!(json["paths"]["/generated_images/{kind}/{owner}/{filename}"]["get"].is_object());
        let personal_image_path = format!(
            "{base}{}",
            config.server.routes.discord_summary.replace(":id", "me")
        );
        assert!(json["paths"][&personal_image_path]["post"].is_object());
        assert_eq!(
            json["paths"][&personal_image_path]["post"]["security"][0],
            serde_json::json!({"session_cookie":[]})
        );
        assert_eq!(json["paths"].as_object().unwrap().len(), 20);
        for item in json["paths"].as_object().unwrap().values() {
            for operation in item.as_object().unwrap().values() {
                assert!(operation["responses"]["429"].is_object());
            }
        }
        assert_eq!(
            json["components"]["securitySchemes"]["session_cookie"]["in"],
            "cookie"
        );
        assert_eq!(
            json["paths"][format!("{base}/admin/discord/stats")]["get"]["security"][0],
            serde_json::json!({"session_cookie":[]})
        );
    }

    #[test]
    fn configured_paths_support_multiple_parameters_and_images_stay_at_root() {
        config::init();
        let mut config = config::Config::global().clone();
        config.server.routes.base = "/v2/api".into();
        config.server.routes.discord_summary = "/profiles/:id/discord".into();
        config.server.routes.minecraft_summary = "/profiles/:uuid/minecraft".into();
        config.server.routes.monitoring = "/health".into();
        let mut spec = ApiDoc::openapi();
        update_openapi_paths(&mut spec, &config);
        for path in [
            "/v2/api/profiles/{id}/discord",
            "/v2/api/profiles/me/discord",
            "/v2/api/profiles/{uuid}/minecraft",
            "/v2/api/health",
            "/v2/api/health/check/{type}/{name}",
            "/generated_images/{kind}/{owner}/{filename}",
        ] {
            assert!(spec.paths.paths.contains_key(path), "Missing {path}");
        }
    }
}

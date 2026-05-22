use crate::actions::monitoring_actions;
use crate::config::Config;
use crate::models::monitoring::MonitoringResponse;
use axum::Json;
use axum::extract::Path as AxumPath;
use axum::http::StatusCode;
use axum::response::IntoResponse;

/// Retrieve the health status of all configured services.
///
/// This endpoint performs real-time health checks (HTTP/TCP) on the services
/// defined in the configuration and returns an aggregated status report.
#[utoipa::path(
    get,
    path = "/monitoring",
    responses(
        (status = 200, description = "Aggregated health status of all services", body = MonitoringResponse)
    ),
    tag = "Monitoring"
)]
pub async fn get_monitoring_status() -> impl IntoResponse {
    let config = &Config::global().monitoring;
    let services = monitoring_actions::check_all_services(config).await;

    Json(MonitoringResponse { services })
}

/// Check the health of a specific service.
#[utoipa::path(
    get,
    path = "/monitoring/check/{type}/{name}",
    params(
        ("type" = String, Path, description = "Service type (discord, minecraft, etc.)"),
        ("name" = String, Path, description = "Service name")
    ),
    responses(
        (status = 200, description = "Health status of the specific service", body = crate::models::monitoring::ServiceResult),
        (status = 404, description = "Service not found")
    ),
    tag = "Monitoring"
)]
pub async fn check_single_service(
    AxumPath((service_type, service_name)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    let config = &Config::global().monitoring;

    // Find the service in the config and await the check
    let result = match service_type.as_str() {
        "discord" => {
            let s = config
                .discord
                .as_ref()
                .and_then(|v| v.iter().find(|s| s.name == service_name));

            match s {
                Some(s) => Some(
                    monitoring_actions::check_discord_service(s.name.clone(), s.url.clone()).await,
                ),
                None => None,
            }
        }
        "minecraft" => {
            let s = config
                .minecraft
                .as_ref()
                .and_then(|v| v.iter().find(|s| s.name == service_name));

            match s {
                Some(s) => Some(
                    monitoring_actions::check_minecraft_service(
                        s.name.clone(),
                        s.host.clone(),
                        s.port,
                    )
                    .await,
                ),
                None => None,
            }
        }
        "site" | "api" | "self-hosted" | "http" => {
            let list = match service_type.as_str() {
                "site" => &config.site,
                "api" => &config.api,
                "self-hosted" => &config.self_hosted,
                _ => &config.http,
            };

            let s = list
                .as_ref()
                .and_then(|v| v.iter().find(|s| s.name == service_name));

            match s {
                Some(s) => Some(
                    monitoring_actions::check_http_service(
                        s.name.clone(),
                        s.url.clone(),
                        &service_type,
                    )
                    .await,
                ),
                None => None,
            }
        }
        _ => None,
    };

    match result {
        Some(res) => Json(res).into_response(),
        None => (StatusCode::NOT_FOUND, "Service not found").into_response(),
    }
}

use crate::config::MonitoringConfig;
use crate::models::monitoring::{
    DiscordBotDiscordData, DiscordBotMetadata, DiscordBotUptime, MinecraftMetadata,
    ServiceMetadata, ServiceResult, ServiceStatus,
};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Internal structure to parse Discord bot health check response.
#[derive(serde::Deserialize)]
struct DiscordBotHealthResponse {
    version: String,
    uptime: DiscordBotUptime,
    discord: DiscordBotDiscordData,
}

/// Checks the status of all configured services concurrently.
pub async fn check_all_services(config: &MonitoringConfig) -> Vec<ServiceResult> {
    let mut handles = Vec::new();

    // Spawn tasks for Discord bots
    if let Some(services) = &config.discord {
        for service in services {
            let name = service.name.clone();
            let url = service.url.clone();
            handles.push(tokio::spawn(async move {
                check_discord_service(name, url).await
            }));
        }
    }

    // Spawn tasks for Minecraft servers
    if let Some(services) = &config.minecraft {
        for service in services {
            let name = service.name.clone();
            let host = service.host.clone();
            let port = service.port;
            handles.push(tokio::spawn(async move {
                check_minecraft_service(name, host, port).await
            }));
        }
    }

    // Spawn tasks for other HTTP services (Site, Api, SelfHosted, Http)
    let http_groups = [
        (&config.site, "site"),
        (&config.api, "api"),
        (&config.self_hosted, "self-hosted"),
        (&config.http, "http"),
    ];

    for (group, type_name) in http_groups {
        if let Some(services) = group {
            for service in services {
                let name = service.name.clone();
                let url = service.url.clone();
                let t_name = type_name.to_string();
                handles.push(tokio::spawn(async move {
                    check_http_service(name, url, &t_name).await
                }));
            }
        }
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }

    // Sort results by type_name to ensure consistent grouping
    results.sort_by(|a, b| a.type_name.cmp(&b.type_name));

    results
}

/// Performs a generic HTTP health check.
pub async fn check_http_service(name: String, url: String, type_name: &str) -> ServiceResult {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let start = Instant::now();
    let resp = client.get(&url).send().await;
    let duration = start.elapsed().as_millis() as u64;

    let type_name_str = type_name.to_string();

    match resp {
        Ok(res) if res.status().is_success() => ServiceResult {
            name,
            type_name: type_name_str,
            status: ServiceStatus::Up,
            response_time_ms: duration,
            metadata: None,
            error: None,
        },
        Ok(res) => ServiceResult {
            name,
            type_name: type_name_str,
            status: ServiceStatus::Down,
            response_time_ms: duration,
            metadata: None,
            error: Some(format!("HTTP Error: {}", res.status())),
        },
        Err(e) => ServiceResult {
            name,
            type_name: type_name_str,
            status: ServiceStatus::Down,
            response_time_ms: duration,
            metadata: None,
            error: Some(e.to_string()),
        },
    }
}

/// Performs a Discord bot health check with specific metadata.
pub async fn check_discord_service(name: String, url: String) -> ServiceResult {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let start = Instant::now();
    let resp = client.get(&url).send().await;
    let duration = start.elapsed().as_millis() as u64;

    match resp {
        Ok(res) if res.status().is_success() => {
            let mut metadata = None;

            // Try to parse Discord bot metadata
            if let Ok(body) = res.json::<DiscordBotHealthResponse>().await {
                metadata = Some(ServiceMetadata::DiscordBot(DiscordBotMetadata {
                    version: body.version,
                    uptime: body.uptime,
                    discord: body.discord,
                }));
            }

            ServiceResult {
                name,
                type_name: "discord".to_string(),
                status: ServiceStatus::Up,
                response_time_ms: duration,
                metadata,
                error: None,
            }
        }
        Ok(res) => ServiceResult {
            name,
            type_name: "discord".to_string(),
            status: ServiceStatus::Down,
            response_time_ms: duration,
            metadata: None,
            error: Some(format!("HTTP Error: {}", res.status())),
        },
        Err(e) => ServiceResult {
            name,
            type_name: "discord".to_string(),
            status: ServiceStatus::Down,
            response_time_ms: duration,
            metadata: None,
            error: Some(e.to_string()),
        },
    }
}

/// Performs a Minecraft (TCP) health check and fetches player count.
pub async fn check_minecraft_service(name: String, host: String, port: u16) -> ServiceResult {
    let start = Instant::now();
    let addr = (host.as_str(), port);

    // Use craftping to get server status and player count
    let result = timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(addr).await?;
        craftping::tokio::ping(&mut stream, &host, port).await
    })
    .await;

    let duration = start.elapsed().as_millis() as u64;

    match result {
        Ok(Ok(response)) => ServiceResult {
            name,
            type_name: "minecraft".to_string(),
            status: ServiceStatus::Up,
            response_time_ms: duration,
            metadata: Some(ServiceMetadata::Minecraft(MinecraftMetadata {
                online_players: response.online_players as u32,
                max_players: response.max_players as u32,
            })),
            error: None,
        },
        Ok(Err(e)) => ServiceResult {
            name,
            type_name: "minecraft".to_string(),
            status: ServiceStatus::Down,
            response_time_ms: duration,
            metadata: None,
            error: Some(format!("Ping failed: {}", e)),
        },
        Err(_) => ServiceResult {
            name,
            type_name: "minecraft".to_string(),
            status: ServiceStatus::Down,
            response_time_ms: duration,
            metadata: None,
            error: Some("Ping timed out".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::monitoring::ServiceStatus;
    use mockito::Server;

    #[tokio::test]
    async fn test_check_http_service_success() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("GET", "/")
            .with_status(200)
            .create_async()
            .await;

        let result = check_http_service("Test".to_string(), server.url(), "http").await;
        assert_eq!(result.status, ServiceStatus::Up);
        assert_eq!(result.type_name, "http");
    }

    #[tokio::test]
    async fn test_check_http_service_failure() {
        let mut server = Server::new_async().await;
        let _m = server
            .mock("GET", "/")
            .with_status(500)
            .create_async()
            .await;

        let result = check_http_service("Test".to_string(), server.url(), "http").await;
        assert_eq!(result.status, ServiceStatus::Down);
    }

    #[tokio::test]
    async fn test_check_discord_service_metadata() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "version": "1.0.0",
            "uptime": { "seconds": 100, "human": "1m" },
            "discord": { "ping": 50, "avatar": "http://avatar" }
        }"#;

        let _m = server
            .mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let result = check_discord_service("Bot".to_string(), server.url()).await;
        assert_eq!(result.status, ServiceStatus::Up);
        assert_eq!(result.type_name, "discord");

        if let Some(ServiceMetadata::DiscordBot(meta)) = result.metadata {
            assert_eq!(meta.version, "1.0.0");
            assert_eq!(meta.discord.ping, 50);
        } else {
            panic!("Metadata should be DiscordBot");
        }
    }

    #[tokio::test]
    async fn test_check_minecraft_service_timeout() {
        // Using a non-routable IP to force a timeout or fast failure
        let result =
            check_minecraft_service("MC".to_string(), "192.0.2.1".to_string(), 25565).await;
        assert_eq!(result.status, ServiceStatus::Down);
    }
}

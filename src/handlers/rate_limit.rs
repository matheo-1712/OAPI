//! One quota policy applied to every HTTP route at the application boundary.
use axum::{
    extract::{ConnectInfo, MatchedPath, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const QUOTA: u32 = 10;
const WINDOW: Duration = Duration::from_secs(60);
const MAX_KEYS: usize = 10_000;

#[derive(Default)]
struct Quotas {
    entries: HashMap<(String, String), (Instant, u32)>,
}
impl Quotas {
    fn consume(&mut self, identity: String, route: String, now: Instant) -> Option<u64> {
        self.entries
            .retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
        let key = (identity, route);
        if !self.entries.contains_key(&key) && self.entries.len() >= MAX_KEYS {
            return Some(60);
        }
        let (start, count) = self.entries.entry(key).or_insert((now, 0));
        if *count >= QUOTA {
            Some((60 - now.duration_since(*start).as_secs()).max(1))
        } else {
            *count += 1;
            None
        }
    }
}

fn client_ip(
    peer: IpAddr,
    headers: &axum::http::HeaderMap,
    trusted: &[IpAddr],
) -> Result<IpAddr, StatusCode> {
    if !trusted.contains(&peer) {
        return Ok(peer);
    }
    let Some(header) = headers.get("x-forwarded-for") else {
        return Ok(peer);
    };
    let value = header.to_str().map_err(|_| StatusCode::BAD_REQUEST)?;
    if value.len() > 512 || headers.get_all("x-forwarded-for").iter().count() != 1 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let chain = value
        .split(',')
        .map(|part| {
            part.trim()
                .parse::<IpAddr>()
                .map_err(|_| StatusCode::BAD_REQUEST)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if chain.len() > 10 {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(chain
        .into_iter()
        .rev()
        .find(|ip| !trusted.contains(ip))
        .unwrap_or(peer))
}

/// Ten requests per minute, per authenticated subject (or anonymous IP), per route.
/// MatchedPath groups dynamic IDs and wildcard assets into their actual route template.
pub async fn limit_requests(request: Request, next: Next) -> Response {
    static LIMITS: OnceLock<Mutex<Quotas>> = OnceLock::new();
    let Some(ConnectInfo(peer)) = request.extensions().get::<ConnectInfo<SocketAddr>>() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Client connection unavailable",
        )
            .into_response();
    };
    let ip = match client_ip(
        peer.ip(),
        request.headers(),
        &crate::config::Config::global().server.trusted_proxies,
    ) {
        Ok(ip) => ip,
        Err(status) => return status.into_response(),
    };
    let jar = CookieJar::from_headers(request.headers());
    let identity = jar
        .get("auth_token")
        .and_then(|cookie| crate::actions::AuthAction::verify_token(cookie.value()))
        .map(|claims| format!("user:{}", claims.sub))
        .unwrap_or_else(|| format!("ip:{ip}"));
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str())
        .unwrap_or("<unmatched>")
        .to_string();
    let retry = match LIMITS.get_or_init(Default::default).lock() {
        Ok(mut limits) => limits.consume(identity, route, Instant::now()),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    if let Some(seconds) = retry {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", seconds.to_string())],
            "Trop de requêtes. Réessayez plus tard.",
        )
            .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotas_are_equal_per_route_and_identity_and_expire() {
        let mut quotas = Quotas::default();
        let start = Instant::now();
        for _ in 0..10 {
            assert_eq!(quotas.consume("user:1".into(), "/a".into(), start), None);
        }
        assert_eq!(
            quotas.consume("user:1".into(), "/a".into(), start),
            Some(60)
        );
        assert_eq!(quotas.consume("user:1".into(), "/b".into(), start), None);
        assert_eq!(quotas.consume("user:2".into(), "/a".into(), start), None);
        assert_eq!(
            quotas.consume("user:1".into(), "/a".into(), start + WINDOW),
            None
        );
    }
    #[test]
    fn forwarded_ips_require_a_trusted_peer_and_ignore_spoofed_leftmost_addresses() {
        let proxy: IpAddr = "127.0.0.1".parse().unwrap();
        let client: IpAddr = "192.0.2.10".parse().unwrap();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1, 192.0.2.10".parse().unwrap(),
        );
        assert_eq!(client_ip(client, &headers, &[proxy]).unwrap(), client);
        assert_eq!(client_ip(proxy, &headers, &[proxy]).unwrap(), client);
        headers.insert("x-forwarded-for", "invalid".parse().unwrap());
        assert_eq!(
            client_ip(proxy, &headers, &[proxy]),
            Err(StatusCode::BAD_REQUEST)
        );
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use axum::{Extension, Router, body::Body, http::Request as HttpRequest, routing::get};
    use tower::ServiceExt;

    #[tokio::test]
    async fn global_quota_covers_routes_and_groups_dynamic_ids_without_trusting_headers() {
        crate::config::init();
        let app = Router::new()
            .nest(
                "/api",
                Router::new().route("/users/:id", get(|| async { "ok" })),
            )
            .route("/swagger-ui", get(|| async { "ok" }))
            .route("/index.html", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(limit_requests))
            .layer(axum::middleware::from_fn(
                crate::handlers::middleware::private_response,
            ))
            .layer(Extension(ConnectInfo(
                "192.0.2.123:1234".parse::<SocketAddr>().unwrap(),
            )));
        for route in ["/api/users/", "/swagger-ui", "/index.html", "/unknown"] {
            for count in 0..11 {
                let path = if route.ends_with('/') {
                    format!("{route}{count}")
                } else {
                    route.into()
                };
                let response = app
                    .clone()
                    .oneshot(
                        HttpRequest::builder()
                            .uri(path)
                            .header("x-forwarded-for", format!("203.0.113.{count}"))
                            .header("cookie", format!("auth_token=forged{count}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let expected = if count == 10 {
                    StatusCode::TOO_MANY_REQUESTS
                } else if route == "/unknown" {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::OK
                };
                assert_eq!(response.status(), expected, "{route} request {count}");
                assert_eq!(response.headers()["cache-control"], "no-store");
                if count == 10 {
                    assert!(response.headers().contains_key("retry-after"));
                }
            }
        }
    }
}

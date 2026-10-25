//! One quota policy applied to every HTTP route at the application boundary.
use axum::{
    extract::{ConnectInfo, MatchedPath, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use ipnet::{IpNet, Ipv6Net};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const QUOTA: u32 = 10;
const WINDOW: Duration = Duration::from_secs(60);
const MAX_KEYS: usize = 10_000;
/// Expired windows are swept at most this often instead of on every request.
const SWEEP_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Quotas {
    entries: HashMap<(String, String), (Instant, u32)>,
    last_sweep: Option<Instant>,
}
impl Quotas {
    fn sweep(&mut self, now: Instant) {
        self.entries
            .retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
        self.last_sweep = Some(now);
    }

    fn consume(&mut self, identity: String, route: String, now: Instant) -> Option<u64> {
        if self
            .last_sweep
            .is_none_or(|last| now.duration_since(last) >= SWEEP_INTERVAL)
        {
            self.sweep(now);
        }
        let key = (identity, route);
        if !self.entries.contains_key(&key) && self.entries.len() >= MAX_KEYS {
            self.sweep(now);
            // A full table evicts its oldest window rather than rejecting every newcomer,
            // so flooding it with fresh identities cannot lock legitimate clients out.
            if self.entries.len() >= MAX_KEYS
                && let Some(oldest) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, (start, _))| *start)
                    .map(|(key, _)| key.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        let (start, count) = self.entries.entry(key).or_insert((now, 0));
        if now.duration_since(*start) >= WINDOW {
            *start = now;
            *count = 0;
        }
        if *count >= QUOTA {
            Some((60 - now.duration_since(*start).as_secs()).max(1))
        } else {
            *count += 1;
            None
        }
    }
}

fn is_trusted(ip: &IpAddr, trusted: &[IpNet]) -> bool {
    trusted.iter().any(|net| net.contains(ip))
}

/// Quota key for an anonymous client. One IPv6 subscriber usually holds a whole /64,
/// so its addresses share a single quota instead of each getting their own.
fn anonymous_identity(ip: IpAddr) -> String {
    match ip.to_canonical() {
        IpAddr::V4(v4) => format!("ip:{v4}"),
        IpAddr::V6(v6) => {
            let prefix = Ipv6Net::new(v6, 64).map(|net| net.trunc()).unwrap_or_default();
            format!("ip:{prefix}")
        }
    }
}

fn client_ip(
    peer: IpAddr,
    headers: &axum::http::HeaderMap,
    trusted: &[IpNet],
) -> Result<IpAddr, StatusCode> {
    let peer = peer.to_canonical();
    if !is_trusted(&peer, trusted) {
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
                .map(|ip| ip.to_canonical())
                .map_err(|_| StatusCode::BAD_REQUEST)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if chain.len() > 10 {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(chain
        .into_iter()
        .rev()
        .find(|ip| !is_trusted(ip, trusted))
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
    let identity = crate::handlers::middleware::request_claims(request.headers())
        .map(|claims| format!("user:{}", claims.sub))
        .unwrap_or_else(|| anonymous_identity(ip));
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
    fn a_full_table_evicts_the_oldest_window_instead_of_rejecting_newcomers() {
        let mut quotas = Quotas::default();
        let start = Instant::now();
        for index in 0..MAX_KEYS {
            assert_eq!(quotas.consume(format!("ip:{index}"), "/a".into(), start), None);
        }
        let later = start + Duration::from_secs(1);
        assert_eq!(quotas.consume("ip:new".into(), "/a".into(), later), None);
        assert_eq!(quotas.entries.len(), MAX_KEYS);
        assert!(quotas.entries.contains_key(&("ip:new".to_string(), "/a".to_string())));
    }

    #[test]
    fn expired_windows_restart_even_before_the_next_sweep() {
        let mut quotas = Quotas::default();
        let start = Instant::now();
        for _ in 0..QUOTA {
            quotas.consume("ip:1".into(), "/a".into(), start);
        }
        quotas.last_sweep = Some(start + WINDOW);
        assert_eq!(quotas.consume("ip:1".into(), "/a".into(), start + WINDOW), None);
    }

    #[test]
    fn ipv6_clients_share_one_quota_per_64_prefix() {
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb::2".parse().unwrap();
        let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(anonymous_identity(a), anonymous_identity(b));
        assert_ne!(anonymous_identity(a), anonymous_identity(other));
        assert_eq!(anonymous_identity(a), "ip:2001:db8:1:2::/64");
        let mapped: IpAddr = "::ffff:192.0.2.7".parse().unwrap();
        assert_eq!(anonymous_identity(mapped), "ip:192.0.2.7");
    }

    #[test]
    fn trusted_proxy_ranges_cover_docker_networks() {
        let trusted = ["172.16.0.0/12".parse::<IpNet>().unwrap()];
        let proxy: IpAddr = "172.18.0.5".parse().unwrap();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.9".parse().unwrap());
        assert_eq!(
            client_ip(proxy, &headers, &trusted).unwrap(),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
        let outsider: IpAddr = "198.51.100.1".parse().unwrap();
        assert_eq!(client_ip(outsider, &headers, &trusted).unwrap(), outsider);
    }

    #[test]
    fn forwarded_ips_require_a_trusted_peer_and_ignore_spoofed_leftmost_addresses() {
        let proxy: IpAddr = "127.0.0.1".parse().unwrap();
        let trusted = [IpNet::from(proxy)];
        let client: IpAddr = "192.0.2.10".parse().unwrap();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1, 192.0.2.10".parse().unwrap(),
        );
        assert_eq!(client_ip(client, &headers, &trusted).unwrap(), client);
        assert_eq!(client_ip(proxy, &headers, &trusted).unwrap(), client);
        headers.insert("x-forwarded-for", "invalid".parse().unwrap());
        assert_eq!(
            client_ip(proxy, &headers, &trusted),
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

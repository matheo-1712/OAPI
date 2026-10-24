use crate::{
    actions::{
        friends_actions::{self, FriendError},
        rom_auth_actions::{self, RomAuthError},
    },
    models::{
        JwtClaims,
        rom_auth::{RomCodeResponse, RomDeviceList, RomTokenRequest, RomTokenResponse},
    },
    utils::pocketbase::PocketbaseClient,
};
use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

/// Failed code exchanges tolerated per window, across every caller.
const EXCHANGE_FAILURE_LIMIT: u32 = 30;
const EXCHANGE_FAILURE_WINDOW: Duration = Duration::from_secs(60);

/// Process-wide budget of failed code exchanges (fixed window), against a
/// brute force of the 6-digit code spread over many IPs.
/// Each attempt reserves a slot up front, so concurrent guesses cannot overshoot;
/// a successful (or upstream-failed) exchange gives its slot back.
struct FailureBudget {
    window: Duration,
    limit: u32,
    start: Option<Instant>,
    used: u32,
}

impl FailureBudget {
    const fn new(window: Duration, limit: u32) -> Self {
        Self {
            window,
            limit,
            start: None,
            used: 0,
        }
    }

    /// Reserves one failure slot. Returns the window it belongs to, or the
    /// seconds until the window resets when the budget is exhausted.
    fn reserve(&mut self, now: Instant) -> Result<Instant, u64> {
        let start = match self.start {
            Some(start) if now.duration_since(start) < self.window => start,
            _ => {
                self.start = Some(now);
                self.used = 0;
                now
            }
        };
        if self.used >= self.limit {
            let left = self.window.saturating_sub(now.duration_since(start));
            return Err((left.as_secs() + u64::from(left.subsec_nanos() > 0)).max(1));
        }
        self.used += 1;
        Ok(start)
    }

    /// Gives back a slot reserved in `window` (no-op once that window is over).
    fn release(&mut self, window: Instant) {
        if self.start == Some(window) {
            self.used = self.used.saturating_sub(1);
        }
    }
}

static EXCHANGE_FAILURES: Mutex<FailureBudget> = Mutex::new(FailureBudget::new(
    EXCHANGE_FAILURE_WINDOW,
    EXCHANGE_FAILURE_LIMIT,
));

fn exchange_budget() -> std::sync::MutexGuard<'static, FailureBudget> {
    EXCHANGE_FAILURES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[derive(Deserialize)]
pub struct RomCodeQuery {
    pub label: Option<String>,
}

fn unavailable(error: impl std::fmt::Display) -> (StatusCode, String) {
    tracing::error!("ROM link operation failed: {}", error);
    (
        StatusCode::BAD_GATEWAY,
        "Liaison ROM temporairement indisponible.".into(),
    )
}

fn failure(error: RomAuthError) -> (StatusCode, String) {
    match error {
        RomAuthError::Invalid(message) => (StatusCode::BAD_REQUEST, message.into()),
        RomAuthError::NotFound(message) => (StatusCode::NOT_FOUND, message.into()),
        RomAuthError::Upstream(error) => unavailable(error),
    }
}

fn friend_failure(error: FriendError) -> (StatusCode, String) {
    match error {
        FriendError::Invalid(message) => (StatusCode::BAD_REQUEST, message.into()),
        FriendError::NotFound(message) => (StatusCode::NOT_FOUND, message.into()),
        FriendError::Conflict(message) => (StatusCode::CONFLICT, message.into()),
        FriendError::Forbidden => (
            StatusCode::FORBIDDEN,
            "Ce compte ne vous appartient pas.".into(),
        ),
        FriendError::Upstream(error) => unavailable(error),
    }
}

/// Authenticated PocketBase client plus the session's own Discord account.
async fn session(claims: &JwtClaims) -> Result<(PocketbaseClient, String), (StatusCode, String)> {
    let mut pb = PocketbaseClient::new();
    pb.login().await.map_err(unavailable)?;
    let account = friends_actions::resolve_account(&pb, "me", claims)
        .await
        .map_err(friend_failure)?;
    Ok((pb, account))
}

#[utoipa::path(post, path = "/auth/rom/code", tag = "ROM",
    params(("label" = Option<String>, Query, description = "Device name (trimmed, max 32 characters)")),
    responses((status = 200, body = RomCodeResponse), (status = 401, description = "Authentication required"), (status = 403, description = "Service tokens cannot link a ROM or cross-origin request"), (status = 502, description = "ROM link unavailable")))]
pub async fn create_code(
    Query(query): Query<RomCodeQuery>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<RomCodeResponse>, (StatusCode, String)> {
    let (pb, account) = session(&claims).await?;
    let label = rom_auth_actions::sanitize_label(query.label.as_deref());
    rom_auth_actions::create_code(&pb, &account, &label)
        .await
        .map(Json)
        .map_err(failure)
}

async fn exchange_with_pocketbase(code: &str) -> Result<RomTokenResponse, RomAuthError> {
    if !rom_auth_actions::valid_code(code) {
        return Err(RomAuthError::Invalid("Code invalide."));
    }
    let mut pb = PocketbaseClient::new();
    pb.login().await?;
    rom_auth_actions::exchange_code(&pb, code).await
}

#[utoipa::path(post, path = "/auth/rom/token", tag = "ROM",
    request_body = RomTokenRequest,
    responses((status = 200, body = RomTokenResponse, description = "Token shown once"), (status = 400, description = "Invalid code"), (status = 404, description = "Unknown or expired code"), (status = 429, description = "Too many failed exchanges across all callers, see Retry-After"), (status = 502, description = "ROM link unavailable")))]
pub async fn exchange(Json(body): Json<RomTokenRequest>) -> Response {
    let reserved = exchange_budget().reserve(Instant::now());
    let window = match reserved {
        Ok(window) => window,
        Err(seconds) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [("retry-after", seconds.to_string())],
                "Trop d'échecs de liaison. Réessayez plus tard.",
            )
                .into_response();
        }
    };
    let result = exchange_with_pocketbase(&body.code).await;
    // Only wrong codes (400/404) consume the budget.
    if !matches!(
        result,
        Err(RomAuthError::Invalid(_) | RomAuthError::NotFound(_))
    ) {
        exchange_budget().release(window);
    }
    match result {
        Ok(response) => Json(response).into_response(),
        Err(error) => failure(error).into_response(),
    }
}

#[utoipa::path(get, path = "/auth/rom/devices", tag = "ROM",
    responses((status = 200, body = RomDeviceList), (status = 401, description = "Authentication required"), (status = 403, description = "Service tokens have no devices"), (status = 502, description = "ROM link unavailable")))]
pub async fn devices(
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<RomDeviceList>, (StatusCode, String)> {
    let (pb, account) = session(&claims).await?;
    rom_auth_actions::list_devices(&pb, &account)
        .await
        .map(Json)
        .map_err(failure)
}

#[utoipa::path(post, path = "/auth/rom/devices/{device}/revoke", tag = "ROM",
    params(("device" = String, Path, description = "PocketBase discord_identity_token record ID")),
    responses((status = 204, description = "Device revoked"), (status = 400, description = "Invalid ID"), (status = 401, description = "Authentication required"), (status = 403, description = "Cross-origin request or service token"), (status = 404, description = "Device not found"), (status = 502, description = "ROM link unavailable")))]
pub async fn revoke(
    Path(device): Path<String>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<StatusCode, (StatusCode, String)> {
    let (pb, account) = session(&claims).await?;
    rom_auth_actions::revoke_device(&pb, &account, &device)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::middleware::SERVICE_SUBJECT;
    use crate::models::Role;

    #[test]
    fn failure_budget_blocks_after_the_limit_until_the_window_resets() {
        let mut budget = FailureBudget::new(Duration::from_secs(60), 30);
        let start = Instant::now();
        for _ in 0..30 {
            assert_eq!(budget.reserve(start), Ok(start));
        }
        assert_eq!(budget.reserve(start), Err(60));
        assert_eq!(
            budget.reserve(start + Duration::from_millis(20_500)),
            Err(40)
        );
        assert_eq!(
            budget.reserve(start + Duration::from_millis(59_900)),
            Err(1)
        );
        let later = start + Duration::from_secs(60);
        assert_eq!(budget.reserve(later), Ok(later));
    }

    #[test]
    fn released_attempts_do_not_count() {
        let mut budget = FailureBudget::new(Duration::from_secs(60), 30);
        let start = Instant::now();
        for _ in 0..100 {
            let window = budget.reserve(start).unwrap();
            budget.release(window);
        }
        for _ in 0..30 {
            assert!(budget.reserve(start).is_ok());
        }
        assert!(budget.reserve(start).is_err());
        // A release from a finished window never frees a slot of the current one.
        let later = start + Duration::from_secs(61);
        for _ in 0..30 {
            assert!(budget.reserve(later).is_ok());
        }
        budget.release(start);
        assert!(budget.reserve(later).is_err());
    }

    #[tokio::test]
    async fn service_token_cannot_create_a_link_code() {
        let pb = PocketbaseClient::for_test("http://127.0.0.1:9".into(), "t");
        let claims = JwtClaims {
            sub: SERVICE_SUBJECT.into(),
            username: "astro".into(),
            role: Role::Admin,
            exp: 0,
            iat: 0,
            profile: None,
        };
        let error = friends_actions::resolve_account(&pb, "me", &claims)
            .await
            .unwrap_err();
        assert!(matches!(error, FriendError::Forbidden));
        assert_eq!(friend_failure(error).0, StatusCode::FORBIDDEN);
    }
}

use crate::config::Config;
use crate::models::{
    DiscordMemberResponse, DiscordOAuthUser, DiscordUser, JwtClaims, Role, SessionProfile,
};
use crate::utils::constants::DISCORD_USERS_COLLECTION;
use crate::utils::pocketbase::PocketbaseClient;
use crate::utils::pocketbase::quote;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl, basic::BasicClient,
};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error};

/// Handles Discord OAuth2 flow, user validation via PocketBase, and JWT management.
pub struct AuthAction {
    oauth_client: BasicClient,
    reqwest_client: Client,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct OAuthState {
    nonce: String,
    return_to: String,
    /// True when the login started from the website (Astroloutre) rather than the OAPI portal.
    #[serde(default)]
    site: bool,
    /// Website origin that started the login; absent means `site_auth.url`.
    #[serde(default)]
    origin: Option<String>,
    exp: usize,
}

/// Where the browser goes back to once Discord authentication is over.
#[derive(Debug, PartialEq, Eq)]
pub struct LoginTarget {
    /// Relative path, on the OAPI portal or on the website.
    pub return_to: String,
    /// True when `return_to` belongs to the website configured in `site_auth.url`.
    pub site: bool,
    /// Configured website origin to go back to; `None` uses the primary `site_auth.url`.
    pub origin: Option<String>,
}

fn decode_oauth_state(state: &str, secret: &str) -> Option<LoginTarget> {
    if secret.trim().len() < 32 {
        return None;
    }
    let mut validation = Validation::default();
    validation.leeway = 0;
    decode::<OAuthState>(
        state,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()
    .map(|data| LoginTarget {
        return_to: data.claims.return_to,
        site: data.claims.site,
        origin: data.claims.origin,
    })
}

/// Builds the Discord CDN URL of an avatar, rejecting hashes that could alter the URL.
fn discord_avatar_url(user_id: &str, hash: &str) -> Option<String> {
    let valid = |value: &str| {
        !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    (valid(user_id) && valid(hash) && hash.len() <= 64)
        .then(|| format!("https://cdn.discordapp.com/avatars/{user_id}/{hash}.png"))
}

fn verify_with_secret(token: &str, secret: &str) -> Option<JwtClaims> {
    if secret.trim().len() < 32 {
        return None;
    }
    decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|data| data.claims)
    .filter(|_| !is_revoked(token))
}

/// Upper bound of the revocation list; expired entries are dropped first.
const MAX_REVOKED: usize = 100_000;

/// Logged-out session tokens (SHA-256) with their expiry, kept until the JWT expires.
/// In memory: a restart forgets them, which bounds the exposure to one process lifetime.
fn revoked() -> &'static Mutex<HashMap<[u8; 32], usize>> {
    static REVOKED: OnceLock<Mutex<HashMap<[u8; 32], usize>>> = OnceLock::new();
    REVOKED.get_or_init(Default::default)
}

fn token_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn now_secs() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .unwrap_or_default()
}

fn is_revoked(token: &str) -> bool {
    revoked()
        .lock()
        .map(|list| list.contains_key(&token_digest(token)))
        // A poisoned list fails closed.
        .unwrap_or(true)
}

fn revoke_until(token: &str, exp: usize) {
    let now = now_secs();
    if exp <= now {
        return;
    }
    if let Ok(mut list) = revoked().lock() {
        if list.len() >= MAX_REVOKED {
            list.retain(|_, expiry| *expiry > now);
        }
        if list.len() < MAX_REVOKED {
            list.insert(token_digest(token), exp);
        }
    }
}

/// What PocketBase currently says about a session's Discord account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountState {
    Missing,
    Member { is_admin: bool },
}

/// Applies the live account state to claims signed earlier: a removed account loses its
/// session and a revoked administrator keeps only the Normal role. A lookup failure only
/// drops administrator rights, so an outage never grants anything.
pub fn reconcile_claims(
    mut claims: JwtClaims,
    state: Result<AccountState, String>,
) -> Option<JwtClaims> {
    match state {
        Ok(AccountState::Missing) => None,
        Ok(AccountState::Member { is_admin }) => {
            if claims.role == Role::Admin && !is_admin {
                claims.role = Role::Normal;
            }
            Some(claims)
        }
        Err(error) => {
            if claims.role == Role::Admin {
                error!("Administrator session check failed: {}", error);
                claims.role = Role::Normal;
            }
            Some(claims)
        }
    }
}

/// Account states are cached briefly so protected routes do not each query PocketBase.
const ACCOUNT_STATE_TTL: Duration = Duration::from_secs(60);
const MAX_ACCOUNT_STATES: usize = 10_000;

type AccountStates = HashMap<String, (Instant, AccountState)>;

fn account_states() -> &'static Mutex<AccountStates> {
    static STATES: OnceLock<Mutex<AccountStates>> = OnceLock::new();
    STATES.get_or_init(Default::default)
}

async fn fetch_account_state(discord_id: &str) -> Result<AccountState, String> {
    if !crate::services::badge_service::valid_id(discord_id) {
        return Ok(AccountState::Missing);
    }
    let mut pb = PocketbaseClient::new();
    pb.login().await?;
    let users: crate::utils::pocketbase::ListResponse<serde_json::Value> = pb
        .list_records_with_params(
            DISCORD_USERS_COLLECTION,
            [
                ("filter", format!("discord_id = {}", quote(discord_id))),
                ("fields", "is_admin".into()),
                ("page", "1".into()),
                ("perPage", "1".into()),
            ],
        )
        .await?;
    Ok(match users.items.first() {
        None => AccountState::Missing,
        Some(user) => AccountState::Member {
            is_admin: user["is_admin"] == true,
        },
    })
}

/// Live (at most [`ACCOUNT_STATE_TTL`] old) state of a session's account.
pub async fn account_state(discord_id: &str) -> Result<AccountState, String> {
    if let Ok(states) = account_states().lock()
        && let Some((at, state)) = states.get(discord_id)
        && at.elapsed() < ACCOUNT_STATE_TTL
    {
        return Ok(*state);
    }
    let state = fetch_account_state(discord_id).await?;
    if let Ok(mut states) = account_states().lock() {
        if states.len() >= MAX_ACCOUNT_STATES {
            states.retain(|_, (at, _)| at.elapsed() < ACCOUNT_STATE_TTL);
        }
        if states.len() < MAX_ACCOUNT_STATES {
            states.insert(discord_id.to_owned(), (Instant::now(), state));
        }
    }
    Ok(state)
}

impl AuthAction {
    pub fn new() -> Self {
        let config = Config::global();
        let client_id = ClientId::new(config.auth.discord_client_id.clone());
        let client_secret = ClientSecret::new(config.auth.discord_client_secret.clone());
        let auth_url =
            AuthUrl::new("https://discord.com/api/oauth2/authorize".to_string()).unwrap();
        let token_url = TokenUrl::new("https://discord.com/api/oauth2/token".to_string()).unwrap();
        let redirect_url = RedirectUrl::new(config.auth.discord_redirect_url.clone()).unwrap();

        let oauth_client =
            BasicClient::new(client_id, Some(client_secret), auth_url, Some(token_url))
                .set_redirect_uri(redirect_url);

        Self {
            oauth_client,
            reqwest_client: crate::utils::http::shared().clone(),
        }
    }

    pub fn get_login_url(&self, target: LoginTarget) -> Result<(String, String), String> {
        let config = Config::global();
        config.validate_security()?;
        let state = OAuthState {
            nonce: CsrfToken::new_random().secret().clone(),
            return_to: target.return_to,
            site: target.site,
            origin: target.origin,
            exp: (chrono::Utc::now().timestamp() + 600) as usize,
        };
        let state = encode(
            &Header::default(),
            &state,
            &EncodingKey::from_secret(config.auth.jwt_secret.as_bytes()),
        )
        .map_err(|_| "Unable to create OAuth state".to_string())?;
        let (auth_url, _) = self
            .oauth_client
            .authorize_url(|| CsrfToken::new(state.clone()))
            .add_scope(Scope::new("identify".to_string()))
            .add_scope(Scope::new("guilds".to_string()))
            .add_scope(Scope::new("guilds.members.read".to_string()))
            .url();

        Ok((auth_url.to_string(), state))
    }

    pub fn verify_oauth_state(state: &str) -> Option<LoginTarget> {
        let config = Config::global();
        config.validate_security().ok()?;
        decode_oauth_state(state, &config.auth.jwt_secret)
    }

    pub async fn handle_callback(&self, code: String) -> Result<String, String> {
        let config = Config::global();
        config.validate_security()?;

        let token_result = self
            .oauth_client
            .exchange_code(AuthorizationCode::new(code))
            .request_async(oauth2::reqwest::async_http_client)
            .await
            .map_err(|e| format!("OAuth exchange failed: {}", e))?;

        let access_token = token_result.access_token().secret();
        debug!("OAuth exchange successful for token");

        // Get Discord User Info
        let user_info: DiscordOAuthUser = self
            .reqwest_client
            .get("https://discord.com/api/users/@me")
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| {
                error!("Request to Discord API failed: {}", e);
                format!("Request to Discord API failed: {}", e)
            })?
            .json()
            .await
            .map_err(|e| {
                error!("Failed to parse user info: {}", e);
                format!("Failed to parse user info: {}", e)
            })?;

        debug!("Fetched Discord user info for: {}", user_info.username);

        // Verify in Pocketbase
        let mut pb_client = PocketbaseClient::new();
        pb_client.login().await?;

        let filter = format!("discord_id={}", quote(&user_info.id));
        let pb_users: Vec<DiscordUser> = pb_client
            .list_all_records(DISCORD_USERS_COLLECTION, &filter)
            .await?;

        let pb_user = match pb_users.into_iter().next() {
            Some(u) => u,
            None => {
                error!("User {} not authorized in Pocketbase", user_info.username);
                return Err("User not authorized in Pocketbase".to_string());
            }
        };

        debug!(
            "User {} found in Pocketbase. is_admin: {}",
            user_info.username, pb_user.is_admin
        );

        let role = if pb_user.is_admin {
            Role::Admin
        } else {
            // Check Discord Roles for Investor
            let guild_id = &config.discord_auth.guild_id;

            if guild_id.is_empty() {
                Role::Normal
            } else {
                let member_url = format!(
                    "https://discord.com/api/users/@me/guilds/{}/member",
                    guild_id
                );

                let member_res = self
                    .reqwest_client
                    .get(&member_url)
                    .header("Authorization", format!("Bearer {}", access_token))
                    .send()
                    .await;

                if let Ok(res) = member_res {
                    if res.status().is_success() {
                        if let Ok(member_info) = res.json::<DiscordMemberResponse>().await {
                            if member_info
                                .roles
                                .contains(&config.discord_auth.investor_role_id)
                            {
                                Role::LoutreInvesti
                            } else {
                                Role::Normal
                            }
                        } else {
                            Role::Normal
                        }
                    } else {
                        Role::Normal
                    }
                } else {
                    Role::Normal
                }
            }
        };

        // Generate JWT
        let iat = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        let exp = iat + 3600 * 24 * 7; // 7 days

        // Fresh Discord avatar first, PocketBase copy as a fallback
        let avatar_url = user_info
            .avatar
            .as_deref()
            .and_then(|hash| discord_avatar_url(&user_info.id, hash))
            .or(pb_user.avatar_url);
        let claims = JwtClaims {
            sub: user_info.id,
            username: user_info.username,
            role,
            exp,
            iat,
            profile: Some(SessionProfile {
                discord_tag: pb_user.discord_tag,
                avatar_url,
            }),
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(config.auth.jwt_secret.as_bytes()),
        )
        .map_err(|e| format!("Failed to generate JWT: {}", e))?;

        Ok(token)
    }

    pub fn verify_token(token: &str) -> Option<JwtClaims> {
        let config = Config::global();
        verify_with_secret(token, &config.auth.jwt_secret)
    }

    /// Invalidates a session token server-side until it would have expired.
    pub fn revoke_token(token: &str) {
        if let Some(claims) = Self::verify_token(token) {
            revoke_until(token, claims.exp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TEST_SECRET: &str = "test-only-secret-at-least-32-bytes-long";

    fn session(role: Role) -> JwtClaims {
        JwtClaims {
            sub: "123".into(),
            username: "Loutre".into(),
            role,
            exp: now_secs() + 3600,
            iat: now_secs(),
            profile: None,
        }
    }

    #[test]
    fn revoked_tokens_stop_verifying() {
        let token = encode(
            &Header::default(),
            &session(Role::Normal),
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .unwrap();
        assert!(verify_with_secret(&token, TEST_SECRET).is_some());
        revoke_until(&token, now_secs() + 3600);
        assert!(verify_with_secret(&token, TEST_SECRET).is_none());
    }

    #[test]
    fn live_account_state_revokes_admin_rights_and_removed_accounts() {
        let admin = || session(Role::Admin);
        assert_eq!(
            reconcile_claims(admin(), Ok(AccountState::Member { is_admin: true }))
                .unwrap()
                .role,
            Role::Admin
        );
        assert_eq!(
            reconcile_claims(admin(), Ok(AccountState::Member { is_admin: false }))
                .unwrap()
                .role,
            Role::Normal
        );
        assert_eq!(
            reconcile_claims(admin(), Err("down".into())).unwrap().role,
            Role::Normal
        );
        assert!(reconcile_claims(admin(), Ok(AccountState::Missing)).is_none());
        assert_eq!(
            reconcile_claims(session(Role::LoutreInvesti), Err("down".into()))
                .unwrap()
                .role,
            Role::LoutreInvesti
        );
    }

    #[test]
    fn oauth_state_rejects_expiry_tampering_and_wrong_key() {
        let state = OAuthState {
            nonce: "random-nonce".into(),
            return_to: "/tools.html".into(),
            site: false,
            origin: None,
            exp: (chrono::Utc::now().timestamp() + 600) as usize,
        };
        let token = encode(
            &Header::default(),
            &state,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .unwrap();
        assert_eq!(
            decode_oauth_state(&token, TEST_SECRET),
            Some(LoginTarget {
                return_to: "/tools.html".into(),
                site: false,
                origin: None
            })
        );
        assert!(decode_oauth_state(&format!("{}tampered", token), TEST_SECRET).is_none());
        assert!(decode_oauth_state(&token, "another-test-secret-at-least-32-bytes").is_none());
        assert!(decode_oauth_state(&token, "").is_none());
        let expired = OAuthState { exp: 1, ..state };
        let token = encode(
            &Header::default(),
            &expired,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .unwrap();
        assert!(decode_oauth_state(&token, TEST_SECRET).is_none());
    }

    #[test]
    fn oauth_state_keeps_the_website_target() {
        let state = OAuthState {
            nonce: "random-nonce".into(),
            return_to: "/classements/discord".into(),
            site: true,
            origin: Some("https://rom-patcher.ect-adl.fr".into()),
            exp: (chrono::Utc::now().timestamp() + 600) as usize,
        };
        let token = encode(
            &Header::default(),
            &state,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .unwrap();
        assert_eq!(
            decode_oauth_state(&token, TEST_SECRET),
            Some(LoginTarget {
                return_to: "/classements/discord".into(),
                site: true,
                origin: Some("https://rom-patcher.ect-adl.fr".into())
            })
        );
    }

    #[test]
    fn discord_avatar_url_rejects_unsafe_values() {
        assert_eq!(
            discord_avatar_url("123", "a_abc123").as_deref(),
            Some("https://cdn.discordapp.com/avatars/123/a_abc123.png")
        );
        assert!(discord_avatar_url("123", "../evil").is_none());
        assert!(discord_avatar_url("123", "").is_none());
        assert!(discord_avatar_url("1/2", "abc").is_none());
    }

    #[test]
    fn test_verify_token_valid() {
        let claims = JwtClaims {
            sub: "123".to_string(),
            username: "test_user".to_string(),
            role: Role::Normal,
            exp: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600) as usize,
            iat: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as usize,
            profile: None,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .unwrap();

        let verified = verify_with_secret(&token, TEST_SECRET);
        assert!(verified.is_some());
        assert_eq!(verified.unwrap().username, "test_user");
        assert!(verify_with_secret(&token, "").is_none());
        assert!(verify_with_secret(&token, "short").is_none());
        assert!(verify_with_secret(&token, "another-test-secret-at-least-32-bytes").is_none());
    }

    #[test]
    fn test_verify_token_expired() {
        let claims = JwtClaims {
            sub: "123".to_string(),
            username: "test_user".to_string(),
            role: Role::Normal,
            exp: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 3600) as usize,
            iat: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 7200) as usize,
            profile: None,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .unwrap();

        let verified = verify_with_secret(&token, TEST_SECRET);
        assert!(verified.is_none());
    }
}

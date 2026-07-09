use crate::config::Config;
use crate::models::{DiscordMemberResponse, DiscordOAuthUser, DiscordUser, JwtClaims, Role};
use crate::utils::constants::DISCORD_USERS_COLLECTION;
use crate::utils::pocketbase::PocketbaseClient;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl, basic::BasicClient,
};
use reqwest::Client;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error};

/// Handles Discord OAuth2 flow, user validation via PocketBase, and JWT management.
pub struct AuthAction {
    oauth_client: BasicClient,
    reqwest_client: Client,
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
            reqwest_client: Client::new(),
        }
    }

    pub fn get_login_url(&self) -> String {
        let (auth_url, _csrf_token) = self
            .oauth_client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("identify".to_string()))
            .add_scope(Scope::new("guilds".to_string()))
            .add_scope(Scope::new("guilds.members.read".to_string()))
            .url();

        auth_url.to_string()
    }

    pub async fn handle_callback(&self, code: String) -> Result<String, String> {
        let config = Config::global();

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

        let filter = format!("discord_id='{}'", user_info.id);
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

        let claims = JwtClaims {
            sub: user_info.id,
            username: user_info.username,
            role,
            exp,
            iat,
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
        let mut validation = Validation::default();
        validation.validate_exp = true;
        match decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(config.auth.jwt_secret.as_bytes()),
            &validation,
        ) {
            Ok(token_data) => Some(token_data.claims),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    #[test]
    fn test_verify_token_valid() {
        let _ = dotenvy::dotenv();
        config::init();
        let config = Config::global();
        
        let claims = JwtClaims {
            sub: "123".to_string(),
            username: "test_user".to_string(),
            role: Role::Normal,
            exp: (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 3600) as usize,
            iat: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as usize,
        };
        
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(config.auth.jwt_secret.as_bytes()),
        ).unwrap();
        
        let verified = AuthAction::verify_token(&token);
        assert!(verified.is_some());
        assert_eq!(verified.unwrap().username, "test_user");
    }
    
    #[test]
    fn test_verify_token_expired() {
        let _ = dotenvy::dotenv();
        config::init();
        let config = Config::global();
        
        let claims = JwtClaims {
            sub: "123".to_string(),
            username: "test_user".to_string(),
            role: Role::Normal,
            exp: (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 3600) as usize,
            iat: (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 7200) as usize,
        };
        
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(config.auth.jwt_secret.as_bytes()),
        ).unwrap();
        
        let verified = AuthAction::verify_token(&token);
        assert!(verified.is_none());
    }
}

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The role/accreditation level of a user.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq, Eq)]
pub enum Role {
    Admin,
    LoutreInvesti,
    Normal,
}

/// The claims encoded in the JWT token.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    /// Discord ID of the user.
    pub sub: String,
    /// Username of the user.
    pub username: String,
    /// Role of the user.
    pub role: Role,
    /// Expiration timestamp.
    pub exp: usize,
    /// Issued at timestamp.
    pub iat: usize,
}

/// Structure representing a user's Discord profile from OAuth2 API.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DiscordOAuthUser {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
}

/// Structure representing a user's membership in a Discord guild.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DiscordMemberResponse {
    pub user: Option<DiscordOAuthUser>,
    pub roles: Vec<String>,
}

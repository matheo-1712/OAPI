//! Data models for friendships between Discord users (`discord_friendships` collection).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum FriendshipStatus {
    Pending,
    Accepted,
}

/// Raw PocketBase record. `user_a < user_b` so the unique (user_a, user_b) index covers both directions.
#[derive(Deserialize, Clone)]
pub struct Friendship {
    pub id: String,
    pub user_a: String,
    pub user_b: String,
    pub requested_by: String,
    pub status: FriendshipStatus,
    #[serde(default)]
    pub accepted_at: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub expand: Option<FriendshipExpand>,
}

#[derive(Deserialize, Clone, Default)]
pub struct FriendshipExpand {
    pub user_a: Option<FriendUser>,
    pub user_b: Option<FriendUser>,
}

/// Public profile of the other member of a friendship.
#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct FriendUser {
    /// PocketBase discord_users record ID.
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub discord_id: String,
    #[serde(default)]
    pub discord_tag: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct Friend {
    pub friendship_id: String,
    pub user: FriendUser,
    pub status: FriendshipStatus,
    /// Request date for pending friendships, acceptance date otherwise.
    pub since: String,
}

#[derive(Serialize, ToSchema)]
pub struct FriendList {
    pub friends: Vec<Friend>,
    /// Requests received and awaiting this account's answer.
    pub incoming: Vec<Friend>,
    /// Requests sent by this account.
    pub outgoing: Vec<Friend>,
}

#[derive(Serialize, ToSchema)]
pub struct FriendshipResponse {
    pub friendship_id: String,
    pub status: FriendshipStatus,
}

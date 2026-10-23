//! Friendship logic between Discord accounts stored in PocketBase.

use crate::models::discord_friends::{
    Friend, FriendList, FriendUser, Friendship, FriendshipResponse, FriendshipStatus,
};
use crate::models::{JwtClaims, Role};
use crate::services::badge_service::valid_id;
use crate::utils::constants::{DISCORD_FRIENDSHIPS_COLLECTION, DISCORD_USERS_COLLECTION};
use crate::utils::pocketbase::{ListResponse, PocketbaseClient};
use serde_json::json;

pub enum FriendError {
    Invalid(&'static str),
    NotFound(&'static str),
    Forbidden,
    Conflict(&'static str),
    Upstream(String),
}

impl From<String> for FriendError {
    fn from(error: String) -> Self {
        Self::Upstream(error)
    }
}

async fn find_user(pb: &PocketbaseClient, filter: &str) -> Result<Option<FriendUser>, String> {
    let users: Vec<FriendUser> = pb
        .list_all_records(DISCORD_USERS_COLLECTION, filter)
        .await?;
    Ok(users.into_iter().next())
}

/// Resolve `me` from the session, or an explicit record ID the caller owns (Admin and service: any).
pub async fn resolve_account(
    pb: &PocketbaseClient,
    account: &str,
    claims: &JwtClaims,
) -> Result<String, FriendError> {
    if account == "me" {
        if !valid_id(&claims.sub) {
            return Err(FriendError::Forbidden);
        }
        return find_user(pb, &format!("discord_id = '{}'", claims.sub))
            .await?
            .map(|user| user.id)
            .ok_or(FriendError::NotFound(
                "Aucun compte Discord lié à cette session.",
            ));
    }
    if !valid_id(account) {
        return Err(FriendError::Invalid("Invalid record ID"));
    }
    let user = find_user(pb, &format!("id = '{account}'"))
        .await?
        .ok_or(FriendError::NotFound("Compte introuvable."))?;
    if claims.role != Role::Admin && user.discord_id != claims.sub {
        return Err(FriendError::Forbidden);
    }
    Ok(user.id)
}

/// Stored order of a pair, matching the unique (user_a, user_b) index.
fn pair<'a>(first: &'a str, second: &'a str) -> (&'a str, &'a str) {
    if first < second {
        (first, second)
    } else {
        (second, first)
    }
}

async fn find_pair(
    pb: &PocketbaseClient,
    account: &str,
    target: &str,
) -> Result<Option<Friendship>, String> {
    let (user_a, user_b) = pair(account, target);
    let records: Vec<Friendship> = pb
        .list_all_records(
            DISCORD_FRIENDSHIPS_COLLECTION,
            &format!("user_a = '{user_a}' && user_b = '{user_b}'"),
        )
        .await?;
    Ok(records.into_iter().next())
}

fn validate_target(account: &str, target: &str) -> Result<(), FriendError> {
    if !valid_id(target) {
        return Err(FriendError::Invalid("Invalid record ID"));
    }
    if target == account {
        return Err(FriendError::Invalid(
            "Impossible de s'ajouter soi-même en ami.",
        ));
    }
    Ok(())
}

pub async fn list(pb: &PocketbaseClient, account: &str) -> Result<FriendList, String> {
    let filter = format!("user_a = '{account}' || user_b = '{account}'");
    let mut records = Vec::new();
    for page in 1..=20u32 {
        let response: ListResponse<Friendship> = pb
            .list_records_with_params(
                DISCORD_FRIENDSHIPS_COLLECTION,
                [
                    ("filter", filter.clone()),
                    ("expand", "user_a,user_b".into()),
                    ("sort", "-updated".into()),
                    ("page", page.to_string()),
                    ("perPage", "500".into()),
                ],
            )
            .await?;
        records.extend(response.items);
        if page >= response.total_pages {
            break;
        }
    }

    let mut list = FriendList {
        friends: Vec::new(),
        incoming: Vec::new(),
        outgoing: Vec::new(),
    };
    for record in records {
        let expand = record.expand.unwrap_or_default();
        let (other_id, other) = if record.user_a == account {
            (record.user_b, expand.user_b)
        } else {
            (record.user_a, expand.user_a)
        };
        // A deleted account keeps its ID so the friendship can still be removed.
        let user = other.unwrap_or(FriendUser {
            id: other_id,
            ..Default::default()
        });
        let friend = Friend {
            friendship_id: record.id,
            user,
            status: record.status,
            since: if record.status == FriendshipStatus::Accepted {
                record.accepted_at
            } else {
                record.created
            },
        };
        match record.status {
            FriendshipStatus::Accepted => list.friends.push(friend),
            FriendshipStatus::Pending if record.requested_by == account => {
                list.outgoing.push(friend)
            }
            FriendshipStatus::Pending => list.incoming.push(friend),
        }
    }
    Ok(list)
}

async fn mark_accepted(
    pb: &PocketbaseClient,
    friendship: &Friendship,
) -> Result<FriendshipResponse, String> {
    pb.update_record(
        DISCORD_FRIENDSHIPS_COLLECTION,
        &friendship.id,
        &json!({
            "status": FriendshipStatus::Accepted,
            "accepted_at": chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3fZ").to_string(),
        }),
    )
    .await?;
    Ok(FriendshipResponse {
        friendship_id: friendship.id.clone(),
        status: FriendshipStatus::Accepted,
    })
}

/// Send a request; answering a pending request from the target accepts it instead.
pub async fn request(
    pb: &PocketbaseClient,
    account: &str,
    target: &str,
) -> Result<FriendshipResponse, FriendError> {
    validate_target(account, target)?;
    match find_pair(pb, account, target).await? {
        Some(existing) if existing.status == FriendshipStatus::Accepted => {
            Err(FriendError::Conflict("Vous êtes déjà amis."))
        }
        Some(existing) if existing.requested_by == account => {
            Err(FriendError::Conflict("Demande d'ami déjà envoyée."))
        }
        Some(existing) => Ok(mark_accepted(pb, &existing).await?),
        None => {
            if find_user(pb, &format!("id = '{target}'")).await?.is_none() {
                return Err(FriendError::NotFound("Utilisateur introuvable."));
            }
            let (user_a, user_b) = pair(account, target);
            let created = pb
                .create_record(
                    DISCORD_FRIENDSHIPS_COLLECTION,
                    &json!({
                        "user_a": user_a,
                        "user_b": user_b,
                        "requested_by": account,
                        "status": FriendshipStatus::Pending,
                    }),
                )
                .await?;
            Ok(FriendshipResponse {
                friendship_id: created["id"].as_str().unwrap_or_default().to_owned(),
                status: FriendshipStatus::Pending,
            })
        }
    }
}

/// Accept a pending request sent by `target` to `account`.
pub async fn accept(
    pb: &PocketbaseClient,
    account: &str,
    target: &str,
) -> Result<FriendshipResponse, FriendError> {
    validate_target(account, target)?;
    match find_pair(pb, account, target).await? {
        Some(existing)
            if existing.status == FriendshipStatus::Pending && existing.requested_by == target =>
        {
            Ok(mark_accepted(pb, &existing).await?)
        }
        _ => Err(FriendError::NotFound(
            "Aucune demande d'ami reçue de cet utilisateur.",
        )),
    }
}

/// Remove a friend, decline a received request or cancel a sent one.
pub async fn remove(pb: &PocketbaseClient, account: &str, target: &str) -> Result<(), FriendError> {
    validate_target(account, target)?;
    let existing = find_pair(pb, account, target)
        .await?
        .ok_or(FriendError::NotFound(
            "Aucune relation avec cet utilisateur.",
        ))?;
    pb.delete_record(DISCORD_FRIENDSHIPS_COLLECTION, &existing.id)
        .await?;
    Ok(())
}

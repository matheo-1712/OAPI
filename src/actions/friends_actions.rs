//! Friendship logic between Discord accounts stored in PocketBase.

use crate::models::discord_friends::{
    Friend, FriendList, FriendUser, Friendship, FriendshipResponse, FriendshipStatus,
};
use crate::models::{JwtClaims, Role};
use crate::services::badge_service::valid_id;
use crate::utils::constants::{DISCORD_FRIENDSHIPS_COLLECTION, DISCORD_USERS_COLLECTION};
use crate::utils::pocketbase::quote;
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
        return find_user(pb, &format!("discord_id = {}", quote(&claims.sub)))
            .await?
            .map(|user| user.id)
            .ok_or(FriendError::NotFound(
                "Aucun compte Discord lié à cette session.",
            ));
    }
    if !valid_id(account) {
        return Err(FriendError::Invalid("Invalid record ID"));
    }
    let user = find_user(pb, &format!("id = {}", quote(account)))
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
            &format!("user_a = {} && user_b = {}", quote(user_a), quote(user_b)),
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
    let filter = format!("user_a = {} || user_b = {}", quote(account), quote(account));
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

/// Normalize modern Discord usernames before using them in a PocketBase filter.
pub fn normalize_discord_tag(value: &str) -> Result<String, FriendError> {
    let value = value.trim();
    let tag = value
        .strip_prefix('@')
        .unwrap_or(value)
        .to_ascii_lowercase();
    if !(2..=32).contains(&tag.len())
        || tag.contains("..")
        || !tag
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'_')
    {
        return Err(FriendError::Invalid("Tag Discord invalide."));
    }
    Ok(tag)
}

/// The request route accepts Discord tags exclusively, never record IDs.
pub async fn request(
    pb: &PocketbaseClient,
    account: &str,
    discord_tag: &str,
) -> Result<FriendshipResponse, FriendError> {
    let tag = normalize_discord_tag(discord_tag)?;
    let response: ListResponse<FriendUser> = pb
        .list_records_with_params(
            DISCORD_USERS_COLLECTION,
            [
                ("filter", format!("discord_tag = {}", quote(&tag))),
                ("page", "1".into()),
                ("perPage", "2".into()),
                ("fields", "id".into()),
            ],
        )
        .await?;
    if response.total_items > 1 || response.items.len() > 1 {
        return Err(FriendError::Conflict(
            "Plusieurs comptes correspondent à ce tag Discord.",
        ));
    }
    let target = response
        .items
        .into_iter()
        .next()
        .ok_or(FriendError::NotFound(
            "Aucun membre OAPI ne correspond à ce tag Discord.",
        ))?;
    request_to_user(pb, account, &target.id).await
}

/// Answering a pending request from the resolved target accepts it instead.
async fn request_to_user(
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
            if find_user(pb, &format!("id = {}", quote(target)))
                .await?
                .is_none()
            {
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

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    const USERS: &str = "/api/collections/discord_users/records";
    const FRIENDSHIPS: &str = "/api/collections/discord_friendships/records";

    fn page(items: serde_json::Value) -> String {
        let count = items.as_array().unwrap().len();
        json!({"page":1,"perPage":2,"totalPages":1,"totalItems":count,"items":items}).to_string()
    }

    #[test]
    fn discord_tags_are_normalized_and_filter_injection_is_rejected() {
        assert_eq!(
            normalize_discord_tag(" @RereBleue ").ok().as_deref(),
            Some("rerebleue")
        );
        assert_eq!(
            normalize_discord_tag("loutre.bleue_1").ok().as_deref(),
            Some("loutre.bleue_1")
        );
        for tag in [
            "",
            "@",
            "a",
            "a/b",
            "hello world",
            "a..b",
            "rere' || true",
            "étoile",
        ] {
            assert!(normalize_discord_tag(tag).is_err(), "{tag}");
        }
        assert!(normalize_discord_tag(&"a".repeat(33)).is_err());
    }

    #[tokio::test]
    async fn request_looks_up_only_the_discord_tag_then_creates_the_relationship() {
        let mut server = Server::new_async().await;
        let target = server
            .mock("GET", USERS)
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("filter".into(), "discord_tag = 'rerebleue'".into()),
                Matcher::UrlEncoded("perPage".into(), "2".into()),
            ]))
            .with_status(200)
            .with_body(page(json!([{"id":"target123","discord_tag":"rerebleue"}])))
            .create_async()
            .await;
        let pair = server
            .mock("GET", FRIENDSHIPS)
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "user_a = 'account123' && user_b = 'target123'".into(),
            ))
            .with_status(200)
            .with_body(page(json!([])))
            .create_async()
            .await;
        let exists = server
            .mock("GET", USERS)
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "id = 'target123'".into(),
            ))
            .with_status(200)
            .with_body(page(json!([{"id":"target123"}])))
            .create_async()
            .await;
        let create = server.mock("POST", FRIENDSHIPS)
            .match_body(Matcher::PartialJson(json!({"user_a":"account123","user_b":"target123","requested_by":"account123","status":"pending"})))
            .with_status(200).with_body(json!({"id":"relation123"}).to_string()).create_async().await;
        let pb = PocketbaseClient::for_test(server.url(), "test");
        let result = request(&pb, "account123", " @RereBleue ")
            .await
            .ok()
            .unwrap();
        assert_eq!(result.friendship_id, "relation123");
        assert_eq!(result.status, FriendshipStatus::Pending);
        target.assert_async().await;
        pair.assert_async().await;
        exists.assert_async().await;
        create.assert_async().await;
    }

    #[tokio::test]
    async fn internal_record_ids_are_never_used_as_a_fallback_for_requests() {
        let mut server = Server::new_async().await;
        let lookup = server
            .mock("GET", USERS)
            .match_query(Matcher::UrlEncoded(
                "filter".into(),
                "discord_tag = 'record123'".into(),
            ))
            .with_status(200)
            .with_body(page(json!([])))
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "test");
        assert!(matches!(
            request(&pb, "account123", "record123").await,
            Err(FriendError::NotFound(_))
        ));
        lookup.assert_async().await;
    }

    #[tokio::test]
    async fn tag_requests_preserve_reciprocal_acceptance_and_duplicate_rejection() {
        for (status, requester, accepted) in [
            ("pending", "target123", true),
            ("pending", "account123", false),
            ("accepted", "target123", false),
        ] {
            let mut server = Server::new_async().await;
            let lookup = server
                .mock("GET", USERS)
                .match_query(Matcher::UrlEncoded(
                    "filter".into(),
                    "discord_tag = 'rerebleue'".into(),
                ))
                .with_status(200)
                .with_body(page(json!([{"id":"target123"}])))
                .create_async()
                .await;
            let pair = server.mock("GET", FRIENDSHIPS)
                .match_query(Matcher::UrlEncoded("filter".into(), "user_a = 'account123' && user_b = 'target123'".into()))
                .with_status(200).with_body(page(json!([{"id":"relation123","user_a":"account123","user_b":"target123","status":status,"requested_by":requester}]))).create_async().await;
            let patch = server
                .mock("PATCH", format!("{FRIENDSHIPS}/relation123").as_str())
                .match_body(Matcher::PartialJson(json!({"status":"accepted"})))
                .with_status(200)
                .with_body("{}")
                .expect(usize::from(accepted))
                .create_async()
                .await;
            let pb = PocketbaseClient::for_test(server.url(), "test");
            let result = request(&pb, "account123", "rerebleue").await;
            if accepted {
                assert_eq!(result.ok().unwrap().status, FriendshipStatus::Accepted);
            } else {
                assert!(matches!(result, Err(FriendError::Conflict(_))));
            }
            lookup.assert_async().await;
            pair.assert_async().await;
            patch.assert_async().await;
        }
    }

    #[tokio::test]
    async fn duplicate_tags_and_self_requests_are_rejected_without_creating_a_relationship() {
        for (users, is_self) in [
            (json!([{"id":"target1"},{"id":"target2"}]), false),
            (json!([{"id":"account123"}]), true),
        ] {
            let mut server = Server::new_async().await;
            let lookup = server
                .mock("GET", USERS)
                .match_query(Matcher::UrlEncoded(
                    "filter".into(),
                    "discord_tag = 'rerebleue'".into(),
                ))
                .with_status(200)
                .with_body(page(users))
                .create_async()
                .await;
            let pb = PocketbaseClient::for_test(server.url(), "test");
            let result = request(&pb, "account123", "rerebleue").await;
            if is_self {
                assert!(matches!(result, Err(FriendError::Invalid(_))));
            } else {
                assert!(matches!(result, Err(FriendError::Conflict(_))));
            }
            lookup.assert_async().await;
        }
    }
}

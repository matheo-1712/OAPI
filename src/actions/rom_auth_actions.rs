//! Pure helpers for linking a game ROM to a Discord account (code -> `rom_` token).

use crate::models::rom_auth::{
    IdentityTokenRecord, RomCodeResponse, RomDevice, RomDeviceList, RomTokenResponse,
};
use crate::models::{JwtClaims, Role};
use crate::services::badge_service::valid_id;
use crate::utils::constants::{DISCORD_IDENTITY_TOKEN_COLLECTION, DISCORD_USERS_COLLECTION};
use crate::utils::pocketbase::PocketbaseClient;
use rand::rngs::OsRng;
use rand::{Rng, RngCore};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

pub const ROM_TOKEN_PREFIX: &str = "rom_";
pub const CODE_TTL_MINUTES: i64 = 15;

/// Generates a 6-character linking code, each character in `1..=9`.
pub fn generate_code() -> String {
    (0..6)
        .map(|_| char::from(b'0' + OsRng.gen_range(1..=9u8)))
        .collect()
}

/// A valid code is exactly 6 characters, each in `1..=9`.
pub fn valid_code(code: &str) -> bool {
    code.len() == 6 && code.bytes().all(|c| (b'1'..=b'9').contains(&c))
}

/// Generates `rom_` + 64 lowercase hex chars (32 random bytes).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let mut token = String::from(ROM_TOKEN_PREFIX);
    for b in bytes {
        token.push_str(&format!("{b:02x}"));
    }
    token
}

/// Lowercase hex SHA-256 of the full token (prefix included).
pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Extracts the full `rom_...` token from a raw `Authorization` header value.
pub fn parse_rom_bearer(value: &str) -> Option<&str> {
    let token = value.strip_prefix("Bearer ")?;
    let hex = token.strip_prefix(ROM_TOKEN_PREFIX)?;
    let ok = hex.len() == 64 && hex.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
    ok.then_some(token)
}

/// Trims, removes control characters and truncates to 32 characters.
pub fn sanitize_label(label: Option<&str>) -> String {
    label
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(32)
        .collect()
}

#[derive(Debug)]
pub enum RomAuthError {
    Invalid(&'static str),
    NotFound(&'static str),
    Upstream(String),
}

impl From<String> for RomAuthError {
    fn from(error: String) -> Self {
        Self::Upstream(error)
    }
}

const PB_DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3fZ";

async fn find_record(
    pb: &PocketbaseClient,
    filter: &str,
) -> Result<Option<IdentityTokenRecord>, String> {
    let records: Vec<IdentityTokenRecord> = pb
        .list_all_records(DISCORD_IDENTITY_TOKEN_COLLECTION, filter)
        .await?;
    Ok(records.into_iter().next())
}

/// Creates a pending linking code for `account`, replacing its previous pending rows.
pub async fn create_code(
    pb: &PocketbaseClient,
    account: &str,
    label: &str,
) -> Result<RomCodeResponse, RomAuthError> {
    if !valid_id(account) {
        return Err(RomAuthError::Invalid("Identifiant invalide."));
    }
    let stale: Vec<IdentityTokenRecord> = pb
        .list_all_records(
            DISCORD_IDENTITY_TOKEN_COLLECTION,
            &format!(
                "token = '' && (discord_user = '{account}' || linking_code_expires_at < @now)"
            ),
        )
        .await?;
    for row in stale {
        pb.delete_record(DISCORD_IDENTITY_TOKEN_COLLECTION, &row.id)
            .await?;
    }

    let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(CODE_TTL_MINUTES))
        .format(PB_DATE_FORMAT)
        .to_string();
    for _ in 0..5 {
        let code = generate_code();
        if find_record(pb, &format!("linking_code = '{code}'"))
            .await?
            .is_some()
        {
            continue;
        }
        let created = pb
            .create_record(
                DISCORD_IDENTITY_TOKEN_COLLECTION,
                &json!({
                    "discord_user": account,
                    "label": label,
                    "linking_code": code,
                    "linking_code_expires_at": expires_at,
                    "token": "",
                    "is_disabled": false,
                }),
            )
            .await?;
        let device_id = created
            .get("id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| RomAuthError::Upstream("Missing id in created record".into()))?
            .to_string();
        return Ok(RomCodeResponse {
            code,
            expires_at,
            device_id,
        });
    }
    Err(RomAuthError::Upstream("No free linking code".into()))
}

/// Exchanges a valid pending code for a `rom_` token; only its hash is stored.
pub async fn exchange_code(
    pb: &PocketbaseClient,
    code: &str,
) -> Result<RomTokenResponse, RomAuthError> {
    if !valid_code(code) {
        return Err(RomAuthError::Invalid("Code invalide."));
    }
    let record = find_record(
        pb,
        &format!("linking_code = '{code}' && token = '' && linking_code_expires_at > @now"),
    )
    .await?
    .ok_or(RomAuthError::NotFound("Code inconnu ou expiré."))?;
    let token = generate_token();
    pb.update_record(
        DISCORD_IDENTITY_TOKEN_COLLECTION,
        &record.id,
        &json!({
            "token": hash_token(&token),
            "linking_code": "",
            "linking_code_expires_at": "",
        }),
    )
    .await?;
    Ok(RomTokenResponse {
        token,
        device_id: record.id,
        label: record.label,
    })
}

pub async fn list_devices(
    pb: &PocketbaseClient,
    account: &str,
) -> Result<RomDeviceList, RomAuthError> {
    if !valid_id(account) {
        return Err(RomAuthError::Invalid("Identifiant invalide."));
    }
    let records: Vec<IdentityTokenRecord> = pb
        .list_all_records(
            DISCORD_IDENTITY_TOKEN_COLLECTION,
            &format!("discord_user = '{account}' && token != ''"),
        )
        .await?;
    Ok(RomDeviceList {
        devices: records
            .into_iter()
            .map(|r| RomDevice {
                id: r.id,
                label: r.label,
                created: r.created,
                updated: r.updated,
                is_disabled: r.is_disabled,
            })
            .collect(),
    })
}

pub async fn revoke_device(
    pb: &PocketbaseClient,
    account: &str,
    device: &str,
) -> Result<(), RomAuthError> {
    if !valid_id(account) || !valid_id(device) {
        return Err(RomAuthError::Invalid("Identifiant invalide."));
    }
    let record = find_record(
        pb,
        &format!("id = '{device}' && discord_user = '{account}' && token != ''"),
    )
    .await?
    .ok_or(RomAuthError::NotFound("Appareil introuvable."))?;
    pb.delete_record(DISCORD_IDENTITY_TOKEN_COLLECTION, &record.id)
        .await?;
    Ok(())
}

#[derive(Deserialize)]
struct LinkedUser {
    #[serde(default)]
    discord_id: String,
    #[serde(default)]
    username: String,
}

/// Resolves a full `rom_` token to the linked member's claims (role Normal).
pub async fn claims_for_token(
    pb: &PocketbaseClient,
    token: &str,
) -> Result<Option<JwtClaims>, String> {
    let Some(record) = find_record(
        pb,
        &format!("token = '{}' && is_disabled = false", hash_token(token)),
    )
    .await?
    else {
        return Ok(None);
    };
    if !valid_id(&record.discord_user) {
        return Ok(None);
    }
    let users: Vec<LinkedUser> = pb
        .list_all_records(
            DISCORD_USERS_COLLECTION,
            &format!("id = '{}'", record.discord_user),
        )
        .await?;
    let Some(user) = users.into_iter().next() else {
        return Ok(None);
    };
    if !valid_id(&user.discord_id) {
        return Ok(None);
    }
    Ok(Some(JwtClaims {
        sub: user.discord_id,
        username: user.username,
        role: Role::Normal,
        exp: 0,
        iat: 0,
        profile: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_six_digits_from_one_to_nine() {
        for _ in 0..200 {
            let code = generate_code();
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|c| (b'1'..=b'9').contains(&c)));
            assert!(valid_code(&code));
        }
        for bad in ["", "12345", "1234567", "123450", "12a456", " 12345"] {
            assert!(!valid_code(bad));
        }
    }

    #[test]
    fn tokens_are_prefixed_random_hex_and_hashed() {
        let token = generate_token();
        assert!(token.starts_with("rom_"));
        assert_eq!(token.len(), 4 + 64);
        assert_ne!(token, generate_token());
        let hash = hash_token(&token);
        assert_eq!(hash.len(), 64);
        assert_ne!(hash, token[4..]);
        assert_eq!(hash, hash_token(&token));
    }

    #[test]
    fn rom_bearer_requires_exact_format() {
        let token = generate_token();
        assert_eq!(
            parse_rom_bearer(&format!("Bearer {token}")),
            Some(token.as_str())
        );
        for bad in [
            token.clone(),
            format!("bearer {token}"),
            format!("Bearer {}", token.to_uppercase()),
            format!("Bearer {}x", token),
            "Bearer rom_abc".into(),
            format!("Bearer {}", &token[4..]),
        ] {
            assert_eq!(parse_rom_bearer(&bad), None, "{bad}");
        }
    }

    #[test]
    fn labels_are_trimmed_cleaned_and_bounded() {
        assert_eq!(sanitize_label(None), "");
        assert_eq!(sanitize_label(Some("  Console salon  ")), "Console salon");
        assert_eq!(sanitize_label(Some("a\nb\u{7}c")), "abc");
        assert_eq!(sanitize_label(Some(&"é".repeat(40))).chars().count(), 32);
    }

    use mockito::{Matcher, Server};
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    const COL: &str = "/api/collections/discord_identity_token/records";

    fn page(items: serde_json::Value) -> String {
        let n = items.as_array().map_or(0, |a| a.len());
        json!({"page":1,"perPage":500,"totalPages":1,"totalItems":n,"items":items}).to_string()
    }

    async fn list_mock(
        server: &mut Server,
        path: &str,
        f: &str,
        items: serde_json::Value,
    ) -> mockito::Mock {
        server
            .mock("GET", path)
            .match_query(Matcher::UrlEncoded("filter".into(), f.into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(page(items))
            .create_async()
            .await
    }

    #[tokio::test]
    async fn create_code_cleans_pending_rows_then_creates_a_fresh_code() {
        let mut server = Server::new_async().await;
        let clean = list_mock(
            &mut server,
            COL,
            "token = '' && (discord_user = 'acc' || linking_code_expires_at < @now)",
            json!([{"id":"old"}]),
        )
        .await;
        let del = server
            .mock("DELETE", format!("{COL}/old").as_str())
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let collision = server
            .mock("GET", COL)
            .match_query(Matcher::Regex("filter=linking_code".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(page(json!([])))
            .create_async()
            .await;
        let post = server
            .mock("POST", COL)
            .match_body(Matcher::PartialJson(
                json!({"discord_user":"acc","label":"PC","token":""}),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({"id":"new"}).to_string())
            .expect(1)
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        let r = create_code(&pb, "acc", "PC").await.ok().unwrap();
        assert!(valid_code(&r.code));
        assert_eq!(r.device_id, "new");
        clean.assert_async().await;
        del.assert_async().await;
        collision.assert_async().await;
        post.assert_async().await;
    }

    #[tokio::test]
    async fn exchange_rejects_expired_or_used_code() {
        let mut server = Server::new_async().await;
        let get = list_mock(
            &mut server,
            COL,
            "linking_code = '123456' && token = '' && linking_code_expires_at > @now",
            json!([]),
        )
        .await;
        let patch = server
            .mock("PATCH", Matcher::Regex(".*".into()))
            .expect(0)
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        assert!(matches!(
            exchange_code(&pb, "123456").await,
            Err(RomAuthError::NotFound(_))
        ));
        assert!(matches!(
            exchange_code(&pb, "12a456").await,
            Err(RomAuthError::Invalid(_))
        ));
        get.expect(1).assert_async().await;
        patch.assert_async().await;
    }

    #[tokio::test]
    async fn exchange_stores_only_the_hash() {
        let mut server = Server::new_async().await;
        list_mock(
            &mut server,
            COL,
            "linking_code = '123456' && token = '' && linking_code_expires_at > @now",
            json!([{"id":"dev1","label":"PC"}]),
        )
        .await;
        let body: Arc<Mutex<String>> = Arc::default();
        let captured = body.clone();
        let patch = server
            .mock("PATCH", format!("{COL}/dev1").as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body_from_request(move |req| {
                *captured.lock().unwrap() = String::from_utf8(req.body().unwrap().clone()).unwrap();
                json!({"id":"dev1"}).to_string().into_bytes()
            })
            .expect(1)
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        let r = exchange_code(&pb, "123456").await.ok().unwrap();
        patch.assert_async().await;
        let sent: serde_json::Value = serde_json::from_str(&body.lock().unwrap()).unwrap();
        let stored = sent["token"].as_str().unwrap();
        assert_eq!(stored.len(), 64);
        assert_ne!(stored, r.token);
        assert_eq!(stored, hash_token(&r.token));
        assert_eq!(sent["linking_code"], "");
        assert_eq!(r.device_id, "dev1");
        assert_eq!(r.label, "PC");
    }

    #[tokio::test]
    async fn revoke_only_deletes_own_device() {
        let mut server = Server::new_async().await;
        let get = list_mock(
            &mut server,
            COL,
            "id = 'dev1' && discord_user = 'acc' && token != ''",
            json!([]),
        )
        .await;
        let del = server
            .mock("DELETE", Matcher::Regex(".*".into()))
            .expect(0)
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        assert!(matches!(
            revoke_device(&pb, "acc", "dev1").await,
            Err(RomAuthError::NotFound(_))
        ));
        assert!(matches!(
            revoke_device(&pb, "acc", "d'1").await,
            Err(RomAuthError::Invalid(_))
        ));
        get.assert_async().await;
        del.assert_async().await;
    }

    #[tokio::test]
    async fn disabled_or_unknown_tokens_have_no_claims() {
        let token = generate_token();
        let f = format!("token = '{}' && is_disabled = false", hash_token(&token));

        let mut server = Server::new_async().await;
        list_mock(&mut server, COL, &f, json!([])).await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        assert!(claims_for_token(&pb, &token).await.unwrap().is_none());

        let mut server = Server::new_async().await;
        list_mock(
            &mut server,
            COL,
            &f,
            json!([{"id":"dev1","discord_user":"u1"}]),
        )
        .await;
        list_mock(
            &mut server,
            "/api/collections/discord_users/records",
            "id = 'u1'",
            json!([{"discord_id":"123","username":"Loutre"}]),
        )
        .await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        let claims = claims_for_token(&pb, &token).await.unwrap().unwrap();
        assert_eq!(claims.sub, "123");
        assert_eq!(claims.role, Role::Normal);
    }
}

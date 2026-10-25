//! Reading of Mojang profiles to find a player's skin texture.

use crate::services::skin_render::SkinModel;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use std::time::Duration;

/// Base URL of the Mojang session server.
pub const SESSION_SERVER_URL: &str = "https://sessionserver.mojang.com";

/// Only host Mojang textures are accepted from.
const TEXTURES_HOST: &str = "textures.minecraft.net";

/// A skin texture published on Mojang's texture server.
#[derive(Debug, PartialEq, Clone)]
pub struct SkinTexture {
    /// HTTPS URL of the texture.
    pub url: String,
    /// Texture hash (last segment of the URL).
    pub hash: String,
    pub model: SkinModel,
}

impl SkinTexture {
    /// Change marker stored in PocketBase: `v2:<hash>:<model>`.
    ///
    /// The `v2:` prefix marks skins stored as raw textures; older markers never match.
    pub fn skin_hash(&self) -> String {
        format!("v2:{}:{}", self.hash, self.model.as_str())
    }
}

/// Outcome of a profile lookup.
#[derive(Debug, PartialEq)]
pub enum ProfileLookup {
    Skin(SkinTexture),
    /// The profile exists but carries no custom skin.
    NoSkin,
    /// The profile does not exist (204 / 404).
    Unknown,
    /// Mojang answered 429.
    RateLimited,
}

#[derive(Deserialize)]
struct Profile {
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct TexturesPayload {
    #[serde(default)]
    textures: Textures,
}

#[derive(Deserialize, Default)]
struct Textures {
    #[serde(rename = "SKIN")]
    skin: Option<SkinEntry>,
}

#[derive(Deserialize)]
struct SkinEntry {
    url: String,
    metadata: Option<SkinMetadata>,
}

#[derive(Deserialize)]
struct SkinMetadata {
    model: Option<String>,
}

/// Builds the profile URL for a UUID (dashes removed).
pub fn profile_url(base_url: &str, uuid: &str) -> String {
    format!(
        "{}/session/minecraft/profile/{}",
        base_url.trim_end_matches('/'),
        uuid.replace('-', "")
    )
}

/// Validates a texture URL and returns its HTTPS form and hash.
fn parse_texture_url(raw: &str) -> Result<(String, String), String> {
    let rest = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("https://"))
        .ok_or_else(|| "texture URL has an unsupported scheme".to_string())?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if host != TEXTURES_HOST {
        return Err(format!("texture served from unexpected host '{host}'"));
    }
    let hash = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if hash.is_empty() {
        return Err("texture URL has no hash".to_string());
    }
    Ok((format!("https://{rest}"), hash.to_string()))
}

fn parse_profile(body: &str) -> Result<ProfileLookup, String> {
    let profile: Profile =
        serde_json::from_str(body).map_err(|e| format!("invalid Mojang profile: {e}"))?;
    let Some(property) = profile.properties.iter().find(|p| p.name == "textures") else {
        return Ok(ProfileLookup::NoSkin);
    };
    let decoded = STANDARD
        .decode(property.value.trim())
        .map_err(|e| format!("invalid textures base64: {e}"))?;
    let payload: TexturesPayload =
        serde_json::from_slice(&decoded).map_err(|e| format!("invalid textures payload: {e}"))?;
    let Some(skin) = payload.textures.skin else {
        return Ok(ProfileLookup::NoSkin);
    };
    let (url, hash) = parse_texture_url(&skin.url)?;
    let model = match skin.metadata.and_then(|m| m.model).as_deref() {
        Some("slim") => SkinModel::Slim,
        _ => SkinModel::Classic,
    };
    Ok(ProfileLookup::Skin(SkinTexture { url, hash, model }))
}

/// Fetches the skin of a player from the Mojang session server at `base_url`.
pub async fn fetch_skin(base_url: &str, uuid: &str) -> Result<ProfileLookup, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;
    let response = client
        .get(profile_url(base_url, uuid))
        .send()
        .await
        .map_err(|e| format!("Mojang request failed: {}", e.without_url()))?;
    match response.status().as_u16() {
        200 => {
            let body = response
                .text()
                .await
                .map_err(|e| format!("failed to read Mojang response: {}", e.without_url()))?;
            parse_profile(&body)
        }
        204 | 404 => Ok(ProfileLookup::Unknown),
        429 => Ok(ProfileLookup::RateLimited),
        status => Err(format!("unexpected Mojang status {status}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::{Value, json};

    const UUID: &str = "01234567-89ab-cdef-0123-456789abcdef";
    const PATH: &str = "/session/minecraft/profile/0123456789abcdef0123456789abcdef";

    fn profile_body(textures_json: Value) -> String {
        let encoded = STANDARD.encode(json!({ "textures": textures_json }).to_string());
        json!({
            "id": "0123456789abcdef0123456789abcdef",
            "name": "Steve",
            "properties": [{ "name": "textures", "value": encoded }]
        })
        .to_string()
    }

    async fn lookup_with(status: usize, body: String) -> Result<ProfileLookup, String> {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", PATH)
            .with_status(status)
            .with_body(body)
            .create_async()
            .await;
        fetch_skin(&server.url(), UUID).await
    }

    #[test]
    fn profile_url_strips_dashes() {
        assert_eq!(
            profile_url("https://x", UUID),
            "https://x/session/minecraft/profile/0123456789abcdef0123456789abcdef"
        );
    }

    #[tokio::test]
    async fn classic_skin_is_read() {
        let body =
            profile_body(json!({"SKIN": {"url": "http://textures.minecraft.net/texture/abc"}}));
        let result = lookup_with(200, body).await.unwrap();
        assert_eq!(
            result,
            ProfileLookup::Skin(SkinTexture {
                url: "https://textures.minecraft.net/texture/abc".to_string(),
                hash: "abc".to_string(),
                model: SkinModel::Classic,
            })
        );
    }

    #[tokio::test]
    async fn texture_url_is_rewritten_to_https() {
        let body =
            profile_body(json!({"SKIN": {"url": "http://textures.minecraft.net/texture/abc"}}));
        match lookup_with(200, body).await.unwrap() {
            ProfileLookup::Skin(skin) => {
                assert!(skin.url.starts_with("https://textures.minecraft.net/"));
            }
            other => panic!("expected a skin, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn slim_skin_is_read() {
        let body = profile_body(json!({"SKIN": {
            "url": "http://textures.minecraft.net/texture/abc",
            "metadata": {"model": "slim"}
        }}));
        match lookup_with(200, body).await.unwrap() {
            ProfileLookup::Skin(skin) => {
                assert_eq!(skin.model, SkinModel::Slim);
                assert_eq!(skin.skin_hash(), "v2:abc:slim");
            }
            other => panic!("expected a skin, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_skin_is_no_skin() {
        let result = lookup_with(200, profile_body(json!({}))).await.unwrap();
        assert_eq!(result, ProfileLookup::NoSkin);
    }

    #[tokio::test]
    async fn unknown_profile() {
        for status in [204, 404] {
            let result = lookup_with(status, String::new()).await.unwrap();
            assert_eq!(result, ProfileLookup::Unknown, "status {status}");
        }
    }

    #[tokio::test]
    async fn rate_limited() {
        let result = lookup_with(429, String::new()).await.unwrap();
        assert_eq!(result, ProfileLookup::RateLimited);
    }

    #[tokio::test]
    async fn invalid_payload_is_error() {
        let body =
            json!({"properties": [{"name": "textures", "value": "pas-du-base64"}]}).to_string();
        assert!(lookup_with(200, body).await.is_err());
    }

    #[tokio::test]
    async fn texture_from_other_host_is_error() {
        let body = profile_body(json!({"SKIN": {"url": "http://evil.example/texture/abc"}}));
        assert!(lookup_with(200, body).await.is_err());
    }
}

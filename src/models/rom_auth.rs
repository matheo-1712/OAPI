//! Data models for ROM identity tokens (`discord_identity_token` collection).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response when a ROM linking code is generated.
#[derive(Serialize, ToSchema)]
pub struct RomCodeResponse {
    pub code: String,
    pub expires_at: String,
    pub device_id: String,
}

/// Request to exchange a linking code for a token.
#[derive(Deserialize, ToSchema)]
pub struct RomTokenRequest {
    pub code: String,
}

/// Response carrying the freshly minted `rom_` token (shown once).
#[derive(Serialize, ToSchema)]
pub struct RomTokenResponse {
    pub token: String,
    pub device_id: String,
    pub label: String,
}

/// A linked ROM device.
#[derive(Serialize, ToSchema)]
pub struct RomDevice {
    pub id: String,
    pub label: String,
    pub created: String,
    pub updated: String,
    pub is_disabled: bool,
}

/// List of linked ROM devices.
#[derive(Serialize, ToSchema)]
pub struct RomDeviceList {
    pub devices: Vec<RomDevice>,
}

/// Raw PocketBase record.
#[derive(Deserialize, Clone)]
#[allow(dead_code)] // token/linking_code are read by the ROM auth middleware (next task)
pub struct IdentityTokenRecord {
    pub id: String,
    #[serde(default)]
    pub discord_user: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub linking_code: String,
    #[serde(default)]
    pub is_disabled: bool,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
}

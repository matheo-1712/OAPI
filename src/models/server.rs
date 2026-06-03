use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Collection : servers
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct Server {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub server_type: String,
    pub platform: String,
    pub version: Option<String>,
    pub modpack: Option<String>,
    pub modpack_url: Option<String>,
    pub world_name: Option<String>,
    pub embed_color: Option<String>,
    pub container: Option<String>,
    pub description: Option<String>,
    pub is_enabled: bool,
    pub is_global: bool,
    pub image: Option<String>,
    pub created: String,
    pub updated: String,
}

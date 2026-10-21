use crate::{
    models::{DiscordStats, DiscordUser, discord_activity::DiscordActivity},
    services::discord_activity::aggregate,
    utils::{constants::DISCORD_USER_STATS_COLLECTION, pocketbase::PocketbaseClient},
};
use std::{
    collections::HashMap,
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Snapshot {
    pub activity: DiscordActivity,
    pub user: Option<DiscordUser>,
}
static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Snapshot)>>> = OnceLock::new();

pub async fn snapshot(id: Option<&str>) -> Result<Snapshot, String> {
    if id.is_some_and(|id| !crate::services::image_service::valid_discord_id(id)) {
        return Err("Invalid Discord ID".into());
    }
    let key = id.unwrap_or("global");
    let cache = CACHE.get_or_init(Default::default);
    let mut entries = cache.lock().await;
    entries.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(60));
    if let Some((_, data)) = entries.get(key) {
        return Ok(data.clone());
    }
    // Serialize cache misses and bound memory; HTTP timeouts also cover queueing.

    let result = if let Some(id) = id {
        let user = super::discord_actions::fetch_discord_data(id).await?;
        Snapshot {
            activity: aggregate(&user.stats)?,
            user: Some(user),
        }
    } else {
        let mut pb = PocketbaseClient::new();
        pb.login().await?;
        let values = pb
            .list_bounded_records(DISCORD_USER_STATS_COLLECTION, "")
            .await?;
        let stats = parse_stats(values)?;
        Snapshot {
            activity: aggregate(&stats)?,
            user: None,
        }
    };
    if entries.len() >= 16 {
        entries.clear();
    }
    entries.insert(key.into(), (Instant::now(), result.clone()));
    Ok(result)
}

pub fn parse_stats(values: Vec<serde_json::Value>) -> Result<Vec<DiscordStats>, String> {
    values
        .into_iter()
        .map(|mut value| {
            if let Some(object) = value.as_object_mut()
                && let Some(number) = object.get("vocal_time").filter(|v| v.is_number()).cloned()
            {
                object.insert(
                    "vocal_time".into(),
                    serde_json::Value::String(number.to_string()),
                );
            }
            serde_json::from_value(value).map_err(|e| format!("Invalid statistics: {e}"))
        })
        .collect()
}

use crate::{
    models::{DiscordStats, DiscordUser, discord_activity::DiscordActivity},
    services::discord_activity::aggregate,
    utils::{constants::DISCORD_USER_STATS_COLLECTION, pocketbase::PocketbaseClient},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Snapshot {
    pub activity: DiscordActivity,
    pub user: Option<DiscordUser>,
}
type CacheCell = Arc<Mutex<Option<(Instant, Arc<Snapshot>)>>>;
static CACHE: OnceLock<StdMutex<HashMap<String, CacheCell>>> = OnceLock::new();

fn cell_for(entries: &mut HashMap<String, CacheCell>, key: &str) -> Result<CacheCell, String> {
    if let Some(cell) = entries.get(key) {
        return Ok(Arc::clone(cell));
    }
    if entries.len() >= 16 {
        let idle = entries
            .iter()
            .find(|(_, cell)| Arc::strong_count(cell) == 1)
            .map(|(key, _)| key.clone());
        if let Some(idle) = idle {
            entries.remove(&idle);
        } else {
            return Err("Statistics cache busy".into());
        }
    }
    let cell = Arc::new(Mutex::new(None));
    entries.insert(key.into(), Arc::clone(&cell));
    Ok(cell)
}

pub async fn snapshot(id: Option<&str>) -> Result<Arc<Snapshot>, String> {
    if id.is_some_and(|id| !crate::services::image_service::valid_discord_id(id)) {
        return Err("Invalid Discord ID".into());
    }
    let cell = {
        let mut entries = CACHE
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| "Statistics cache unavailable")?;
        cell_for(&mut entries, id.unwrap_or("global"))?
    };
    // Only requests for the same account wait for its in-flight refresh.
    let mut cached = cell.lock().await;
    if let Some((at, data)) = cached.as_ref()
        && at.elapsed() < Duration::from_secs(60)
    {
        return Ok(Arc::clone(data));
    }
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
    let result = Arc::new(result);
    *cached = Some((Instant::now(), Arc::clone(&result)));
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

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[tokio::test]
    async fn refresh_locks_are_isolated_and_active_cells_cannot_be_evicted() {
        let mut entries = HashMap::new();
        let first = cell_for(&mut entries, "1").unwrap();
        let same = cell_for(&mut entries, "1").unwrap();
        assert!(Arc::ptr_eq(&first, &same));
        let second = cell_for(&mut entries, "2").unwrap();
        let _busy = first.lock().await;
        assert!(second.try_lock().is_ok());
        let mut active = vec![second];
        for key in 3..=16 {
            active.push(cell_for(&mut entries, &key.to_string()).unwrap());
        }
        assert!(cell_for(&mut entries, "17").is_err());
        assert_eq!(entries.len(), 16);
        drop(active);
        assert!(cell_for(&mut entries, "17").is_ok());
        assert!(Arc::ptr_eq(entries.get("1").unwrap(), &first));
    }
}

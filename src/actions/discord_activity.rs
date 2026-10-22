use crate::{
    models::{
        DiscordStats, DiscordUser,
        discord_activity::{DiscordActivity, PlayerActivity},
    },
    services::discord_activity::aggregate,
    utils::{
        constants::{DISCORD_USER_STATS_COLLECTION, DISCORD_USERS_COLLECTION},
        pocketbase::PocketbaseClient,
    },
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
    pub players: Vec<PlayerActivity>,
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
            players: Vec::new(),
        }
    } else {
        let mut pb = PocketbaseClient::new();
        pb.login().await?;
        let values = pb
            .list_bounded_records(DISCORD_USER_STATS_COLLECTION, "")
            .await?;
        let stats = parse_stats(values)?;
        let users = pb
            .list_bounded_records(DISCORD_USERS_COLLECTION, "")
            .await?;
        Snapshot {
            activity: aggregate(&stats)?,
            user: None,
            players: aggregate_players(users, stats)?,
        }
    };
    let result = Arc::new(result);
    *cached = Some((Instant::now(), Arc::clone(&result)));
    Ok(result)
}

#[derive(serde::Deserialize)]
struct PlayerIdentity {
    id: String,
    discord_id: String,
    username: String,
}

fn aggregate_players(
    users: Vec<PlayerIdentity>,
    stats: Vec<DiscordStats>,
) -> Result<Vec<PlayerActivity>, String> {
    let mut aliases = HashMap::new();
    let mut groups = std::collections::BTreeMap::new();
    for user in users {
        for alias in [&user.id, &user.discord_id] {
            if alias.is_empty()
                || aliases
                    .insert(alias.clone(), user.id.clone())
                    .is_some_and(|id| id != user.id)
            {
                return Err("Ambiguous Discord account relation".into());
            }
        }
        groups.insert(user.id.clone(), (Some(user), Vec::new()));
    }
    for stat in stats {
        // Unassigned legacy rows remain in global totals, but have no player.
        if stat.discord_user.trim().is_empty() {
            continue;
        }
        let id = aliases
            .get(&stat.discord_user)
            .unwrap_or(&stat.discord_user)
            .clone();
        groups
            .entry(id)
            .or_insert_with(|| (None, Vec::new()))
            .1
            .push(stat);
    }
    groups
        .into_iter()
        .map(|(id, (user, stats))| {
            Ok(PlayerActivity {
                id,
                discord_id: user.as_ref().map(|user| user.discord_id.clone()),
                username: user.map(|user| user.username),
                stats: aggregate(&stats)?,
            })
        })
        .collect()
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
    #[test]
    fn unassigned_statistics_remain_in_global_totals_without_blocking_players() {
        let rows = parse_stats(vec![
            serde_json::json!({"id":"owned", "discord_user":"account", "message_count":2, "vocal_time":1}),
            serde_json::json!({"id":"unassigned", "discord_user":"", "message_count":3, "vocal_time":2}),
            serde_json::json!({"id":"blank", "discord_user":"  ", "message_count":4, "vocal_time":3}),
        ]).unwrap();
        let totals = aggregate(&rows).unwrap();
        let players = aggregate_players(
            vec![PlayerIdentity {
                id: "account".into(),
                discord_id: "123".into(),
                username: "Alice".into(),
            }],
            rows,
        )
        .unwrap();
        assert_eq!(totals.total_messages, 9);
        assert_eq!(totals.total_vocal_hours, 6.0);
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].id, "account");
        assert_eq!(players[0].stats.total_messages, 2);
        assert_eq!(players[0].stats.total_vocal_hours, 1.0);
    }

    #[test]
    fn player_statistics_merge_aliases_and_include_inactive_and_deleted_accounts() {
        let users = vec![
            PlayerIdentity {
                id: "account".into(),
                discord_id: "123".into(),
                username: "Alice".into(),
            },
            PlayerIdentity {
                id: "inactive".into(),
                discord_id: "456".into(),
                username: "Bob".into(),
            },
        ];
        let rows = parse_stats(vec![
            serde_json::json!({"id":"a", "discord_user":"account", "message_count":2, "vocal_time":1, "date_stats":"2026-10-21"}),
            serde_json::json!({"id":"b", "discord_user":"123", "message_count":3, "vocal_time":2, "date_stats":"2026-10-21"}),
            serde_json::json!({"id":"c", "discord_user":"deleted", "message_count":7, "vocal_time":4}),
        ]).unwrap();
        let players = aggregate_players(users, rows).unwrap();
        assert_eq!(players.len(), 3);
        assert_eq!(players[0].username.as_deref(), Some("Alice"));
        assert_eq!(players[0].stats.total_messages, 5);
        assert_eq!(players[0].stats.total_vocal_hours, 3.0);
        assert_eq!(players[0].stats.daily[0].message_count, 5);
        assert_eq!(players[1].id, "deleted");
        assert!(players[1].username.is_none());
        assert_eq!(players[1].stats.undated_messages, 7);
        assert_eq!(players[2].stats.total_messages, 0);
    }

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

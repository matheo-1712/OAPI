use crate::models::{DiscordChannel, DiscordStats, DiscordVoiceConnection, discord_activity::*};
use std::collections::{BTreeMap, BTreeSet};

/// Business date of a record (`YYYY-MM-DD`), or `None` when absent or invalid.
fn business_date(raw: &str) -> Option<&str> {
    let date = raw.trim().trim_start_matches('\u{feff}');
    date.get(..10).and_then(|date| {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .ok()
            .map(|_| date)
    })
}

/// Channel total for `ch`, keeping its latest stored name.
fn channel_entry<'a>(
    totals: &'a mut BTreeMap<String, ChannelTotal>,
    ch: &DiscordChannel,
) -> &'a mut ChannelTotal {
    let entry = totals.entry(ch.id.clone()).or_insert_with(|| ChannelTotal {
        id: ch.id.clone(),
        name: ch.name.clone(),
        message_count: 0,
        vocal_hours: 0.0,
    });
    entry.name.clone_from(&ch.name);
    entry
}

/// Migration snapshot: cumulative 2025 history, not a real activity day.
const MIGRATION_DATE: &str = "2025-01-01";

pub fn aggregate(stats: &[DiscordStats]) -> Result<DiscordActivity, String> {
    aggregate_with_mentions(stats, &[])
}

/// Same as [`aggregate`], with `mentions`: `(date_stats, companion)` taken from other
/// members' records that mention this member. The bot does not fill `vocal_with`
/// symmetrically, so a day counts as shared when either member recorded the other.
pub fn aggregate_with_mentions(
    stats: &[DiscordStats],
    mentions: &[(String, DiscordVoiceConnection)],
) -> Result<DiscordActivity, String> {
    let mut days: BTreeMap<String, ActivityDay> = BTreeMap::new();
    let mut text: BTreeMap<String, ChannelTotal> = BTreeMap::new();
    let mut voice: BTreeMap<String, ChannelTotal> = BTreeMap::new();
    let mut complete = true;
    let mut companions: BTreeMap<String, (VoiceCompanion, BTreeSet<&str>)> = BTreeMap::new();
    let mut total_messages = 0i64;
    let mut total_vocal_hours = 0.0;
    let mut undated_records = 0u64;
    let mut undated_messages = 0i64;
    let mut undated_vocal_hours = 0.0;
    for stat in stats {
        // Synthetic badge-only entries do not represent activity.
        if stat.id == "dummy" {
            continue;
        }
        let hours: f64 = stat.vocal_time.parse().map_err(|_| "Invalid vocal time")?;
        if !hours.is_finite() || hours < 0.0 || stat.message_count < 0 {
            return Err("Invalid activity counters".into());
        }
        total_messages = total_messages
            .checked_add(stat.message_count)
            .ok_or("Message overflow")?;
        total_vocal_hours += hours;
        let date = business_date(&stat.date_stats);
        // Keep the migration snapshot counters in lifetime totals only.
        if date == Some(MIGRATION_DATE) {
            continue;
        }
        // A companion counts once per business day, however many rows that day holds.
        // Undated rows cannot be attributed to a day and are ignored here.
        if let Some(date) = date {
            for companion in stat.vocal_with.iter().flatten() {
                let (entry, dates) = companions.entry(companion.id.clone()).or_insert_with(|| {
                    (
                        VoiceCompanion {
                            id: companion.id.clone(),
                            username: companion.username.clone(),
                            records_together: 0,
                        },
                        BTreeSet::new(),
                    )
                });
                entry.username.clone_from(&companion.username);
                dates.insert(date);
            }
        }
        if let Some(date) = date {
            let day = days.entry(date.into()).or_insert_with(|| ActivityDay {
                date: date.into(),
                ..Default::default()
            });
            day.message_count = day
                .message_count
                .checked_add(stat.message_count)
                .ok_or("Message overflow")?;
            day.vocal_hours += hours;
        } else {
            undated_records += 1;
            undated_messages = undated_messages
                .checked_add(stat.message_count)
                .ok_or("Message overflow")?;
            undated_vocal_hours += hours;
        }
        for ch in stat.text_channels.iter().flatten() {
            let entry = channel_entry(&mut text, ch);
            match ch.nb_message {
                Some(n) if n >= 0 => {
                    entry.message_count = entry
                        .message_count
                        .checked_add(n)
                        .ok_or("Channel overflow")?
                }
                None => complete = false,
                _ => return Err("Invalid channel messages".into()),
            }
        }
        // The bot's per-channel voice counters undercount sessions, while the row's own
        // vocal_time is reliable: share it among the row's channels, all of it to a single
        // channel, otherwise pro rata of their counters (equal shares without usable ones).
        if let Some(channels) = stat.voice_channels.as_deref().filter(|c| !c.is_empty()) {
            let mut weights = Vec::with_capacity(channels.len());
            for ch in channels {
                match ch.vocal_time {
                    Some(h) if h.is_finite() && h >= 0.0 => weights.push(Some(h)),
                    None => weights.push(None),
                    _ => return Err("Invalid channel vocal time".into()),
                }
            }
            let known: f64 = weights.iter().flatten().sum();
            let pro_rata = weights.iter().all(Option::is_some) && known > 0.0;
            if channels.len() > 1 && !pro_rata {
                complete = false;
            }
            for (ch, weight) in channels.iter().zip(weights) {
                let share = match weight {
                    Some(weight) if pro_rata => weight / known,
                    _ => 1.0 / channels.len() as f64,
                };
                channel_entry(&mut voice, ch).vocal_hours += hours * share;
            }
        }
    }
    let daily: Vec<_> = days.into_values().collect();
    if !total_vocal_hours.is_finite() || voice.values().any(|c| !c.vocal_hours.is_finite()) {
        return Err("Vocal overflow".into());
    }
    // Sorted keys and strict comparison make ties deterministic (earliest date / lowest ID).
    let record_messages_day = daily
        .iter()
        .filter(|d| d.message_count > 0)
        .fold(None::<&ActivityDay>, |best, d| {
            if best.is_none_or(|b| d.message_count > b.message_count) {
                Some(d)
            } else {
                best
            }
        })
        .cloned();
    let record_vocal_day = daily
        .iter()
        .filter(|d| d.vocal_hours > 0.0)
        .fold(None::<&ActivityDay>, |best, d| {
            if best.is_none_or(|b| d.vocal_hours > b.vocal_hours) {
                Some(d)
            } else {
                best
            }
        })
        .cloned();
    let favorite_text_channel =
        text.into_values()
            .filter(|c| c.message_count > 0)
            .fold(None::<ChannelTotal>, |best, c| {
                if best
                    .as_ref()
                    .is_none_or(|b| c.message_count > b.message_count)
                {
                    Some(c)
                } else {
                    best
                }
            });
    let favorite_voice_channel = voice.into_values().filter(|c| c.vocal_hours > 0.0).fold(
        None::<ChannelTotal>,
        |best, c| {
            if best.as_ref().is_none_or(|b| c.vocal_hours > b.vocal_hours) {
                Some(c)
            } else {
                best
            }
        },
    );
    for (raw_date, companion) in mentions {
        let Some(date) = business_date(raw_date).filter(|date| *date != MIGRATION_DATE) else {
            continue;
        };
        // The member's own records name their companions best: never rename from a mention.
        let (_, dates) = companions.entry(companion.id.clone()).or_insert_with(|| {
            (
                VoiceCompanion {
                    id: companion.id.clone(),
                    username: companion.username.clone(),
                    records_together: 0,
                },
                BTreeSet::new(),
            )
        });
        dates.insert(date);
    }
    let top_voice_companion = companions
        .into_values()
        .map(|(mut companion, dates)| {
            companion.records_together = dates.len() as u64;
            companion
        })
        .fold(None::<VoiceCompanion>, |best, c| {
            if best
                .as_ref()
                .is_none_or(|b| c.records_together > b.records_together)
            {
                Some(c)
            } else {
                best
            }
        });
    Ok(DiscordActivity {
        total_messages,
        total_vocal_hours,
        undated_records,
        undated_messages,
        undated_vocal_hours,
        daily_complete: undated_records == 0,
        record_messages_day,
        record_vocal_day,
        favorite_text_channel,
        favorite_voice_channel,
        top_voice_companion,
        channel_counters_complete: complete,
        daily,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn companions_rank_by_regularity_even_when_durations_are_available() {
        let make = |date: &str, companions: serde_json::Value| {
            let mut stat = row(date, 1, 10.0, serde_json::json!([]), serde_json::json!([]));
            stat.vocal_with = Some(serde_json::from_value(companions).unwrap());
            stat
        };
        let result = aggregate(&[
            make(
                "2026-10-20",
                serde_json::json!([
                    {"id":"a","username":"Frequent","vocal_time":1.0},
                    {"id":"b","username":"Longer","vocal_time":5.0}
                ]),
            ),
            make(
                "2026-10-21",
                serde_json::json!([
                    {"id":"a","username":"Renamed","vocal_time":1.0}
                ]),
            ),
            // Several rows on the same day count as one day together.
            make(
                "2026-10-20",
                serde_json::json!([{"id":"b","username":"Longer"}]),
            ),
            make(
                "2026-10-20",
                serde_json::json!([{"id":"b","username":"Longer"}]),
            ),
            // Undated rows cannot be attributed to a day.
            make("", serde_json::json!([{"id":"b","username":"Longer"}])),
            make(
                "2025-01-01",
                serde_json::json!([
                    {"id":"old","username":"Legacy","vocal_time":999.0}
                ]),
            ),
        ])
        .unwrap();
        let top = result.top_voice_companion.unwrap();
        assert_eq!(top.id, "a");
        assert_eq!(top.username, "Renamed");
        assert_eq!(top.records_together, 2);
        let b_only = aggregate(&[
            make(
                "2026-10-20",
                serde_json::json!([{"id":"b","username":"Longer"}]),
            ),
            make(
                "2026-10-20",
                serde_json::json!([{"id":"b","username":"Longer"}]),
            ),
            make("", serde_json::json!([{"id":"b","username":"Longer"}])),
        ])
        .unwrap();
        assert_eq!(b_only.top_voice_companion.unwrap().records_together, 1);
        let undated_only = aggregate(&[make(
            "",
            serde_json::json!([{"id":"b","username":"Longer"}]),
        )])
        .unwrap();
        assert!(undated_only.top_voice_companion.is_none());
        let json = serde_json::to_value(&top).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 3);
        let unknown = aggregate(&[make(
            "2026-10-21",
            serde_json::json!([
                {"id":"a","username":"Unknown"}, {"id":"a","username":"Unknown"}
            ]),
        )])
        .unwrap();
        assert_eq!(unknown.top_voice_companion.as_ref().unwrap().id, "a");
        assert_eq!(unknown.top_voice_companion.unwrap().records_together, 1);
    }
    #[test]
    fn migration_snapshot_only_contributes_to_lifetime_totals() {
        let legacy = row(
            "2025-01-01 00:00:00.000Z",
            5211,
            307.1,
            serde_json::json!([{"id":"old", "name":"Old", "nb_message":5211}]),
            serde_json::json!([{"id":"old", "name":"Old"}]),
        );
        let modern = row(
            "2026-10-21",
            10,
            2.0,
            serde_json::json!([{"id":"new", "name":"New", "nb_message":10}]),
            serde_json::json!([{"id":"new", "name":"New", "vocal_time":2.0}]),
        );
        let result = aggregate(&[legacy.clone(), modern]).unwrap();
        assert_eq!(result.total_messages, 5221);
        assert!((result.total_vocal_hours - 309.1).abs() < 1e-9);
        assert_eq!(result.daily.len(), 1);
        assert_eq!(result.record_messages_day.unwrap().date, "2026-10-21");
        assert_eq!(result.record_vocal_day.unwrap().vocal_hours, 2.0);
        assert_eq!(result.favorite_text_channel.unwrap().id, "new");
        assert_eq!(result.favorite_voice_channel.unwrap().id, "new");
        assert!(result.channel_counters_complete);
        assert!(result.daily_complete);
        let legacy_only = aggregate(&[legacy]).unwrap();
        assert_eq!(legacy_only.total_vocal_hours, 307.1);
        assert!(legacy_only.daily.is_empty());
        assert!(legacy_only.record_vocal_day.is_none());
        assert!(legacy_only.favorite_text_channel.is_none());
    }

    #[test]
    fn available_channel_counters_produce_favorites_despite_other_missing_counters() {
        let result = aggregate(&[row("2026-10-21", 10, 2.0,
            serde_json::json!([{"id":"a", "name":"Missing"}, {"id":"b", "name":"Known", "nb_message":10}]),
            serde_json::json!([{"id":"c", "name":"Known", "vocal_time":2.0}]))]).unwrap();
        assert!(!result.channel_counters_complete);
        assert_eq!(result.favorite_text_channel.unwrap().id, "b");
        assert_eq!(result.favorite_voice_channel.unwrap().id, "c");
    }
    #[test]
    fn undated_history_preserves_totals_without_fabricating_day_records() {
        let mut rows = vec![row(
            "2026-10-21T00:00:00.000Z",
            10,
            2.0,
            serde_json::json!([]),
            serde_json::json!([]),
        )];
        for date in ["", "invalid", "2026-02-30", "2026"] {
            rows.push(row(
                date,
                3,
                0.5,
                serde_json::json!([]),
                serde_json::json!([]),
            ));
        }
        let result = aggregate(&rows).unwrap();
        assert_eq!(result.total_messages, 22);
        assert_eq!(result.total_vocal_hours, 4.0);
        assert_eq!(result.undated_records, 4);
        assert_eq!(result.undated_messages, 12);
        assert_eq!(result.undated_vocal_hours, 2.0);
        assert!(!result.daily_complete);
        assert_eq!(result.daily.len(), 1);
        assert_eq!(result.daily[0].message_count, 10);
        assert_eq!(result.record_messages_day.unwrap().date, "2026-10-21");
        assert_eq!(result.record_vocal_day.unwrap().date, "2026-10-21");
    }

    #[test]
    fn missing_and_null_dates_and_pocketbase_timestamp_formats_are_supported() {
        for date in [serde_json::Value::Null, serde_json::json!("")] {
            let rows = crate::actions::discord_activity::parse_stats(vec![serde_json::json!({"id":"legacy", "discord_user":"owner", "message_count":1, "vocal_time":1, "date_stats":date})]).unwrap();
            assert_eq!(aggregate(&rows).unwrap().undated_records, 1);
        }
        let rows = crate::actions::discord_activity::parse_stats(vec![serde_json::json!({"id":"legacy", "discord_user":"owner", "message_count":1, "vocal_time":1})]).unwrap();
        assert_eq!(aggregate(&rows).unwrap().undated_records, 1);
        for date in [
            "2026-10-21",
            "2026-10-21 00:00:00.000Z",
            "2026-10-21T00:00:00Z",
            " 2026-10-21 00:00:00 ",
        ] {
            let result = aggregate(&[row(
                date,
                1,
                1.0,
                serde_json::json!([]),
                serde_json::json!([]),
            )])
            .unwrap();
            assert!(result.daily_complete);
            assert_eq!(result.daily[0].date, "2026-10-21");
        }
    }
    fn row(
        date: &str,
        messages: i64,
        hours: f64,
        text: serde_json::Value,
        voice: serde_json::Value,
    ) -> DiscordStats {
        crate::actions::discord_activity::parse_stats(vec![serde_json::json!({
            "id":"row", "discord_user":"owner", "date_stats":date,
            "message_count":messages,"vocal_time":hours,"text_channels":text,"voice_channels":voice
        })])
        .unwrap()
        .remove(0)
    }
    #[test]
    fn weighted_channels_daily_merge_and_records() {
        let rows = vec![
            row(
                "2026-10-21 00:00:00",
                10,
                1.0,
                serde_json::json!([{"id":"a","name":"A","nb_message":1},{"id":"b","name":"B","nb_message":9}]),
                serde_json::json!([{"id":"v","name":"V","vocal_time":0.8}]),
            ),
            row(
                "2026-10-20",
                3,
                2.0,
                serde_json::json!([{"id":"a","name":"A","nb_message":3}]),
                serde_json::json!([{"id":"w","name":"W","vocal_time":2.0}]),
            ),
            row(
                "2026-10-21",
                5,
                2.0,
                serde_json::json!([{"id":"a","name":"renamed","nb_message":5}]),
                serde_json::json!([{"id":"v","name":"V","vocal_time":2.0}]),
            ),
        ];
        let stats = aggregate(&rows).unwrap();
        assert_eq!(stats.total_messages, 18);
        assert_eq!(stats.total_vocal_hours, 5.0);
        assert_eq!(stats.daily.len(), 2);
        assert_eq!(stats.record_messages_day.unwrap().message_count, 15);
        assert_eq!(stats.record_vocal_day.unwrap().date, "2026-10-21");
        assert_eq!(stats.favorite_text_channel.unwrap().id, "a");
        assert_eq!(stats.favorite_voice_channel.unwrap().id, "v");
    }
    #[test]
    fn row_vocal_time_is_shared_among_its_voice_channels() {
        let voice = |channels: serde_json::Value, hours: f64| {
            row("2026-10-21", 0, hours, serde_json::json!([]), channels)
        };
        // Hours of the favorite voice channel, when it is `id`.
        let hours = |result: &DiscordActivity, id: &str| {
            result
                .favorite_voice_channel
                .as_ref()
                .filter(|c| c.id == id)
                .map(|c| c.vocal_hours)
        };
        // A single channel receives the whole row, despite an undercounted counter.
        let single = aggregate(&[voice(
            serde_json::json!([{"id":"a","name":"A","vocal_time":0.01}]),
            0.9,
        )])
        .unwrap();
        assert_eq!(hours(&single, "a"), Some(0.9));
        assert!(single.channel_counters_complete);
        // Several channels share the row pro rata of their counters.
        let shared = aggregate(&[voice(
            serde_json::json!([{"id":"a","name":"A","vocal_time":0.3},{"id":"b","name":"B","vocal_time":0.1}]),
            2.0,
        )])
        .unwrap();
        assert!((hours(&shared, "a").unwrap() - 1.5).abs() < 1e-9);
        assert!(shared.channel_counters_complete);
        // Without usable counters, equal shares, flagged as incomplete.
        let equal = aggregate(&[voice(
            serde_json::json!([{"id":"a","name":"A"},{"id":"b","name":"B","vocal_time":0.0}]),
            2.0,
        )])
        .unwrap();
        assert_eq!(hours(&equal, "a"), Some(1.0));
        assert!(!equal.channel_counters_complete);
        // Rows without channels stay unattributed.
        let none = aggregate(&[voice(serde_json::json!([]), 2.0)]).unwrap();
        assert!(none.favorite_voice_channel.is_none());
    }

    #[test]
    fn empty_legacy_invalid_and_overflow_are_explicit() {
        let empty = aggregate(&[]).unwrap();
        assert!(empty.record_vocal_day.is_none());
        assert_eq!(empty.total_messages, 0);
        let legacy = row(
            "2026-10-21",
            1,
            1.0,
            serde_json::json!([{"id":"a","name":"A"}]),
            serde_json::json!([]),
        );
        assert!(!aggregate(&[legacy]).unwrap().channel_counters_complete);
        assert!(
            aggregate(&[row(
                "2026-10-21",
                -1,
                1.0,
                serde_json::json!([]),
                serde_json::json!([])
            )])
            .is_err()
        );
        let large = row(
            "2026-10-21",
            i64::MAX,
            1.0,
            serde_json::json!([]),
            serde_json::json!([]),
        );
        assert!(aggregate(&[large.clone(), large]).is_err());
    }
}

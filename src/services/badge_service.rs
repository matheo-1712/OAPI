use crate::utils::{constants::*, pocketbase::PocketbaseClient};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AwardRequest {
    /// PocketBase record ID, not the Minecraft UUID or Discord snowflake.
    pub recipient: String,
    pub kind: RecipientKind,
    pub badge: String,
}

#[derive(Deserialize, ToSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecipientKind {
    Player,
    DiscordUser,
}

#[derive(Serialize, ToSchema)]
pub struct AwardResult {
    pub badge: String,
    pub created: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    stat: String,
    minimum: f64,
    #[serde(default)]
    server: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BadgeDefinition {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    name_prefix: Option<String>,
    kind: RecipientKind,
    #[serde(default)]
    rule: Option<Rule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalogue {
    badges: Vec<BadgeDefinition>,
}

fn catalogue() -> Result<Vec<BadgeDefinition>, String> {
    let catalogue: Catalogue = config::Config::builder()
        .add_source(config::File::new("badges.yaml", config::FileFormat::Yaml))
        .build()
        .and_then(|c| c.try_deserialize())
        .map_err(|e| format!("Invalid badges.yaml: {e}"))?;
    let mut ids = std::collections::HashSet::new();
    for badge in &catalogue.badges {
        let selectors = [
            badge.id.as_deref(),
            badge.name.as_deref(),
            badge.name_prefix.as_deref(),
        ];
        if selectors.iter().flatten().count() != 1
            || selectors.iter().flatten().any(|s| s.trim().is_empty())
            || badge.id.as_ref().is_some_and(|id| !valid_id(id))
            || !ids.insert((
                badge.kind as u8,
                badge.id.clone(),
                badge.name.clone(),
                badge.name_prefix.clone(),
            ))
        {
            return Err("Invalid or duplicate badge ID in badges.yaml".into());
        }
        if let Some(rule) = &badge.rule {
            qualifies(rule, &[])?;
        }
    }
    Ok(catalogue.badges)
}

fn matches_badge(definition: &BadgeDefinition, badge: &Value) -> bool {
    let platform = match definition.kind {
        RecipientKind::Player => "Minecraft",
        RecipientKind::DiscordUser => "Discord",
    };
    if !badge["plateform"]
        .as_str()
        .is_some_and(|p| p.eq_ignore_ascii_case(platform))
    {
        return false;
    }
    if let Some(id) = &definition.id {
        return badge["id"].as_str() == Some(id);
    }
    if let Some(name) = &definition.name {
        return badge["name"].as_str() == Some(name);
    }
    definition.name_prefix.as_ref().is_some_and(|prefix| {
        badge["name"]
            .as_str()
            .is_some_and(|name| name.starts_with(prefix))
    })
}

fn resolve_badge<'a>(
    definition: &BadgeDefinition,
    badges: &'a [Value],
) -> Result<&'a Value, String> {
    let mut matches = badges
        .iter()
        .filter(|badge| matches_badge(definition, badge));
    let badge = matches.next().ok_or_else(|| {
        format!(
            "Configured badge not found: {:?}",
            definition
                .name
                .as_ref()
                .or(definition.name_prefix.as_ref())
                .or(definition.id.as_ref())
        )
    })?;
    if matches.next().is_some() {
        return Err("Ambiguous badge selector in badges.yaml; use its record ID".into());
    }
    Ok(badge)
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|c| c.is_ascii_alphanumeric())
}

async fn find(pb: &PocketbaseClient, collection: &str, id: &str) -> Result<Value, String> {
    if !valid_id(id) {
        return Err("Invalid record ID".into());
    }
    pb.list_all_records::<Value>(collection, &format!("id = '{id}'"))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| "Record not found".into())
}

/// All manual and automatic awards share this idempotent write path.
async fn persist(
    pb: &PocketbaseClient,
    field: &str,
    recipient: &str,
    alias: &str,
    badge: &str,
) -> Result<AwardResult, String> {
    let filter = format!("({field} = '{recipient}' || {field} = '{alias}') && badge = '{badge}'");
    if !pb
        .list_all_records::<Value>(DISCORD_USER_BADGES_COLLECTION, &filter)
        .await?
        .is_empty()
    {
        return Ok(AwardResult {
            badge: badge.into(),
            created: false,
        });
    }
    let body = json!({ field: recipient, "badge": badge });
    if let Err(error) = pb
        .create_record(DISCORD_USER_BADGES_COLLECTION, &body)
        .await
    {
        // A unique index also protects against concurrent requests on other API instances.
        if pb
            .list_all_records::<Value>(DISCORD_USER_BADGES_COLLECTION, &filter)
            .await?
            .is_empty()
        {
            return Err(error);
        }
        return Ok(AwardResult {
            badge: badge.into(),
            created: false,
        });
    }
    Ok(AwardResult {
        badge: badge.into(),
        created: true,
    })
}

pub async fn award(pb: &PocketbaseClient, request: AwardRequest) -> Result<AwardResult, String> {
    let badge = find(pb, BADGES_COLLECTION, &request.badge).await?;
    if badge["is_enabled"] != true {
        return Err("Badge disabled".into());
    }
    let definitions = catalogue()?;
    let definition = definitions
        .iter()
        .find(|d| d.kind == request.kind && matches_badge(d, &badge))
        .ok_or("Badge missing from badges.yaml")?;
    if definition.rule.is_some() {
        return Err("Use the statistics evaluation endpoint for this badge".into());
    }
    let (collection, field, alias_field) = match request.kind {
        RecipientKind::Player => (MINECRAFT_PLAYERS_COLLECTION, "player", "account_id"),
        RecipientKind::DiscordUser => (DISCORD_USERS_COLLECTION, "discord_user", "discord_id"),
    };
    let recipient = find(pb, collection, &request.recipient).await?;
    let alias = recipient[alias_field]
        .as_str()
        .filter(|id| valid_id(id))
        .unwrap_or(&request.recipient);
    persist(pb, field, &request.recipient, alias, &request.badge).await
}

fn qualifies(rule: &Rule, stats: &[Value]) -> Result<bool, String> {
    let mut parts = rule.stat.split('.');
    let field = parts.next().unwrap_or_default();
    let key = parts.next();
    let scalar = matches!(
        field,
        "playtime"
            | "deaths"
            | "mob_kills"
            | "player_kills"
            | "blocks_mined"
            | "blocks_placed"
            | "total_distance"
            | "distance_walked"
            | "distance_elytra"
            | "distance_fligth"
            | "message_count"
            | "vocal_time"
            | "voice_joined"
            | "membership"
    );
    let map = matches!(field, "mobs_killed" | "items_crafted" | "items_broken");
    if parts.next().is_some()
        || !(scalar && key.is_none() || map && key.is_some_and(|k| !k.is_empty()))
        || !rule.minimum.is_finite()
        || rule.minimum <= 0.0
    {
        return Err("Invalid badge stats_rule".into());
    }
    let mut total = 0.0;
    for row in stats.iter().filter(|row| {
        rule.server
            .as_ref()
            .is_none_or(|s| row["server"].as_str() == Some(s))
    }) {
        let value = match key {
            Some(key) => &row[field][key],
            None => &row[field],
        };
        if value.is_null() {
            continue;
        }
        let number = value
            .as_f64()
            .or_else(|| {
                (field == "vocal_time")
                    .then(|| value.as_str().and_then(|s| s.parse::<f64>().ok()))
                    .flatten()
            })
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or("Invalid stored statistic")?;
        total += number;
    }
    if !total.is_finite() {
        return Err("Statistics overflow".into());
    }
    Ok(total >= rule.minimum)
}

pub async fn evaluate(pb: &PocketbaseClient, player_id: &str) -> Result<Vec<AwardResult>, String> {
    evaluate_recipient(pb, player_id, RecipientKind::Player).await
}

pub async fn evaluate_discord(
    pb: &PocketbaseClient,
    user_id: &str,
) -> Result<Vec<AwardResult>, String> {
    evaluate_recipient(pb, user_id, RecipientKind::DiscordUser).await
}

async fn evaluate_recipient(
    pb: &PocketbaseClient,
    player_id: &str,
    kind: RecipientKind,
) -> Result<Vec<AwardResult>, String> {
    let (collection, alias_field, stats_collection, stats_field, award_field) = match kind {
        RecipientKind::Player => (
            MINECRAFT_PLAYERS_COLLECTION,
            "account_id",
            MINECRAFT_STATS_COLLECTION,
            "account_id",
            "player",
        ),
        RecipientKind::DiscordUser => (
            DISCORD_USERS_COLLECTION,
            "discord_id",
            DISCORD_USER_STATS_COLLECTION,
            "discord_user",
            "discord_user",
        ),
    };
    let player = find(pb, collection, player_id).await?;
    let uuid = player[alias_field]
        .as_str()
        .filter(|id| valid_id(id))
        .ok_or("Invalid stored account ID")?;
    let mut stats = pb
        .list_all_records::<Value>(
            stats_collection,
            &format!("{stats_field} = '{player_id}' || {stats_field} = '{uuid}'"),
        )
        .await?;
    let definitions = catalogue()?;
    if kind == RecipientKind::DiscordUser {
        // Existing voice-channel evidence covers a first join even if its duration rounded to zero.
        for row in &mut stats {
            let joined = row["voice_channels"]
                .as_array()
                .is_some_and(|channels| !channels.is_empty())
                || row["vocal_time"]
                    .as_f64()
                    .or_else(|| {
                        row["vocal_time"]
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                    })
                    .is_some_and(|hours| hours > 0.0);
            row["voice_joined"] = json!(u8::from(joined));
        }
        let joined = player["joined_at"]
            .as_str()
            .is_some_and(|date| !date.trim().is_empty());
        stats.push(json!({"membership": u8::from(joined)}));
    }
    let badges = pb.list_all_records::<Value>(BADGES_COLLECTION, "").await?;
    // Validate every rule before beginning writes.
    let mut eligible = Vec::new();
    for definition in definitions {
        if definition.kind != kind {
            continue;
        }
        let badge = resolve_badge(&definition, &badges)?;
        let id = badge["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or("Invalid badge ID")?;
        let Some(rule) = definition.rule else {
            continue;
        };
        if badge["is_enabled"] == true && qualifies(&rule, &stats)? {
            eligible.push(id.to_owned());
        }
    }
    let mut results = Vec::new();
    for badge in eligible {
        results.push(persist(pb, award_field, player_id, uuid, &badge).await?);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(stat: &str, minimum: f64) -> Rule {
        Rule {
            stat: stat.into(),
            minimum,
            server: None,
        }
    }

    #[test]
    fn discord_accepts_decimal_hour_strings_and_inclusive_thresholds() {
        let stats = vec![
            json!({"message_count": 60, "vocal_time": "40.5"}),
            json!({"message_count": 40, "vocal_time": 59.5}),
        ];
        assert!(qualifies(&rule("message_count", 100.0), &stats).unwrap());
        assert!(!qualifies(&rule("message_count", 101.0), &stats).unwrap());
        assert!(qualifies(&rule("vocal_time", 100.0), &stats).unwrap());
        assert!(!qualifies(&rule("vocal_time", 100.01), &stats).unwrap());
        assert!(qualifies(&rule("vocal_time", 1.0), &[json!({"vocal_time":"NaN"})]).is_err());
        assert!(qualifies(&rule("vocal_time", 1.0), &[json!({"vocal_time":"invalid"})]).is_err());
    }

    #[test]
    fn configured_tiers_use_graph_units_and_unlock_cumulatively() {
        let definitions = catalogue().unwrap();
        for (kind, field, thresholds) in [
            (
                RecipientKind::Player,
                "playtime",
                vec![72000.0, 3600000.0, 36000000.0, 72000000.0, 144000000.0],
            ),
            (
                RecipientKind::DiscordUser,
                "vocal_time",
                vec![100.0, 500.0, 1000.0, 2000.0],
            ),
            (
                RecipientKind::DiscordUser,
                "message_count",
                vec![100.0, 1000.0, 3000.0, 8000.0, 15000.0],
            ),
        ] {
            let rules: Vec<_> = definitions
                .iter()
                .filter(|d| d.kind == kind)
                .filter_map(|d| d.rule.as_ref())
                .filter(|r| r.stat == field)
                .collect();
            assert_eq!(
                rules.iter().map(|r| r.minimum).collect::<Vec<_>>(),
                thresholds
            );
            for (index, threshold) in thresholds.iter().enumerate() {
                let rows = vec![json!({field: threshold})];
                assert_eq!(
                    rules
                        .iter()
                        .filter(|r| qualifies(r, &rows).unwrap())
                        .count(),
                    index + 1
                );
                assert!(!qualifies(rules[index], &[json!({field: threshold - 0.01})]).unwrap());
            }
        }
        assert!(
            !definitions
                .iter()
                .any(|d| d.name.as_deref() == Some("Une loutre qui ne prend pas de douche"))
        );
    }

    #[test]
    fn badge_resolution_scopes_platform_and_rejects_ambiguous_prefixes() {
        let definitions = catalogue().unwrap();
        let definition = definitions
            .iter()
            .find(|d| d.name_prefix.is_some())
            .unwrap();
        let badges = vec![
            json!({"id":"one", "name":"La Loutre qui ne touche pas d'eau non plus ...", "plateform":"Minecraft"}),
        ];
        assert_eq!(resolve_badge(definition, &badges).unwrap()["id"], "one");
        let mut ambiguous = badges.clone();
        ambiguous.push(badges[0].clone());
        assert!(resolve_badge(definition, &ambiguous).is_err());
        let wrong_platform = vec![json!({"name": badges[0]["name"], "plateform":"Discord"})];
        assert!(resolve_badge(definition, &wrong_platform).is_err());
    }
    #[test]
    fn thresholds_aggregate_and_can_scope_servers_and_json_keys() {
        let stats = vec![
            json!({"server":"a", "blocks_mined":4, "mobs_killed":{"minecraft:zombie":3}}),
            json!({"server":"b", "blocks_mined":6}),
        ];
        let mut rule = Rule {
            stat: "blocks_mined".into(),
            minimum: 10.0,
            server: None,
        };
        assert!(qualifies(&rule, &stats).unwrap());
        rule.server = Some("a".into());
        assert!(!qualifies(&rule, &stats).unwrap());
        rule.stat = "mobs_killed.minecraft:zombie".into();
        rule.minimum = 3.0;
        assert!(qualifies(&rule, &stats).unwrap());
        rule.stat = "unknown".into();
        assert!(qualifies(&rule, &stats).is_err());
        rule.stat = "deaths".into();
        rule.minimum = -1.0;
        assert!(qualifies(&rule, &stats).is_err());
    }
}

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Clone, Serialize, ToSchema)]
pub struct PlayerActivity {
    /// PocketBase account ID, or the stored reference for a deleted account.
    pub id: String,
    pub discord_id: Option<String>,
    pub username: Option<String>,
    pub stats: DiscordActivity,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct GlobalDiscordActivity {
    #[serde(flatten)]
    pub totals: DiscordActivity,
    pub players: Vec<PlayerActivity>,
}

#[derive(Clone, Serialize, ToSchema, Default)]
pub struct ActivityDay {
    pub date: String,
    pub message_count: i64,
    /// Decimal hours, as stored in PocketBase.
    pub vocal_hours: f64,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct ChannelTotal {
    pub id: String,
    pub name: String,
    pub message_count: i64,
    pub vocal_hours: f64,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct VoiceCompanion {
    pub id: String,
    pub username: String,
    /// Number of distinct dated days with a record mentioning this user.
    pub records_together: u64,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct DiscordActivity {
    pub total_messages: i64,
    pub total_vocal_hours: f64,
    /// Activity with an absent or invalid date is included in totals, but not daily.
    pub undated_records: u64,
    pub undated_messages: i64,
    pub undated_vocal_hours: f64,
    /// False if daily history is incomplete; day records use known dates only.
    pub daily_complete: bool,
    pub record_messages_day: Option<ActivityDay>,
    pub record_vocal_day: Option<ActivityDay>,
    pub favorite_text_channel: Option<ChannelTotal>,
    pub favorite_voice_channel: Option<ChannelTotal>,
    /// Most regularly present companion, ranked by distinct days together, excluding migration.
    pub top_voice_companion: Option<VoiceCompanion>,
    /// False when channel counters are missing; favorites use available counters only.
    pub channel_counters_complete: bool,
    pub daily: Vec<ActivityDay>,
    pub fetched_at: String,
}

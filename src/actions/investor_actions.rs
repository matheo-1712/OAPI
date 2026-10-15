use crate::models::investor::InvestorStats;
use crate::utils::constants::{
    DISCORD_USER_BADGES_COLLECTION, DISCORD_USERS_COLLECTION, MINECRAFT_PLAYERS_COLLECTION,
    MINECRAFT_SERVERS_COLLECTION,
};
use crate::utils::pocketbase::PocketbaseClient;

pub async fn get_investor_stats() -> Result<InvestorStats, String> {
    let mut pb = PocketbaseClient::new();
    pb.login().await?;
    fetch_stats(&pb).await
}

pub(crate) async fn fetch_stats(pb: &PocketbaseClient) -> Result<InvestorStats, String> {
    let (discord_users, minecraft_players, minecraft_servers, badges_awarded) = tokio::try_join!(
        pb.count_records(DISCORD_USERS_COLLECTION),
        pb.count_records(MINECRAFT_PLAYERS_COLLECTION),
        pb.count_records(MINECRAFT_SERVERS_COLLECTION),
        pb.count_records(DISCORD_USER_BADGES_COLLECTION),
    )?;

    Ok(InvestorStats {
        discord_users,
        minecraft_players,
        minecraft_servers,
        badges_awarded,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    })
}

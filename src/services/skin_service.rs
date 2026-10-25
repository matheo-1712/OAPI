//! Daily refresh of Minecraft heads and skins stored in PocketBase.
//!
//! For each Minecraft player, the Mojang skin is compared with `players.skin_hash`;
//! when it changed (or the image fields are empty), the head render and the raw
//! skin texture are regenerated and uploaded.

use crate::services::image_service::valid_uuid;
use crate::services::mojang::{self, ProfileLookup, SESSION_SERVER_URL, SkinTexture};
use crate::services::scheduler::{BoxFuture, TaskSummary};
use crate::services::skin_render::{encode_png, render_head, texture_png};
use crate::utils::constants::MINECRAFT_PLAYERS_COLLECTION;
use crate::utils::image_assets;
use crate::utils::pocketbase::PocketbaseClient;
use image::DynamicImage;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{info, warn};

/// Pause between two Mojang profile requests.
pub const MOJANG_DELAY: Duration = Duration::from_millis(1500);

/// Downloads a skin texture; abstracted so tests need no network.
pub type TextureFetcher = for<'a> fn(&'a str) -> BoxFuture<'a, Option<DynamicImage>>;

fn download_texture(url: &str) -> BoxFuture<'_, Option<DynamicImage>> {
    Box::pin(image_assets::download(url))
}

/// Refreshes the head and skin of every Minecraft player.
pub async fn refresh_all(pb: &PocketbaseClient) -> Result<TaskSummary, String> {
    refresh_all_with(pb, SESSION_SERVER_URL, MOJANG_DELAY, download_texture).await
}

/// Encoded head and skin texture PNGs.
type Pngs = (Vec<u8>, Vec<u8>);

/// Renders the head PNG and re-encodes the texture PNG.
async fn render_pngs(texture: &SkinTexture, fetch_texture: TextureFetcher) -> Result<Pngs, String> {
    let image = fetch_texture(&texture.url)
        .await
        .ok_or("failed to download the skin texture")?
        .to_rgba8();
    let head = encode_png(&render_head(&image)?)?;
    let skin = texture_png(&image)?;
    Ok((head, skin))
}

fn has_text(record: &Value, field: &str) -> bool {
    record[field].as_str().is_some_and(|v| !v.is_empty())
}

fn record_id(record: &Value) -> String {
    record["id"].as_str().unwrap_or_default().to_owned()
}

async fn refresh_all_with(
    pb: &PocketbaseClient,
    mojang_url: &str,
    delay: Duration,
    fetch_texture: TextureFetcher,
) -> Result<TaskSummary, String> {
    let records = pb
        .list_all_records::<Value>(MINECRAFT_PLAYERS_COLLECTION, "")
        .await?;

    // Group records by normalized UUID, keeping first-seen order.
    let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for record in records {
        let Some(account) = record["account_id"].as_str().filter(|id| valid_uuid(id)) else {
            continue;
        };
        let key = account.replace('-', "").to_ascii_lowercase();
        let slot = *index.entry(key.clone()).or_insert_with(|| {
            groups.push((key, Vec::new()));
            groups.len() - 1
        });
        groups[slot].1.push(record);
    }

    let mut summary = TaskSummary::default();
    let mut updated = 0usize;
    for (position, (uuid, group)) in groups.into_iter().enumerate() {
        if position > 0 {
            tokio::time::sleep(delay).await;
        }
        summary.processed += group.len();
        let texture = match mojang::fetch_skin(mojang_url, &uuid).await {
            Ok(ProfileLookup::Skin(texture)) => texture,
            Ok(ProfileLookup::NoSkin | ProfileLookup::Unknown) => {
                summary.succeeded += group.len();
                continue;
            }
            Ok(ProfileLookup::RateLimited) => {
                warn!("Mojang rate limit reached, skins refresh resumes at the next run");
                break;
            }
            Err(error) => {
                warn!(%error, "Failed to read Mojang profile");
                summary.failed.extend(group.iter().map(record_id));
                continue;
            }
        };
        let hash = texture.skin_hash();
        let mut rendered: Option<Result<Pngs, String>> = None;
        for record in group {
            let id = record_id(&record);
            if record["skin_hash"].as_str() == Some(hash.as_str())
                && has_text(&record, "head")
                && has_text(&record, "skin")
            {
                summary.succeeded += 1;
                continue;
            }
            let pngs = match &rendered {
                Some(cached) => cached,
                None => rendered.insert(render_pngs(&texture, fetch_texture).await),
            };
            let result = match pngs {
                Ok((head, skin)) => pb
                    .update_record_multipart(
                        MINECRAFT_PLAYERS_COLLECTION,
                        &id,
                        &[("skin_hash", hash.clone())],
                        vec![
                            ("head", "head.png", head.clone()),
                            ("skin", "skin.png", skin.clone()),
                        ],
                    )
                    .await
                    .map(|_| ()),
                Err(error) => Err(error.clone()),
            };
            match result {
                Ok(()) => {
                    summary.succeeded += 1;
                    updated += 1;
                }
                Err(error) => {
                    warn!(record = %id, %error, "Failed to update player skin");
                    summary.failed.push(id);
                }
            }
        }
    }
    info!(
        processed = summary.processed,
        updated, "Skins refresh finished"
    );
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use image::RgbaImage;
    use mockito::{Matcher, Mock, Server, ServerGuard};
    use serde_json::json;

    const UUID: &str = "0123456789abcdef0123456789abcdef";
    const UUID_DASHED: &str = "01234567-89ab-cdef-0123-456789abcdef";
    const UUID2: &str = "fedcba9876543210fedcba9876543210";

    fn page(items: Value) -> String {
        let n = items.as_array().map_or(0, |a| a.len());
        json!({"page":1,"perPage":500,"totalPages":1,"totalItems":n,"items":items}).to_string()
    }

    fn test_texture(_: &str) -> BoxFuture<'_, Option<DynamicImage>> {
        Box::pin(async { Some(DynamicImage::ImageRgba8(RgbaImage::new(64, 64))) })
    }

    fn profile_body() -> String {
        let payload =
            json!({"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc"}}});
        let value = STANDARD.encode(payload.to_string());
        json!({"id":UUID,"name":"Steve","properties":[{"name":"textures","value":value}]})
            .to_string()
    }

    async fn players(server: &mut ServerGuard, items: Value) -> Mock {
        server
            .mock("GET", "/api/collections/players/records")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(page(items))
            .create_async()
            .await
    }

    async fn mojang_mock(server: &mut ServerGuard, uuid: &str, status: usize, hits: usize) -> Mock {
        let mut mock = server
            .mock("GET", format!("/session/minecraft/profile/{uuid}").as_str())
            .with_status(status);
        if status == 200 {
            mock = mock.with_body(profile_body());
        }
        mock.expect(hits).create_async().await
    }

    async fn patch(server: &mut ServerGuard, id: &str, status: usize, hits: usize) -> Mock {
        patch_matching(server, id, status, hits, Matcher::Any).await
    }

    async fn patch_matching(
        server: &mut ServerGuard,
        id: &str,
        status: usize,
        hits: usize,
        body: Matcher,
    ) -> Mock {
        server
            .mock(
                "PATCH",
                format!("/api/collections/players/records/{id}").as_str(),
            )
            .match_body(body)
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(json!({"id": id}).to_string())
            .expect(hits)
            .create_async()
            .await
    }

    async fn run(server: &ServerGuard) -> TaskSummary {
        let pb = PocketbaseClient::for_test(server.url(), "token");
        refresh_all_with(&pb, &server.url(), Duration::ZERO, test_texture)
            .await
            .unwrap()
    }

    fn summary(processed: usize, succeeded: usize, failed: &[&str]) -> TaskSummary {
        TaskSummary {
            processed,
            succeeded,
            failed: failed.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn updates_changed_skin() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([{"id":"p1","account_id":UUID,"skin_hash":"","head":"","skin":""}]),
        )
        .await;
        let m = mojang_mock(&mut server, UUID, 200, 1).await;
        let p = patch_matching(
            &mut server,
            "p1",
            200,
            1,
            Matcher::AllOf(vec![
                Matcher::Regex("v2:abc:classic".into()),
                Matcher::Regex("name=\"head\"".into()),
                Matcher::Regex("name=\"skin\"".into()),
            ]),
        )
        .await;
        assert_eq!(run(&server).await, summary(1, 1, &[]));
        m.assert_async().await;
        p.assert_async().await;
    }

    #[tokio::test]
    async fn mixed_group_updates_only_stale_record() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([
                {"id":"p1","account_id":UUID,"skin_hash":"v2:abc:classic","head":"h.png","skin":"s.png"},
                {"id":"p2","account_id":UUID_DASHED,"skin_hash":"","head":"","skin":""}
            ]),
        )
        .await;
        let m = mojang_mock(&mut server, UUID, 200, 1).await;
        let p1 = patch(&mut server, "p1", 200, 0).await;
        let p2 = patch(&mut server, "p2", 200, 1).await;
        assert_eq!(run(&server).await, summary(2, 2, &[]));
        m.assert_async().await;
        p1.assert_async().await;
        p2.assert_async().await;
    }

    #[tokio::test]
    async fn unchanged_skin_is_not_uploaded() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([{"id":"p1","account_id":UUID,"skin_hash":"v2:abc:classic","head":"h.png","skin":"s.png"}]),
        )
        .await;
        let m = mojang_mock(&mut server, UUID, 200, 1).await;
        let p = patch(&mut server, "p1", 200, 0).await;
        assert_eq!(run(&server).await, summary(1, 1, &[]));
        m.assert_async().await;
        p.assert_async().await;
    }

    #[tokio::test]
    async fn empty_file_fields_force_regeneration() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([{"id":"p1","account_id":UUID,"skin_hash":"v2:abc:classic","head":"","skin":"s.png"}]),
        )
        .await;
        let m = mojang_mock(&mut server, UUID, 200, 1).await;
        let p = patch(&mut server, "p1", 200, 1).await;
        assert_eq!(run(&server).await, summary(1, 1, &[]));
        m.assert_async().await;
        p.assert_async().await;
    }

    #[tokio::test]
    async fn v1_hash_forces_regeneration() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([{"id":"p1","account_id":UUID,"skin_hash":"abc:classic","head":"h.png","skin":"s.png"}]),
        )
        .await;
        let m = mojang_mock(&mut server, UUID, 200, 1).await;
        let p = patch(&mut server, "p1", 200, 1).await;
        assert_eq!(run(&server).await, summary(1, 1, &[]));
        m.assert_async().await;
        p.assert_async().await;
    }

    #[tokio::test]
    async fn steam_accounts_are_ignored() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([{"id":"p1","account_id":"76561198000000000"}]),
        )
        .await;
        assert_eq!(run(&server).await, summary(0, 0, &[]));
    }

    #[tokio::test]
    async fn duplicate_account_ids_call_mojang_once() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([
                {"id":"p1","account_id":UUID,"skin_hash":"","head":"","skin":""},
                {"id":"p2","account_id":UUID_DASHED,"skin_hash":"","head":"","skin":""}
            ]),
        )
        .await;
        let m = mojang_mock(&mut server, UUID, 200, 1).await;
        let p1 = patch(&mut server, "p1", 200, 1).await;
        let p2 = patch(&mut server, "p2", 200, 1).await;
        assert_eq!(run(&server).await, summary(2, 2, &[]));
        m.assert_async().await;
        p1.assert_async().await;
        p2.assert_async().await;
    }

    #[tokio::test]
    async fn unknown_profile_is_skipped() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([{"id":"p1","account_id":UUID,"skin_hash":"","head":"","skin":""}]),
        )
        .await;
        let m = mojang_mock(&mut server, UUID, 204, 1).await;
        let p = patch(&mut server, "p1", 200, 0).await;
        assert_eq!(run(&server).await, summary(1, 1, &[]));
        m.assert_async().await;
        p.assert_async().await;
    }

    #[tokio::test]
    async fn failure_does_not_stop_others() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([
                {"id":"p1","account_id":UUID,"skin_hash":"","head":"","skin":""},
                {"id":"p2","account_id":UUID2,"skin_hash":"","head":"","skin":""}
            ]),
        )
        .await;
        let m1 = mojang_mock(&mut server, UUID, 200, 1).await;
        let m2 = mojang_mock(&mut server, UUID2, 200, 1).await;
        let p1 = patch(&mut server, "p1", 500, 1).await;
        let p2 = patch(&mut server, "p2", 200, 1).await;
        assert_eq!(run(&server).await, summary(2, 1, &["p1"]));
        m1.assert_async().await;
        m2.assert_async().await;
        p1.assert_async().await;
        p2.assert_async().await;
    }

    #[tokio::test]
    async fn rate_limit_stops_the_run_with_partial_summary() {
        let mut server = Server::new_async().await;
        players(
            &mut server,
            json!([
                {"id":"p1","account_id":UUID,"skin_hash":"","head":"","skin":""},
                {"id":"p2","account_id":UUID2,"skin_hash":"","head":"","skin":""}
            ]),
        )
        .await;
        let first = mojang_mock(&mut server, UUID, 429, 1).await;
        let second = mojang_mock(&mut server, UUID2, 200, 0).await;
        assert_eq!(run(&server).await, summary(1, 0, &[]));
        first.assert_async().await;
        second.assert_async().await;
    }
}

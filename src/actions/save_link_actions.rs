//! Links an uploaded Mario Kart DS save to the session's DS Games player.
//!
//! The upload is untrusted: an embedded player ID never decides which account
//! is authorised, it is only checked against the session's own player. A new
//! credential is registered in `adln_devices` and read back before any bytes
//! are returned; an existing one is verified and reused with its counters.

use crate::actions::adl_onboarding::Member;
use crate::actions::save_link_journal::{JournalEntry, LinkJournal, operation_key};
use crate::services::adln_device::{self, WrappingKey};
use crate::services::mkds_save::{self, Credential, SaveError, valid_record_id};
use crate::utils::constants::ADLN_DEVICES_COLLECTION;
use crate::utils::keyed_lock;
use crate::utils::records::Records;
use serde_json::{Value, json};

/// Game identifier stored in `adln_devices.games`.
pub const GAME: &str = "MKDS";

#[derive(Debug, PartialEq, Eq)]
pub enum LinkError {
    Save(SaveError),
    /// The save is linked to another player.
    ForeignAccount,
    /// The save's device was revoked.
    Revoked,
    /// The save carries a device that is not registered.
    Unregistered,
    /// The device record does not match the save (player, game or secret).
    Mismatch,
    /// PocketBase or the journal failed; safe to retry.
    Unavailable(String),
}

impl From<SaveError> for LinkError {
    fn from(error: SaveError) -> Self {
        Self::Save(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOutcome {
    Created,
    Reused,
}

pub struct LinkedSave {
    pub bytes: Vec<u8>,
    pub outcome: LinkOutcome,
}

async fn find_device<R: Records>(records: &R, device_id: &str) -> Result<Option<Value>, LinkError> {
    if !valid_record_id(device_id) {
        return Ok(None);
    }
    let rows = records
        .find(ADLN_DEVICES_COLLECTION, &[("id", device_id)])
        .await
        .map_err(LinkError::Unavailable)?;
    Ok(rows.into_iter().find(|row| row["id"] == device_id))
}

/// Checks a device record against the session's player and returns its secret.
fn verified_secret(
    key: &WrappingKey,
    record: &Value,
    device_id: &str,
    player_id: &str,
) -> Result<[u8; 32], LinkError> {
    if record.get("player").and_then(Value::as_str) != Some(player_id) {
        return Err(LinkError::Mismatch);
    }
    if record.get("revoked").and_then(Value::as_bool) != Some(false) {
        return Err(LinkError::Revoked);
    }
    let allowed = record
        .get("games")
        .and_then(Value::as_array)
        .is_some_and(|games| games.iter().any(|game| game == GAME));
    if !allowed {
        return Err(LinkError::Mismatch);
    }
    let envelope = record
        .get("secret_ciphertext")
        .and_then(Value::as_str)
        .unwrap_or_default();
    adln_device::open(key, device_id, player_id, envelope).ok_or(LinkError::Mismatch)
}

/// Verifies the credential already present in a save.
async fn reuse<R: Records>(
    records: &R,
    key: &WrappingKey,
    member: &Member,
    credential: Credential,
) -> Result<Credential, LinkError> {
    if credential.player_id != member.player_id {
        return Err(LinkError::ForeignAccount);
    }
    let record = find_device(records, &credential.device_id)
        .await?
        .ok_or(LinkError::Unregistered)?;
    let secret = verified_secret(key, &record, &credential.device_id, &member.player_id)?;
    if !adln_device::secrets_equal(&secret, &credential.secret) {
        return Err(LinkError::Mismatch);
    }
    Ok(credential)
}

/// Registers (or reconciles) the device of this upload and returns its credential.
async fn provision<R: Records>(
    records: &R,
    key: &WrappingKey,
    journal: &LinkJournal,
    member: &Member,
    upload: &[u8],
) -> Result<Credential, LinkError> {
    let operation = operation_key(key, &member.discord_id, &member.player_id, upload);
    let _guard = keyed_lock::lock(&format!("save-link:{operation}")).await;
    let journaled = journal
        .get(&operation)
        .map_err(LinkError::Unavailable)?
        .filter(|entry| entry.player_id == member.player_id);
    let device_id = match journaled {
        Some(entry) => entry.device_id,
        None => {
            let device_id = adln_device::generate_device_id();
            // Recorded before the create, so an uncertain create is reconciled on retry.
            journal
                .put(
                    &operation,
                    &JournalEntry {
                        device_id: device_id.clone(),
                        player_id: member.player_id.clone(),
                        created_at: chrono::Utc::now().timestamp(),
                    },
                )
                .map_err(LinkError::Unavailable)?;
            device_id
        }
    };

    let mut record = find_device(records, &device_id).await?;
    if record.is_none() {
        let secret = adln_device::generate_secret();
        let created = records
            .create(
                ADLN_DEVICES_COLLECTION,
                &json!({
                    "id": device_id,
                    "player": member.player_id,
                    "secret_ciphertext": adln_device::seal(key, &device_id, &member.player_id, &secret),
                    "revoked": false,
                    "games": [GAME],
                }),
            )
            .await;
        // Read back whatever the create returned: the stored record is authoritative.
        record = find_device(records, &device_id).await?;
        if record.is_none() {
            return Err(LinkError::Unavailable(match created {
                Err(error) => error,
                Ok(_) => "device record missing after creation".into(),
            }));
        }
    }
    let record = record.expect("checked above");
    let secret = verified_secret(key, &record, &device_id, &member.player_id)?;
    Ok(Credential {
        generation: 1,
        login_counter: 0,
        device_id,
        player_id: member.player_id.clone(),
        secret,
    })
}

/// Returns a patched copy of `upload` carrying the member's verified device credential.
pub async fn link_save<R: Records>(
    records: &R,
    key: &WrappingKey,
    journal: &LinkJournal,
    member: &Member,
    upload: &[u8],
) -> Result<LinkedSave, LinkError> {
    mkds_save::validate_save(upload)?;
    if !valid_record_id(&member.player_id) {
        return Err(LinkError::Unavailable(
            "DS Games player ID is not a 15-character record ID".into(),
        ));
    }
    let (credential, outcome) = match mkds_save::read_credential(upload)? {
        Some(existing) => (
            reuse(records, key, member, existing).await?,
            LinkOutcome::Reused,
        ),
        None => (
            provision(records, key, journal, member, upload).await?,
            LinkOutcome::Created,
        ),
    };
    let bytes = mkds_save::write_credential(upload, &credential)?;
    mkds_save::verify_patched(upload, &bytes, &credential).map_err(LinkError::Unavailable)?;
    Ok(LinkedSave { bytes, outcome })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mkds_save::{CREDENTIAL_REGION, PAGE_A, PAGE_B, PAGE_LEN, SAVE_LEN};
    use crate::utils::records::fake::{CreateFault, FakeRecords};

    const PLAYER: &str = "0s90t9j1i8zxpwr";

    fn key() -> WrappingKey {
        WrappingKey::from_hex(&"5a".repeat(32)).unwrap()
    }

    fn member(player: &str) -> Member {
        Member {
            discord_id: "111".into(),
            discord_user_id: "user00000000001".into(),
            player_id: player.into(),
        }
    }

    fn save() -> Vec<u8> {
        let mut save: Vec<u8> = (0..SAVE_LEN).map(|i| (i * 31 % 251) as u8).collect();
        save[..8].copy_from_slice(mkds_save::SAVE_MAGIC);
        save[CREDENTIAL_REGION].fill(0);
        save
    }

    fn journal() -> (LinkJournal, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("oapi-link-{}", adln_device::generate_device_id()));
        (LinkJournal::new(&dir), dir)
    }

    async fn link(
        store: &FakeRecords,
        journal: &LinkJournal,
        member: &Member,
        upload: &[u8],
    ) -> Result<LinkedSave, LinkError> {
        link_save(store, &key(), journal, member, upload).await
    }

    #[tokio::test]
    async fn first_link_registers_one_encrypted_device_and_patches_only_the_pages() {
        let store = FakeRecords::default();
        let (journal, dir) = journal();
        let original = save();
        let linked = link(&store, &journal, &member(PLAYER), &original)
            .await
            .unwrap();
        assert_eq!(linked.outcome, LinkOutcome::Created);
        assert_eq!(linked.bytes.len(), SAVE_LEN);
        assert_eq!(original[..PAGE_A], linked.bytes[..PAGE_A]);
        assert_eq!(
            original[PAGE_B + PAGE_LEN..],
            linked.bytes[PAGE_B + PAGE_LEN..]
        );

        let devices = store.rows(ADLN_DEVICES_COLLECTION);
        assert_eq!(devices.len(), 1);
        let device = &devices[0];
        assert_eq!(device["player"], PLAYER);
        assert_eq!(device["revoked"], false);
        assert_eq!(device["games"], json!(["MKDS"]));
        let ciphertext = device["secret_ciphertext"].as_str().unwrap();
        assert_eq!(ciphertext.len(), 83);

        let credential = mkds_save::read_credential(&linked.bytes).unwrap().unwrap();
        assert_eq!(credential.generation, 1);
        assert_eq!(credential.login_counter, 0);
        assert_eq!(credential.player_id, PLAYER);
        assert_eq!(device["id"], credential.device_id.as_str());
        let id = credential.device_id.as_str();
        assert_eq!(
            adln_device::open(&key(), id, PLAYER, ciphertext),
            Some(credential.secret)
        );
        // The stored row never holds the raw secret or the key.
        let row = device.to_string();
        let hex: String = credential
            .secret
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert!(!row.contains(&hex) && !row.contains(&"5a".repeat(32)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn uploading_a_linked_save_reuses_it_and_keeps_counters() {
        let store = FakeRecords::default();
        let (journal, dir) = journal();
        let first = link(&store, &journal, &member(PLAYER), &save())
            .await
            .unwrap();
        let mut credential = mkds_save::read_credential(&first.bytes).unwrap().unwrap();
        credential.generation = 4;
        credential.login_counter = 17;
        let played = mkds_save::write_credential(&first.bytes, &credential).unwrap();
        let again = link(&store, &journal, &member(PLAYER), &played)
            .await
            .unwrap();
        assert_eq!(again.outcome, LinkOutcome::Reused);
        assert_eq!(again.bytes, played);
        assert_eq!(store.rows(ADLN_DEVICES_COLLECTION).len(), 1);

        // A damaged mirror is repaired from the valid copy.
        let mut damaged = played.clone();
        damaged[PAGE_B + 0x50] ^= 0xFF;
        let repaired = link(&store, &journal, &member(PLAYER), &damaged)
            .await
            .unwrap();
        assert_eq!(repaired.bytes, played);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn retries_of_the_same_upload_return_the_same_device() {
        let store = FakeRecords::default();
        let (journal, dir) = journal();
        let original = save();
        let first = link(&store, &journal, &member(PLAYER), &original)
            .await
            .unwrap();
        // The download failed: the user uploads the original again.
        let retry = link(&store, &journal, &member(PLAYER), &original)
            .await
            .unwrap();
        assert_eq!(first.bytes, retry.bytes);
        // Concurrent duplicates of a new upload also share one device.
        let mut other = save();
        other[0x100] ^= 1;
        let owner = member(PLAYER);
        let (a, b) = tokio::join!(
            link(&store, &journal, &owner, &other),
            link(&store, &journal, &owner, &other)
        );
        assert_eq!(a.unwrap().bytes, b.unwrap().bytes);
        assert_eq!(store.rows(ADLN_DEVICES_COLLECTION).len(), 2);
        assert_eq!(store.creates.lock().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn an_uncertain_create_is_reconciled_with_the_same_device_id() {
        // Stored, but the response timed out: the readback finds it, no second device.
        let store = FakeRecords::default();
        let (journal, dir) = journal();
        store.fail_next_create(ADLN_DEVICES_COLLECTION, CreateFault::StoredThenTimeout);
        let linked = link(&store, &journal, &member(PLAYER), &save())
            .await
            .unwrap();
        assert_eq!(store.rows(ADLN_DEVICES_COLLECTION).len(), 1);
        assert_eq!(store.creates.lock().unwrap().len(), 1);
        assert_eq!(linked.outcome, LinkOutcome::Created);

        // Lost create: no download, and the retry creates the journaled device ID.
        let store = FakeRecords::default();
        store.fail_next_create(ADLN_DEVICES_COLLECTION, CreateFault::Lost);
        let upload = {
            let mut upload = save();
            upload[0x200] ^= 1;
            upload
        };
        assert!(matches!(
            link(&store, &journal, &member(PLAYER), &upload).await,
            Err(LinkError::Unavailable(_))
        ));
        let linked = link(&store, &journal, &member(PLAYER), &upload)
            .await
            .unwrap();
        let creates = store.creates.lock().unwrap().clone();
        assert_eq!(creates.len(), 2);
        assert_eq!(creates[0].1["id"], creates[1].1["id"]);
        let credential = mkds_save::read_credential(&linked.bytes).unwrap().unwrap();
        assert_eq!(creates[0].1["id"], credential.device_id.as_str());
        assert_eq!(store.rows(ADLN_DEVICES_COLLECTION).len(), 1);

        // PocketBase down: nothing returned.
        let store = FakeRecords::default();
        *store.down.lock().unwrap() = true;
        assert!(matches!(
            link(&store, &journal, &member(PLAYER), &save()).await,
            Err(LinkError::Unavailable(_))
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn saves_of_other_accounts_and_untrusted_credentials_are_refused() {
        let store = FakeRecords::default();
        let (journal, dir) = journal();
        let linked = link(&store, &journal, &member(PLAYER), &save())
            .await
            .unwrap();

        // Someone else's session uploading this save: the embedded player is not theirs.
        assert_eq!(
            link(&store, &journal, &member("zzzzzzzzzzzzzzz"), &linked.bytes)
                .await
                .err(),
            Some(LinkError::ForeignAccount)
        );

        // Arbitrary device ID/secret forged into the save: never registered.
        let forged = Credential {
            generation: 1,
            login_counter: 0,
            device_id: "forgeddevice001".into(),
            player_id: PLAYER.into(),
            secret: [9; 32],
        };
        let forged_save = mkds_save::write_credential(&save(), &forged).unwrap();
        assert_eq!(
            link(&store, &journal, &member(PLAYER), &forged_save)
                .await
                .err(),
            Some(LinkError::Unregistered)
        );
        assert_eq!(store.rows(ADLN_DEVICES_COLLECTION).len(), 1);

        // Right device ID, wrong secret.
        let mut credential = mkds_save::read_credential(&linked.bytes).unwrap().unwrap();
        credential.secret[0] ^= 1;
        let wrong_secret = mkds_save::write_credential(&save(), &credential).unwrap();
        assert_eq!(
            link(&store, &journal, &member(PLAYER), &wrong_secret)
                .await
                .err(),
            Some(LinkError::Mismatch)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn revoked_or_inconsistent_device_records_are_refused() {
        let (journal, dir) = journal();
        let store = FakeRecords::default();
        let linked = link(&store, &journal, &member(PLAYER), &save())
            .await
            .unwrap();
        let credential = mkds_save::read_credential(&linked.bytes).unwrap().unwrap();
        let row = store.rows(ADLN_DEVICES_COLLECTION)[0].clone();
        for (field, value, expected) in [
            ("revoked", json!(true), LinkError::Revoked),
            ("games", json!(["PKMN"]), LinkError::Mismatch),
            ("games", json!(null), LinkError::Mismatch),
            ("player", json!("otherplayer0001"), LinkError::Mismatch),
            (
                "secret_ciphertext",
                json!("v1.garbage"),
                LinkError::Mismatch,
            ),
        ] {
            let store = FakeRecords::default();
            let mut changed = row.clone();
            changed[field] = value;
            store.insert(ADLN_DEVICES_COLLECTION, changed);
            assert_eq!(
                link(&store, &journal, &member(PLAYER), &linked.bytes)
                    .await
                    .err(),
                Some(expected),
                "{field}"
            );
            assert!(
                store.creates.lock().unwrap().is_empty(),
                "never re-registered"
            );
        }
        // A journaled device revoked meanwhile is not revived for a retry of the original.
        let mut revoked = row.clone();
        revoked["revoked"] = json!(true);
        let store = FakeRecords::default();
        store.insert(ADLN_DEVICES_COLLECTION, revoked);
        assert_eq!(
            link(&store, &journal, &member(PLAYER), &save()).await.err(),
            Some(LinkError::Revoked)
        );
        assert_eq!(credential.player_id, PLAYER);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn invalid_saves_and_pages_are_rejected_before_any_write() {
        let store = FakeRecords::default();
        let (journal, dir) = journal();
        let mut appended = save();
        appended.extend_from_slice(&[0; 16]);
        let mut conflicting = save();
        let a = Credential {
            generation: 1,
            login_counter: 0,
            device_id: "abcdefghij12345".into(),
            player_id: PLAYER.into(),
            secret: [1; 32],
        };
        let b = Credential {
            device_id: "bbcdefghij12345".into(),
            ..a.clone()
        };
        conflicting[PAGE_A..PAGE_A + PAGE_LEN].copy_from_slice(&mkds_save::encode_page(&a));
        conflicting[PAGE_B..PAGE_B + PAGE_LEN].copy_from_slice(&mkds_save::encode_page(&b));
        let mut garbage = save();
        garbage[PAGE_A + 3] = 7;
        for (upload, error) in [
            (appended, SaveError::InvalidSave),
            (save()[..SAVE_LEN - 1].to_vec(), SaveError::InvalidSave),
            (conflicting, SaveError::ConflictingCopies),
            (garbage, SaveError::UnsupportedCredential),
        ] {
            assert_eq!(
                link(&store, &journal, &member(PLAYER), &upload).await.err(),
                Some(LinkError::Save(error))
            );
        }
        assert!(store.creates.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}

//! Recovery journal for save linking.
//!
//! Before a device record is created, the chosen device ID is written to a
//! private file named by an opaque operation key (HMAC of the session's
//! Discord ID, its player and the uploaded save's SHA-256). A retry of the same
//! operation, after a timeout, a lost response or a failed download, finds the
//! same device ID and reconciles it instead of creating another device. The
//! journal holds no secret: the device secret is recovered by decrypting the
//! stored `secret_ciphertext` with the wrapping key.

use crate::services::adln_device::WrappingKey;
use crate::services::mkds_save::valid_record_id;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;

/// Directory of the journal (a private, persistent volume in production).
pub const JOURNAL_DIR_ENV: &str = "ADLN_LINK_JOURNAL_DIR";
const DEFAULT_DIR: &str = "data/adln_links";
/// Retries of the same upload reuse its device for this long.
const RETENTION_SECS: i64 = 30 * 24 * 3600;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub device_id: String,
    pub player_id: String,
    pub created_at: i64,
}

pub struct LinkJournal {
    dir: PathBuf,
}

/// Opaque operation key: `HMAC(K_op, label || 0 || discord_id || 0 || player_id || 0 || SHA-256(save))`,
/// with `K_op` derived from the wrapping key so it is never used directly.
pub fn operation_key(key: &WrappingKey, discord_id: &str, player_id: &str, save: &[u8]) -> String {
    let mut derive = <Hmac<Sha256> as Mac>::new_from_slice(key.bytes()).expect("any key length");
    derive.update(b"ADLN/website-link-journal-key/1");
    let op_key = derive.finalize().into_bytes();
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&op_key).expect("any key length");
    mac.update(b"ADLN/website-link-op/1\0");
    mac.update(discord_id.as_bytes());
    mac.update(b"\0");
    mac.update(player_id.as_bytes());
    mac.update(b"\0");
    mac.update(&Sha256::digest(save));
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl LinkJournal {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn from_env() -> Self {
        Self::new(std::env::var(JOURNAL_DIR_ENV).unwrap_or_else(|_| DEFAULT_DIR.into()))
    }

    fn path(&self, operation: &str) -> Option<PathBuf> {
        (operation.len() == 64 && operation.bytes().all(|c| c.is_ascii_hexdigit()))
            .then(|| self.dir.join(format!("{operation}.json")))
    }

    /// The device chosen earlier for this operation, if still retained.
    pub fn get(&self, operation: &str) -> Result<Option<JournalEntry>, String> {
        let path = self.path(operation).ok_or("invalid operation key")?;
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("journal read failed: {}", error.kind())),
        };
        let entry: JournalEntry =
            serde_json::from_str(&text).map_err(|_| "journal entry is unreadable".to_string())?;
        if !valid_record_id(&entry.device_id) || !valid_record_id(&entry.player_id) {
            return Err("journal entry is invalid".into());
        }
        let expired = chrono::Utc::now().timestamp() - entry.created_at > RETENTION_SECS;
        Ok((!expired).then_some(entry))
    }

    /// Durably records the device chosen for this operation (written, synced, then renamed).
    pub fn put(&self, operation: &str, entry: &JournalEntry) -> Result<(), String> {
        let path = self.path(operation).ok_or("invalid operation key")?;
        let io = |error: std::io::Error| format!("journal write failed: {}", error.kind());
        create_private_dir(&self.dir).map_err(io)?;
        self.prune();
        let temp = path.with_extension("tmp");
        let mut file = std::fs::File::create(&temp).map_err(io)?;
        file.write_all(&serde_json::to_vec(entry).map_err(|e| e.to_string())?)
            .map_err(io)?;
        file.sync_all().map_err(io)?;
        drop(file);
        std::fs::rename(&temp, &path).map_err(io)?;
        Ok(())
    }

    /// Removes entries past their retention; failures are ignored.
    fn prune(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let now = chrono::Utc::now().timestamp();
        for entry in entries.flatten() {
            let path = entry.path();
            let stale = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<JournalEntry>(&text).ok())
                .is_some_and(|entry| now - entry.created_at > RETENTION_SECS);
            if stale {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

fn create_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn temp_journal() -> (LinkJournal, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "oapi-journal-{}",
            crate::services::adln_device::generate_device_id()
        ));
        (LinkJournal::new(&dir), dir)
    }

    #[test]
    fn operation_keys_bind_user_player_and_save() {
        let key = WrappingKey::from_hex(&"42".repeat(32)).unwrap();
        let base = operation_key(&key, "111", "0s90t9j1i8zxpwr", b"save");
        assert_eq!(base.len(), 64);
        assert_eq!(base, operation_key(&key, "111", "0s90t9j1i8zxpwr", b"save"));
        for other in [
            operation_key(&key, "112", "0s90t9j1i8zxpwr", b"save"),
            operation_key(&key, "111", "1s90t9j1i8zxpwr", b"save"),
            operation_key(&key, "111", "0s90t9j1i8zxpwr", b"savf"),
            operation_key(
                &WrappingKey::from_hex(&"43".repeat(32)).unwrap(),
                "111",
                "0s90t9j1i8zxpwr",
                b"save",
            ),
        ] {
            assert_ne!(base, other);
        }
    }

    #[test]
    fn entries_round_trip_expire_and_hold_no_secret() {
        let (journal, dir) = temp_journal();
        let op = "a".repeat(64);
        assert_eq!(journal.get(&op), Ok(None));
        let entry = JournalEntry {
            device_id: "abcdefghij12345".into(),
            player_id: "0s90t9j1i8zxpwr".into(),
            created_at: chrono::Utc::now().timestamp(),
        };
        journal.put(&op, &entry).unwrap();
        assert_eq!(journal.get(&op), Ok(Some(entry.clone())));
        let stored = std::fs::read_to_string(dir.join(format!("{op}.json"))).unwrap();
        assert!(!stored.contains("secret"));

        let old = JournalEntry {
            created_at: entry.created_at - RETENTION_SECS - 1,
            ..entry
        };
        journal.put(&op, &old).unwrap();
        assert_eq!(journal.get(&op), Ok(None));
        assert!(journal.get("../../etc/passwd").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}

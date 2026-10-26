//! Per-key async locks, so concurrent requests for the same account or save
//! are serialised inside this process.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use tokio::sync::OwnedMutexGuard;

type Locks = Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>;

fn locks() -> &'static Locks {
    static LOCKS: OnceLock<Locks> = OnceLock::new();
    LOCKS.get_or_init(Default::default)
}

/// Waits for exclusive use of `key`; released when the guard drops.
pub async fn lock(key: &str) -> OwnedMutexGuard<()> {
    let entry = {
        let mut map = locks().lock().unwrap_or_else(PoisonError::into_inner);
        // Entries only referenced by the map are idle: drop them.
        map.retain(|_, lock| Arc::strong_count(lock) > 1);
        map.entry(key.to_owned()).or_default().clone()
    };
    entry.lock_owned().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn same_key_waits_and_other_keys_do_not() {
        let held = lock("test:a").await;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), lock("test:a"))
                .await
                .is_err()
        );
        let _other = tokio::time::timeout(Duration::from_millis(20), lock("test:b"))
            .await
            .expect("independent key");
        drop(held);
        let _again = tokio::time::timeout(Duration::from_millis(200), lock("test:a"))
            .await
            .expect("released");
    }
}

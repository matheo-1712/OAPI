//! Minimal record access used by onboarding and save linking, so their
//! race and failure handling can be tested against an in-memory store.

use crate::utils::pocketbase::{ListResponse, PocketbaseClient, quote};
use serde_json::Value;

/// Records matching every `field = value` pair, oldest first (`created`, then `id`).
pub trait Records {
    async fn find(&self, collection: &str, equals: &[(&str, &str)]) -> Result<Vec<Value>, String>;
    async fn create(&self, collection: &str, body: &Value) -> Result<Value, String>;
    async fn delete(&self, collection: &str, id: &str) -> Result<(), String>;
}

/// `a = 'x' && b = 'y'` with every value quoted.
pub fn equality_filter(equals: &[(&str, &str)]) -> String {
    equals
        .iter()
        .map(|(field, value)| format!("{field} = {}", quote(value)))
        .collect::<Vec<_>>()
        .join(" && ")
}

impl Records for PocketbaseClient {
    async fn find(&self, collection: &str, equals: &[(&str, &str)]) -> Result<Vec<Value>, String> {
        let response: ListResponse<Value> = self
            .list_records_with_params(
                collection,
                [
                    ("filter", equality_filter(equals)),
                    ("sort", "created,id".into()),
                    ("page", "1".into()),
                    ("perPage", "50".into()),
                ],
            )
            .await?;
        Ok(response.items)
    }

    async fn create(&self, collection: &str, body: &Value) -> Result<Value, String> {
        self.create_record(collection, body).await
    }

    async fn delete(&self, collection: &str, id: &str) -> Result<(), String> {
        self.delete_record(collection, id).await
    }
}

#[cfg(test)]
pub mod fake {
    //! In-memory stand-in for PocketBase with failure injection.

    use super::Records;
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// What the next `create` in a collection does.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CreateFault {
        /// Stored, but the caller sees a timeout (uncertain result).
        StoredThenTimeout,
        /// Not stored; the caller sees an error.
        Lost,
        /// Another service inserts the same row just before this create.
        ConcurrentDuplicate,
    }

    #[derive(Default)]
    pub struct FakeRecords {
        rows: Mutex<HashMap<String, Vec<Value>>>,
        faults: Mutex<HashMap<String, Vec<CreateFault>>>,
        /// Collection -> fields forming a unique index.
        unique: Mutex<HashMap<String, Vec<String>>>,
        pub creates: Mutex<Vec<(String, Value)>>,
        pub deletes: Mutex<Vec<(String, String)>>,
        clock: Mutex<u64>,
        pub down: Mutex<bool>,
    }

    impl FakeRecords {
        pub fn insert(&self, collection: &str, mut row: Value) -> Value {
            let mut clock = self.clock.lock().unwrap();
            *clock += 1;
            if row.get("id").is_none() {
                row["id"] = json!(format!("gen{:012}", *clock));
            }
            row["created"] = json!(format!("2026-10-26 00:00:{:06}.000Z", *clock));
            self.rows
                .lock()
                .unwrap()
                .entry(collection.into())
                .or_default()
                .push(row.clone());
            row
        }

        pub fn rows(&self, collection: &str) -> Vec<Value> {
            self.rows
                .lock()
                .unwrap()
                .get(collection)
                .cloned()
                .unwrap_or_default()
        }

        pub fn fail_next_create(&self, collection: &str, fault: CreateFault) {
            self.faults
                .lock()
                .unwrap()
                .entry(collection.into())
                .or_default()
                .push(fault);
        }

        pub fn unique(&self, collection: &str, fields: &[&str]) {
            self.unique.lock().unwrap().insert(
                collection.into(),
                fields.iter().map(|f| f.to_string()).collect(),
            );
        }

        fn violates_unique(&self, collection: &str, body: &Value) -> bool {
            let rows = self.rows(collection);
            if let Some(id) = body.get("id")
                && rows.iter().any(|row| row.get("id") == Some(id))
            {
                return true;
            }
            let Some(fields) = self.unique.lock().unwrap().get(collection).cloned() else {
                return false;
            };
            rows.iter()
                .any(|row| fields.iter().all(|field| row.get(field) == body.get(field)))
        }
    }

    impl Records for FakeRecords {
        async fn find(
            &self,
            collection: &str,
            equals: &[(&str, &str)],
        ) -> Result<Vec<Value>, String> {
            tokio::task::yield_now().await;
            if *self.down.lock().unwrap() {
                return Err("PocketBase unavailable".into());
            }
            Ok(self
                .rows(collection)
                .into_iter()
                .filter(|row| {
                    equals.iter().all(|(field, value)| match row.get(*field) {
                        Some(Value::String(text)) => text == value,
                        Some(Value::Bool(flag)) => flag.to_string() == *value,
                        _ => false,
                    })
                })
                .collect())
        }

        async fn create(&self, collection: &str, body: &Value) -> Result<Value, String> {
            tokio::task::yield_now().await;
            self.creates
                .lock()
                .unwrap()
                .push((collection.into(), body.clone()));
            if *self.down.lock().unwrap() {
                return Err("PocketBase unavailable".into());
            }
            let fault = {
                let mut faults = self.faults.lock().unwrap();
                faults
                    .get_mut(collection)
                    .filter(|queue| !queue.is_empty())
                    .map(|queue| queue.remove(0))
            };
            if fault == Some(CreateFault::Lost) {
                return Err("PocketBase create failed: timeout".into());
            }
            if fault == Some(CreateFault::ConcurrentDuplicate) {
                let mut other = body.clone();
                if let Some(object) = other.as_object_mut() {
                    object.remove("id");
                }
                self.insert(collection, other);
            }
            if self.violates_unique(collection, body) {
                return Err("PocketBase create failed: 400 Bad Request".into());
            }
            let row = self.insert(collection, body.clone());
            if fault == Some(CreateFault::StoredThenTimeout) {
                return Err("PocketBase create failed: timeout".into());
            }
            Ok(row)
        }

        async fn delete(&self, collection: &str, id: &str) -> Result<(), String> {
            self.deletes
                .lock()
                .unwrap()
                .push((collection.into(), id.into()));
            if let Some(rows) = self.rows.lock().unwrap().get_mut(collection) {
                rows.retain(|row| row["id"] != id);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    #[test]
    fn filters_quote_every_value() {
        assert_eq!(
            equality_filter(&[("discord_user", "abc"), ("platform", "x' || '1'='1")]),
            "discord_user = 'abc' && platform = 'x\\' || \\'1\\'=\\'1'"
        );
    }

    #[tokio::test]
    async fn pocketbase_lookups_are_sorted_oldest_first() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/collections/players/records")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded(
                    "filter".into(),
                    "discord_user = 'u1' && platform = 'p1'".into(),
                ),
                Matcher::UrlEncoded("sort".into(), "created,id".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"page":1,"perPage":50,"totalPages":1,"totalItems":1,"items":[{"id":"a"}]}"#,
            )
            .create_async()
            .await;
        let pb = PocketbaseClient::for_test(server.url(), "t");
        let rows = pb
            .find("players", &[("discord_user", "u1"), ("platform", "p1")])
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        list.assert_async().await;
    }
}

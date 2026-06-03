# PocketBase Integration (API Client)

The project uses PocketBase as its central data source. The `src/utils/pocketbase.rs` utility provides a robust client to interact with the PocketBase REST API.

---

## Client Features

### 1. Admin Authentication
The client automatically authenticates as an administrator when `login()` is called. This bypasses public visibility rules and allows reading all collections required for statistics generation.

### 2. Pagination Management (Exhaustive Retrieval)
Unlike standard calls which are limited to 100 results by PocketBase, the `list_all_records` method:
- Loops through all available pages.
- Uses a page size of 500 to optimize performance.
- Merges all results before returning them.

This ensures an exact total playtime calculation across thousands of records.

---

## Usage Example in an Action

```rust
let mut pb = PocketbaseClient::new();
pb.login().await?;

// Fetch ALL player stats (even if there are 5000 rows)
let stats: Vec<MinecraftStats> = pb.list_all_records("players_stats", &filter).await?;
```

---

## Security
- The authentication token is stored locally within the client instance.
- Request errors are logged via `tracing` along with the raw response body to aid in diagnostics (e.g., 400 filter errors).

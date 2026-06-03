# Project Architecture

OAPI follows a Layered Architecture to ensure clear separation of concerns, facilitate testing, and allow for modular evolution.

---

## Application Layers

### 1. Configuration Layer (`src/config.rs`)
Responsible for loading and merging configuration sources (YAML, ENV). It provides global, thread-safe access to application settings via a `OnceLock`.

### 2. Models Layer (`src/models/`)
Defines data structures (DTOs - Data Transfer Objects). These structures are used to:
- Deserialize responses from PocketBase.
- Serialize responses from our own API.
- Automatically generate OpenAPI documentation (Swagger).

### 3. Routing Layer (`src/routes/`)
Defines the entry points of our API. It uses the paths defined in the configuration to build the `Axum` routing tree.

### 4. Handlers Layer (`src/handlers/`)
Manages the HTTP interface:
- Extracting parameters from path (`Path`), request (`Query`), or JSON body.
- Basic technical validation.
- Delegating business logic to the **Actions** layer.
- Returning appropriate HTTP status codes.

### 5. Actions Layer (`src/actions/`)
**The heart of orchestration (Use Cases).** An action coordinates the data flow to fulfill a specific need:
- Calls to PocketBase via the utility client.
- Aggregation, filtering, and calculations on raw data.
- Calling **Services** for heavy processing (image generation).
- This layer contains the "recipe" for an endpoint.

### 6. Services Layer (`src/services/`)
Contains pure and isolated business logic:
- Image processing (drawing, geometric calculations, resizing).
- Complex mathematical calculations.
- This layer knows nothing about HTTP or the data source.

### 7. Utilities Layer (`src/utils/`)
Reusable cross-cutting functions:
- `pocketbase.rs`: PocketBase client with admin authentication and automatic pagination management.
- `formatters.rs`: Formatting for dates, text, numbers (thousands separators), and durations.
- `constants.rs`: UI labels and database collection names.

---

## Request Flow (Minecraft Example)

1.  **Client** sends a `POST` to `/api/minecraft-summary/{uuid}`.
2.  **Router** redirects to `minecraft_handler::create_minecraft_summary_by_id`.
3.  **Handler** extracts the UUID and calls `minecraft_actions::get_minecraft_summary_action`.
4.  **Action**:
    - Authenticates to PocketBase via `PocketbaseClient`.
    - Fetches player info, statistics (across all pages), and server info.
    - Aggregates playtime and identifies favorite servers.
    - Passes the data to `image_service::generate_minecraft_profile`.
5.  **Service**:
    - Calculates a data hash for caching.
    - If the image is not in cache:
        - Downloads the player's head avatar.
        - Draws the image (statistics, badges, servers).
        - Saves the PNG file.
    - Returns the image URL.
6.  **Action** & **Handler** return the result to the client in JSON format.

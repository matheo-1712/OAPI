# OAPI Project Instructions

## Architectural Principles
- **Logic-Heavy, No Data Management**: This API is designed to perform complex computations and logic rather than traditional CRUD/Database operations.
- **Layered Architecture**:
    - **Models** (`src/models/`): Data structures for request/response (DTOs).
    - **Handlers** (`src/handlers/`): HTTP-related logic (routing, status codes, JSON extraction).
    - **Actions** (`src/actions/`): Use case orchestration and external data fetching.
    - **Services** (`src/services/`): Pure business logic and complex computations.
- **OpenAPI Standards**: Maintain automatic documentation using `utoipa`.
- **Structured Logging**: Use `tracing` for all events.
- **Dark Theme Frontend**: A minimalist dashboard for monitoring and interaction.

## Development & Quality Standards (CI/CD)
- **Zero Warnings Policy**: All code must pass Clippy without warnings (`cargo clippy --all-targets --all-features -- -D warnings`).
- **Mandatory Formatting**: Use `cargo fmt` before every commit. CI will fail if formatting is not perfect.
- **Automated Testing**: Any new feature or bug fix must include unit tests. Run `cargo test --all-features` to verify.
- **CI/CD Pipeline**: GitHub Actions handles automated validation on every push to `master` or `feat/*` branches.
- **Node.js Compatibility**: Ensure GitHub Actions use Node.js 24 environment (via `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24: true`).

## Configuration Management Logic
- **Hierarchical System**: The application uses a multi-source configuration approach managed in `src/config.rs`.
- **`default_config.yaml` (The Source of Truth)**:
    - Contains all default values and the mandatory structure.
    - **MUST be committed** to the repository.
    - Serves as the fallback if a value is missing elsewhere.
- **`config.yaml` (Local Overrides)**:
    - Used for environment-specific settings (local URLs, ports, secrets).
    - **MUST NOT be committed** (ignored by `.gitignore`).
    - Automatically generated as a template on the first run if missing.
- **Precedence**: Local `config.yaml` values always override `default_config.yaml` values.

## External API Resiliency
- **Handling Nulls & Missing Fields**: External APIs may return `null` or omit fields when data is unavailable. Always use `Option<Vec<T>>` with `#[serde(default)]` for collection fields to prevent deserialization failures.
- **Fail-Safe Processing**: Services must handle `None` values gracefully (e.g., using `.unwrap_or_default()` or `if let Some(...)`) to ensure the application remains functional even with incomplete data.

## Verification Workflow
- **Mandatory Pre-Commit Checks**: Before every commit, you MUST run:
    1. `cargo fmt` (Formatting)
    2. `cargo clippy --all-targets --all-features -- -D warnings` (Linting)
    3. `cargo test --all-features` (Functional Verification)
- **Regression Testing**: Ensure new changes do not break existing features by running the full test suite.
# OAPI Project Instructions

## Architectural Principles
- **Logic-Heavy, No Data Management**: This API is designed to perform complex computations and logic rather than traditional CRUD/Database operations.
- **Layered Architecture**:
    - **Models** (`src/models.rs`): Data structures for request/response (DTOs).
    - **Handlers** (`src/handlers.rs`): HTTP-related logic (routing, status codes, JSON extraction).
    - **Services** (`src/services.rs`): Pure business logic and complex computations.
- **OpenAPI Standards**: Maintain automatic documentation using `utoipa`.
- **Structured Logging**: Use `tracing` for all events.
- **Dark Theme Frontend**: A minimalist dashboard for monitoring and interaction.

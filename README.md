# 🦦 OAPI — Otter API System

[![CI/CD Pipeline](https://github.com/matheo-1712/OAPI/actions/workflows/ci.yml/badge.svg)](https://github.com/matheo-1712/OAPI/actions)
[![Rust](https://img.shields.io/badge/rust-v1.81+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Built with Axum](https://img.shields.io/badge/built%20with-Axum-blue)](https://github.com/tokio-rs/axum)

**OAPI** is a high-performance, asynchronous orchestration API built with Rust. It serves as the backbone logic for the *Antre des Loutres* community, handling complex image generation, real-time infrastructure monitoring, and external API integration.

---

## ✨ Core Features

### 🎨 Dynamic Image Generation
Generate high-fidelity profile summary cards for Discord and Minecraft.
- **Smart Caching**: Uses SHA-256 state hashing to avoid redundant generation.
- **Real-time Stats**: Aggregates messaging activity, voice time, and Minecraft gameplay (playtime, distance, blocks).
- **Rich Aesthetics**: Custom font rendering, dynamic color pills, and automated avatar retrieval (Discord & Minecraft heads).

### 📊 Infrastructure Monitoring
Real-time health tracking for the entire community ecosystem.
- **Multi-protocol**: Supports HTTP(S) and Minecraft (TCP/SLP) status pings.
- **Concurrent Checks**: All services are polled in parallel for sub-second response times.
- **Live Dashboard**: A minimalist, modern web interface included.

### 🔌 PocketBase Integration
A robust data layer powered by PocketBase.
- **Admin Auth**: Secure and automated administrative access to collections.
- **Exhaustive Fetching**: Custom pagination logic to retrieve complete historical data.
- **Automatic Documentation**: Fully compliant OpenAPI 3.0 spec generated via `utoipa`.

---

## 🏗 Architecture

OAPI follows a strictly decoupled, layered architecture to ensure maintainability and testability:

| Layer | Responsibility |
| :--- | :--- |
| **Handlers** | HTTP entry points, parameter extraction, and status codes. |
| **Actions** | Use-case orchestration and external data fetching. |
| **Services** | Pure business logic and computationally intensive tasks. |
| **Models** | Strictly typed DTOs and OpenAPI schema definitions. |
| **Utils** | Generic HTTP fetchers, formatters, and global constants. |

---

## ⚙️ Configuration

The system uses a hierarchical YAML configuration management system.

1. **`default_config.yaml`**: The source of truth. Contains all default values and mandatory structure. (Committed to Git).
2. **`config.yaml`**: Local overrides for environment-specific settings (local URLs, secret keys). (Ignored by Git).

*On first run, the application automatically generates a `config.yaml` template if it is missing.*

---

## 🚀 Getting Started

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- A terminal with `cargo` access

### Installation & Run
```bash
# Clone the repository
git clone https://github.com/matheo-1712/OAPI.git
cd OAPI

# Run in development mode
cargo run
```

### Accessing the API
- **Swagger UI**: [http://localhost:3000/swagger-ui](http://localhost:3000/swagger-ui)
- **Monitoring Dashboard**: [http://localhost:3000/monitoring.html](http://localhost:3000/monitoring.html)

---

## 🛠 Development & Quality

We maintain a **Zero Warnings Policy**. Every contribution must pass strict CI/CD checks.

### Pre-commit Workflow
Before pushing, ensure your code meets the quality standards:
```bash
# Format code
cargo fmt

# Check for lints and warnings
cargo clippy --all-targets --all-features -- -D warnings

# Run all tests
cargo test --all-features
```

---

## 📚 Documentation

Deep dives into specific modules:
- [📖 Architecture Overview](./docs/architecture.md) ([EN](./docs/en/architecture.md))
- [📡 Monitoring System](./docs/monitoring.md) ([EN](./docs/en/monitoring.md))
- [🖼 Image Generation Engine](./docs/generation_images.md) ([EN](./docs/en/generation_images.md))
- [⚙️ Configuration Management](./docs/configuration.md) ([EN](./docs/en/configuration.md))
- [🔌 PocketBase Integration](./docs/pocketbase.md) ([EN](./docs/en/pocketbase.md))

---

## 🎨 Tech Stack
- **Axum**: Asynchronous web framework.
- **Tokio**: Multi-threaded runtime.
- **Utoipa**: Automatic OpenAPI documentation.
- **Image-rs**: Native image processing.
- **Reqwest**: Type-safe HTTP client.
- **Tracing**: Structured diagnostic logging.

---
Built by [matheo-1712](https://github.com/matheo-1712)

# 🦦 OAPI — Otter API System

[![CI/CD Pipeline](https://github.com/matheo-1712/OAPI/actions/workflows/ci.yml/badge.svg)](https://github.com/matheo-1712/OAPI/actions)
[![Rust](https://img.shields.io/badge/rust-v1.81+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Built with Axum](https://img.shields.io/badge/built%20with-Axum-blue)](https://github.com/tokio-rs/axum)

**OAPI** is a high-performance, asynchronous orchestration API built with Rust. It serves as the backbone logic for the *Antre des Loutres* community, handling complex image generation, real-time infrastructure monitoring, and external API integration.

---

## ✨ Core Features

### 🎨 Dynamic Image Generation
Generate high-fidelity Discord profile summary cards on-the-fly.
- **Smart Caching**: Uses SHA-256 state hashing to avoid redundant generation.
- **Real-time Stats**: Aggregates messaging activity and vocal time into visual components.
- **Rich Aesthetics**: Custom font rendering and dynamic role-based color pills.

### 📊 Infrastructure Monitoring
Real-time health tracking for the entire community ecosystem.
- **Multi-protocol**: Supports HTTP(S) and Minecraft (TCP/SLP) status pings.
- **Concurrent Checks**: All services are polled in parallel for sub-second response times.
- **Live Dashboard**: A minimalist, modern web interface included.

### 🔌 Resilient API Integration
A robust fetching layer designed for high availability.
- **Fail-safe Parsing**: Gracefully handles `null` or missing fields from external sources.
- **Unified Schema**: Standardized DTOs for consistent data handling across the stack.
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
- [📖 Architecture Overview](./docs/architecture.md)
- [📡 Monitoring System](./docs/monitoring.md)
- [🖼 Image Generation Engine](./docs/generation_images.md)
- [⚙️ Configuration Management](./docs/configuration.md)
- [🔌 API Integration Guide](./docs/ajouter_api_externe.md)

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

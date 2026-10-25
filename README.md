# OAPI — Otter API

[![Rust CI](https://github.com/matheo-1712/OAPI/actions/workflows/ci.yml/badge.svg)](https://github.com/matheo-1712/OAPI/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.85%2B%20(edition%202024)-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

OAPI is the asynchronous Rust API (Axum + Tokio) behind the *Antre des Loutres* community.
It generates Discord and Minecraft profile cards, monitors the community infrastructure,
handles Discord authentication and exposes community data stored in PocketBase.

## Table of contents

- [Features](#features)
- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Getting started](#getting-started)
- [Configuration](#configuration)
- [Usage](#usage)
- [Deployment](#deployment)
- [Testing and quality](#testing-and-quality)
- [Documentation](#documentation)
- [Contributing](#contributing)
- [License](#license)

## Features

| Area | Description |
| :--- | :--- |
| **Profile cards** | PNG summary cards for Discord and Minecraft profiles (messages, voice time, playtime, distance, blocks). Generation is skipped when the SHA-256 hash of the profile state is unchanged. |
| **Monitoring** | Parallel health checks of HTTP(S) services and Minecraft servers (Server List Ping), with a web dashboard. |
| **Authentication** | Discord OAuth2 login, stateless JWT session cookie, roles (`Admin`, `LoutreInvesti`, …), service API key for trusted servers and read-only `rom_` tokens for games. |
| **Friends** | Friendship system between Discord accounts. |
| **Badges** | Automatic badge attribution from Discord and Minecraft statistics, rules defined in `badges.yaml`. |
| **Discord activity** | JSON endpoints for Discord activity statistics, with request limits. |
| **Investor statistics** | PocketBase record counts, restricted to `LoutreInvesti` and `Admin` accounts. Returns `503` rather than partial data when the database fails. |
| **Daily tasks** | Background scheduler: account purge, badge refresh, then Minecraft head and skin refresh from Mojang. |
| **API documentation** | OpenAPI 3 specification generated with `utoipa`, served through Swagger UI. |

## Architecture

OAPI uses a layered architecture. Each layer only depends on the layers below it.

| Layer | Directory | Responsibility |
| :--- | :--- | :--- |
| Routes | `src/routes/` | Router assembly and middleware (authentication, CORS, rate limiting). |
| Handlers | `src/handlers/` | HTTP entry points: parameter extraction, status codes. |
| Actions | `src/actions/` | Use-case orchestration and external data fetching. |
| Services | `src/services/` | Business logic: image rendering, badges, scheduler, Mojang skins. |
| Models | `src/models/` | Typed DTOs and OpenAPI schemas. |
| Utils | `src/utils/` | PocketBase client, formatters, paths and constants. |

Static web pages are in `public/`, front-end tests in `tests/`, deployment files in `deploy/`.
See [docs/architecture.md](docs/architecture.md) for details.

**External dependencies:** PocketBase (data storage), the Discord API (OAuth2, profiles) and the
Mojang API (Minecraft skins).

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) stable, 1.85 or later (edition 2024)
- [Node.js](https://nodejs.org) 18 or later, to run the front-end tests
- A reachable PocketBase instance and its administrator credentials
- A Discord application (client ID, client secret, redirect URL)
- Docker and Docker Compose, for production deployment only

## Getting started

```bash
git clone https://github.com/matheo-1712/OAPI.git
cd OAPI
cp .env.example .env
```

Fill in `.env` (see [Environment variables](#environment-variables)), then start the server:

```bash
cargo run
```

The server listens on `http://127.0.0.1:3000` by default.

> [!WARNING]
> Daily tasks run at startup, including the purge of Discord accounts on the configured
> PocketBase. When working against a shared database, set `scheduler.enabled: false` in
> `config.yaml`. See [docs/daily_tasks.md](docs/daily_tasks.md).

## Configuration

### Configuration files

Configuration is loaded in this order, each source overriding the previous one:

1. **`default_config.yaml`**: default values and complete structure. Committed.
2. **`config.yaml`**: local overrides (host, port, monitored services, roles, scheduler).
   Ignored by Git. A commented template is created on first run when the file is missing.
3. **Environment variables**: secrets and environment mode.

See [docs/configuration.md](docs/configuration.md) for every key.

### Environment variables

Defined in `.env`, see [.env.example](.env.example). Never commit `.env`.

| Variable | Required | Description |
| :--- | :---: | :--- |
| `OAPI_ENV` | Yes | `development` or `production`. |
| `PB_URL` | Yes | PocketBase URL. |
| `PB_EMAIL` | Yes | PocketBase administrator email. |
| `PB_PASSWORD` | Yes | PocketBase administrator password. |
| `DISCORD_CLIENT_ID` | Yes | Discord application client ID. |
| `DISCORD_CLIENT_SECRET` | Yes | Discord application client secret. |
| `DISCORD_REDIRECT_URL` | Yes | OAuth2 callback, ending with `/api/auth/callback`. Must be HTTPS in production. |
| `JWT_SECRET` | Yes | Random secret of at least 32 bytes (`openssl rand -hex 32`). Startup fails if it is missing or too short. |
| `OAPI_SERVICE_API_KEY` | No | Key for trusted server-to-server calls, with administrator rights. Never expose it to browser code. |
| `RUST_LOG` | No | Log filter, for example `info,OAPI=debug,tower_http=debug`. |
| `POCKETBASE_NETWORK` | No | Docker network shared with PocketBase (Compose only). Default: `pocketbase_network`. |

## Usage

| URL | Description |
| :--- | :--- |
| `/` | Home page: login and badge refresh. |
| `/tools.html` | Card generation and personal Discord activity. |
| `/explorer.html` | Route catalogue built from the OpenAPI specification. |
| `/monitoring.html` | Monitoring dashboard, refreshed every 30 seconds. |
| `/swagger-ui` | Swagger UI. The specification is at `/api-docs/openapi.json`. |
| `/healthz` | Liveness probe, returns `204 No Content`. |

API routes are under the `/api` prefix (configurable with `server.routes.base`).

Generated images are written to `generated_images/`, outside `public/`. They are only served to
the profile owner or to an administrator.

## Deployment

Production runs with Docker Compose behind a reverse proxy that terminates TLS. The container
listens on port `9003` and joins the PocketBase Docker network.

```bash
docker compose up -d --build
```

Production requires `OAPI_ENV=production` and an HTTPS `DISCORD_REDIRECT_URL`. Production settings
are in [deploy/config.production.yaml](deploy/config.production.yaml).
The full procedure is in [docs/production.md](docs/production.md).

## Testing and quality

The project follows a zero-warnings policy. CI ([.github/workflows/ci.yml](.github/workflows/ci.yml))
runs formatting, Clippy, tests, release build and a Semgrep scan on every push to `master` and
`feat/*`, and on every pull request to `master`.

Run the same checks locally before pushing:

```bash
cargo fmt --all -- --check
```

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

```bash
cargo test --all-features
```

```bash
node --test tests/*.test.cjs
```

## Documentation

Documentation is written in French. English versions are available for some pages.

| Topic | French | English |
| :--- | :--- | :--- |
| Architecture | [architecture.md](docs/architecture.md) | [EN](docs/en/architecture.md) |
| Configuration | [configuration.md](docs/configuration.md) | [EN](docs/en/configuration.md) |
| Authentication | [AUTH.md](docs/AUTH.md) | [EN](docs/en/AUTH.md) |
| PocketBase integration | [pocketbase.md](docs/pocketbase.md) | [EN](docs/en/pocketbase.md) |
| Image generation | [generation_images.md](docs/generation_images.md) | [EN](docs/en/generation_images.md) |
| Monitoring | [monitoring.md](docs/monitoring.md) | [EN](docs/en/monitoring.md) |
| Web interfaces | [interfaces.md](docs/interfaces.md) | — |
| Badges | [badges.md](docs/badges.md) | — |
| Friends API | [friends.md](docs/friends.md) | — |
| Discord activity | [discord_statistics.md](docs/discord_statistics.md) | — |
| ROM linking (`rom_` tokens) | [rom_auth.md](docs/rom_auth.md), [rom_contract.md](docs/rom_contract.md) | — |
| Astro site integration | [astro.md](docs/astro.md) | — |
| Daily tasks | [daily_tasks.md](docs/daily_tasks.md) | — |
| Production deployment | [production.md](docs/production.md) | — |

## Contributing

1. Create a branch from `master` named `feat/<topic>` or `fix/<topic>`.
2. Add tests for every new feature or bug fix.
3. Make sure all checks in [Testing and quality](#testing-and-quality) pass.
4. Write commit messages following [Conventional Commits](https://www.conventionalcommits.org)
   (`feat(scope): …`, `fix(scope): …`, `docs(scope): …`).
5. Open a pull request to `master`.

## License

Distributed under the MIT License. See [LICENSE](LICENSE).

Maintained by [matheo-1712](https://github.com/matheo-1712).

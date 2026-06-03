# Guide: Configuration System

The OAPI project uses a hierarchical and flexible configuration system based on the `config` crate. this system manages default values while allowing local overrides and secrets via environment variables.

---

## Loading Hierarchy

Configuration sources are loaded in the following order (later ones overwrite earlier ones):

1.  **`default_config.yaml`**: Project default values. **This file is tracked by Git.**
2.  **`config.yaml`**: Local environment-specific overrides. **This file is ignored by Git.**
3.  **Environment Variables**: Secrets and sensitive settings (e.g., `PB_PASSWORD`).

---

## 1. Default Values (`default_config.yaml`)

This file contains the mandatory structure and base values:
- Server settings (host, port).
- Internal API route definitions.
- Monitoring configuration (list of services to monitor).

## 2. Local Overrides (`config.yaml`)

On the first run, a `config.yaml` file is automatically generated. You can uncomment lines there to modify the configuration without touching the code.

## 3. Secrets (.env)

Sensitive information is **never** stored in YAML files. It is read from the environment (or a `.env` file):
- `PB_EMAIL`: PocketBase admin email.
- `PB_PASSWORD`: Admin password.
- `PB_URL`: PocketBase instance URL.

---

## Data Structure

The configuration is structured into three main blocks:

### `server`
Manages the Axum server and internal routing.
- `host`: Listening IP.
- `port`: TCP port.
- `routes`: Dynamic paths for endpoints (e.g., `minecraft_summary: "/minecraft-summary/:id"`).

### `monitoring`
List of services to monitor.
- `discord`: Bots with a healthcheck endpoint.
- `minecraft`: Servers with host and port.
- `api`, `site`, `self_hosted`: Simple HTTP services.

### `auth` (Internal)
Groups PocketBase access credentials loaded via the environment.

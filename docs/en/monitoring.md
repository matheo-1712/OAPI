# Monitoring System (Health Check)

This module monitors the real-time status of various external services and aggregates their status into a single API endpoint.

## Configuration

Configuration is managed in `default_config.yaml` or `config.yaml`:

```yaml
monitoring:
  discord:
    - name: "Mateloutre"
      url: "https://mateloutre.fr/health"
  minecraft:
    - name: "Vanilla S4"
      host: "mc.antredesloutres.fr"
      port: 25565
  api: []
  site: []
```

### Service Types

1.  **Discord**: Checks a bot and extracts metadata (Uptime, Version, Ping).
2.  **Minecraft**: Pings the server via the Server List Ping (SLP) protocol to get player counts.
3.  **HTTP (Site, API, Self-hosted)**: Performs a simple `GET` and verifies success (2xx).

---

## Endpoints

### 1. Global Status
- **Route**: `/api/monitoring`
- **Action**: Runs all tests in parallel. The response time is capped by the slowest service (5s timeout).

### 2. Individual Check
- **Route**: `/api/monitoring/check/{type}/{name}`
- **Usage**: Allows refreshing a single specific service on the frontend dashboard without re-testing everything.

## Frontend Integration
The project provides a minimalist dark dashboard accessible at the root URL (`/monitoring.html`) that consumes these endpoints.

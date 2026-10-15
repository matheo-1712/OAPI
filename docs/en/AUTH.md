# Authentication & Authorization Documentation

This document describes the OAuth2 and JWT-based authentication system implemented in OAPI.

## Overview
OAPI uses Discord OAuth2 to authenticate users. Instead of storing sessions on the server (which uses memory and database I/O), we issue a stateless JSON Web Token (JWT) that is stored in an `auth_token` cookie.

## 1. OAuth2 Flow
1. **Login Route (`/api/auth/login`)**: When a user clicks "Se connecter", this route signs their return URL in a short-lived OAuth state and sets an `oauth_state` cookie, then redirects them to the Discord authorization page.
2. **Callback Route (`/api/auth/callback`)**: After Discord authorizes the user, it redirects here with a temporary `code`. The server exchanges this `code` for an access token, fetches the user's Discord profile and roles, verifies their existence in PocketBase, generates a JWT, sets the `auth_token` cookie, and redirects the user back to the `return_to` URL.
3. **Logout Route (`/api/auth/logout`)**: Clears the `auth_token` cookie and redirects the user safely to the home page or `return_to` URL.

## 2. JWT and Cookies
The generated JWT contains the following claims:
- `sub`: Discord User ID
- `username`: Discord Username
- `role`: The highest role assigned to the user (`Admin`, `LoutreInvesti`, or `Normal`).
- `exp`: Expiration timestamp (7 days)

The session cookie is HttpOnly and SameSite=Lax, with Secure enabled for HTTPS or production and a seven-day lifetime. The frontend checks /api/auth/me instead of reading the token. JWT_SECRET must contain at least 32 bytes after trimming; startup rejects missing or short secrets.

## 3. Middlewares
We implemented three cascading security middlewares in `src/handlers/middleware.rs`:
- **`require_auth`**: Validates the JWT signature. Rejects unauthenticated requests with HTTP 401. Allows any valid role.
- **`require_investor`**: Validates the JWT signature and checks if the role is `LoutreInvesti` or `Admin`. Rejects unauthorized requests with HTTP 403.
- **`require_admin`**: Validates the JWT signature and strictly requires the `Admin` role.

To protect a route, you wrap it using `.route_layer()`:
```rust
Router::new()
    .route("/admin/users", get(get_all_users))
    .route_layer(axum::middleware::from_fn(middleware::require_admin))
```

## 4. Emergency Kill-Switch
If cookies or secrets are compromised, you can instantly log out all active users by changing the `JWT_SECRET` environment variable in the `.env` file and restarting the server. Because the JWTs are stateless, changing the signature key invalidates all previously issued tokens immediately.

## OAuth state and private images

OAuth state is signed, random, expires after ten minutes, and must match the browser cookie before code exchange. The signed local return path is validated again at callback. Images are stored outside public and require owner or administrator authorization; Minecraft ownership uses the PocketBase discord_user relation. API and image responses use Cache-Control: no-store. Legacy public image files are no longer served and profiles regenerate on demand.

Set OAPI_ENV=production and an HTTPS Discord callback URL in production. TLS must be terminated by the server or reverse proxy.

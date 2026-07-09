# Authentication & Authorization Documentation

This document describes the OAuth2 and JWT-based authentication system implemented in OAPI.

## Overview
OAPI uses Discord OAuth2 to authenticate users. Instead of storing sessions on the server (which uses memory and database I/O), we issue a stateless JSON Web Token (JWT) that is stored in an `auth_token` cookie.

## 1. OAuth2 Flow
1. **Login Route (`/api/auth/login`)**: When a user clicks "Se connecter", this route saves their current URL in a `return_to` cookie, then redirects them to the Discord authorization page.
2. **Callback Route (`/api/auth/callback`)**: After Discord authorizes the user, it redirects here with a temporary `code`. The server exchanges this `code` for an access token, fetches the user's Discord profile and roles, verifies their existence in PocketBase, generates a JWT, sets the `auth_token` cookie, and redirects the user back to the `return_to` URL.
3. **Logout Route (`/api/auth/logout`)**: Clears the `auth_token` cookie and redirects the user safely to the home page or `return_to` URL.

## 2. JWT and Cookies
The generated JWT contains the following claims:
- `sub`: Discord User ID
- `username`: Discord Username
- `role`: The highest role assigned to the user (`Admin`, `LoutreInvesti`, or `Normal`).
- `exp`: Expiration timestamp (7 days)

The cookie is stored with `HttpOnly = false` to allow frontend JavaScript to easily check the authentication state via `document.cookie`, while the backend routes strictly verify the cryptographic signature of the JWT on every request.

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

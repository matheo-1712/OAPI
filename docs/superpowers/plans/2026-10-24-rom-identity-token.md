# Authentification ROM par code de liaison — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lier une ROM / un appareil à un compte Discord via un code de 6 chiffres généré sur rom-patcher, échangé en jeu contre un token `rom_…` qui donne accès en lecture à la liste d'amis.

**Architecture:** Une ligne `discord_identity_token` par appareil. Actions PocketBase dans `src/actions/rom_auth_actions.rs`, handlers dans `src/handlers/rom_auth_handler.rs`, routes `/auth/rom/*`. `GET /friends/:account` passe dans un routeur à part, protégé par un nouveau middleware `require_friends_reader` qui accepte en plus `Bearer rom_…` ; toutes les autres routes continuent de refuser ces tokens (ils échouent la comparaison au token de service → 401).

**Tech Stack:** Rust 2024, axum 0.7, PocketBase (REST), sha2, rand 0.8 (déjà dans `Cargo.lock` via oauth2), mockito pour les tests.

**Spec:** `docs/superpowers/specs/2026-10-24-rom-identity-token-design.md`

## Global Constraints

- Collection : `discord_identity_token` ; champs `discord_user`, `label`, `token`, `linking_code`, `is_disabled`, `linking_code_expires_at`, `created`, `updated`.
- Code : exactement 6 caractères, chacun dans `1..=9`. Validité : 15 minutes. Usage unique.
- Token : `rom_` + 64 caractères hexadécimaux minuscules (32 octets aléatoires, `OsRng`). Stocké : `sha256` hex minuscule du token complet (préfixe inclus). Jamais stocké ni journalisé en clair.
- Label : facultatif, trim, caractères de contrôle retirés, tronqué à 32 caractères.
- Générer un code supprime les lignes `token = ''` du joueur et toutes les lignes `token = ''` expirées.
- Un token `rom_` n'est accepté que sur `GET /friends/:account` ; rôle `Normal`, `sub` = `discord_id` du `discord_users` lié ; ligne avec `is_disabled = true` refusée.
- Messages d'erreur en français, comme `friends_handler`. Erreurs PocketBase → `502`, détails dans les logs uniquement.
- Les filtres PocketBase ne reçoivent que des valeurs validées (code 6 chiffres, hash hex, `valid_id`).

## Review Focus

1. Code expiré ou déjà échangé saisi en jeu → `404` identique à un code inconnu (test Task 2 `exchange_rejects_expired_or_used_code` via le filtre envoyé).
2. `Bearer rom_…` sur une autre route protégée (`/badges/me/refresh`, `/discord/stats/me`) → `401`, jamais `400` ni accès (test Task 4).
3. Token `rom_` mal formé (mauvaise longueur, majuscules, non-hex) → `401` sans appel PocketBase (test Task 1 `parse_rom_bearer` + Task 4).
4. Révocation d'un appareil d'un autre joueur → `404`, la ligne n'est pas supprimée (test Task 2 `revoke_only_deletes_own_device`).
5. Token de service sur `/auth/rom/code` → `403` (pas de compte personnel ; `resolve_account("me")` le refuse déjà, test Task 3).

---

### Task 1: Fondations (constante, modèles, utilitaires purs)

**Files:**
- Modify: `Cargo.toml` (ajouter `rand = "0.8"`)
- Modify: `src/utils/constants.rs` (ajouter `DISCORD_IDENTITY_TOKEN_COLLECTION`)
- Create: `src/models/rom_auth.rs`, Modify: `src/models/mod.rs` (`pub mod rom_auth;`)
- Create: `src/actions/rom_auth_actions.rs`, Modify: `src/actions/mod.rs` (`pub mod rom_auth_actions;`)
- Modify: `src/utils/pocketbase.rs` (constructeur de test)

**Interfaces:**
- Produces:
  - `pub const DISCORD_IDENTITY_TOKEN_COLLECTION: &str = "discord_identity_token";`
  - `models::rom_auth` : `RomCodeResponse { code: String, expires_at: String, device_id: String }`, `RomTokenRequest { code: String }` (Deserialize), `RomTokenResponse { token: String, device_id: String, label: String }`, `RomDevice { id, label, created, updated: String, is_disabled: bool }`, `RomDeviceList { devices: Vec<RomDevice> }` — tous `Serialize + ToSchema` (sauf la requête : `Deserialize + ToSchema`). `IdentityTokenRecord { id, discord_user, label, token, linking_code: String, is_disabled: bool, created, updated: String }` (Deserialize, `#[serde(default)]` sur les champs texte/bool).
  - `pub const ROM_TOKEN_PREFIX: &str = "rom_";`, `pub const CODE_TTL_MINUTES: i64 = 15;`
  - `pub fn generate_code() -> String`
  - `pub fn valid_code(code: &str) -> bool`
  - `pub fn generate_token() -> String`
  - `pub fn hash_token(token: &str) -> String`
  - `pub fn parse_rom_bearer(value: &str) -> Option<&str>` — reçoit la valeur brute de l'en-tête `Authorization`, rend le token complet (`rom_…`) s'il est `Bearer rom_` + 64 hex minuscules, sinon `None`.
  - `pub fn sanitize_label(label: Option<&str>) -> String`
  - `#[cfg(test)] pub fn PocketbaseClient::for_test(base_url: String, token: &str) -> Self`

- [ ] **Step 1: Écrire les tests (module `tests` de `rom_auth_actions.rs`)**

```rust
#[test]
fn codes_are_six_digits_from_one_to_nine() {
    for _ in 0..200 {
        let code = generate_code();
        assert_eq!(code.len(), 6);
        assert!(code.bytes().all(|c| (b'1'..=b'9').contains(&c)));
        assert!(valid_code(&code));
    }
    for bad in ["", "12345", "1234567", "123450", "12a456", " 12345"] {
        assert!(!valid_code(bad));
    }
}

#[test]
fn tokens_are_prefixed_random_hex_and_hashed() {
    let token = generate_token();
    assert!(token.starts_with("rom_"));
    assert_eq!(token.len(), 4 + 64);
    assert_ne!(token, generate_token());
    let hash = hash_token(&token);
    assert_eq!(hash.len(), 64);
    assert_ne!(hash, token[4..]);
    assert_eq!(hash, hash_token(&token));
}

#[test]
fn rom_bearer_requires_exact_format() {
    let token = generate_token();
    assert_eq!(parse_rom_bearer(&format!("Bearer {token}")), Some(token.as_str()));
    for bad in [token.clone(), format!("bearer {token}"), format!("Bearer {}", token.to_uppercase()),
                format!("Bearer {}x", token), "Bearer rom_abc".into(), format!("Bearer {}", &token[4..])] {
        assert_eq!(parse_rom_bearer(&bad), None, "{bad}");
    }
}

#[test]
fn labels_are_trimmed_cleaned_and_bounded() {
    assert_eq!(sanitize_label(None), "");
    assert_eq!(sanitize_label(Some("  Console salon  ")), "Console salon");
    assert_eq!(sanitize_label(Some("a\nb\u{7}c")), "abc");
    assert_eq!(sanitize_label(Some(&"é".repeat(40))).chars().count(), 32);
}
```

- [ ] **Step 2: Lancer** `cargo test rom_auth_actions` → échec de compilation (fonctions absentes).
- [ ] **Step 3: Implémenter** constante, modèles, `rand` dans `Cargo.toml`, utilitaires (`rand::rngs::OsRng` + `Rng::gen_range(1..=9)` / `RngCore::fill_bytes`, hex via `format!("{:02x}")`, `sha2::Sha256`), et `PocketbaseClient::for_test`.
- [ ] **Step 4: Lancer** `cargo test rom_auth_actions` → PASS.
- [ ] **Step 5: Commit** `feat(rom-auth): add identity token models and helpers`

### Task 2: Actions PocketBase

**Files:**
- Modify: `src/actions/rom_auth_actions.rs`

**Interfaces:**
- Consumes: Task 1 ; `friends_actions::resolve_account` n'est **pas** utilisé ici (le handler résout le compte).
- Produces (toutes sur `&PocketbaseClient`, `account` = ID `discord_users` déjà résolu) :
  - `pub enum RomAuthError { Invalid(&'static str), NotFound(&'static str), Upstream(String) }` + `impl From<String>`.
  - `pub async fn create_code(pb, account: &str, label: &str) -> Result<RomCodeResponse, RomAuthError>`
  - `pub async fn exchange_code(pb, code: &str) -> Result<RomTokenResponse, RomAuthError>`
  - `pub async fn list_devices(pb, account: &str) -> Result<RomDeviceList, String>`
  - `pub async fn revoke_device(pb, account: &str, device: &str) -> Result<(), RomAuthError>`
  - `pub async fn claims_for_token(pb, token: &str) -> Result<Option<JwtClaims>, String>`

Comportement :
- `create_code` : supprime chaque ligne de `token = '' && (discord_user = '{account}' || linking_code_expires_at < @now)` ; puis jusqu'à 5 tirages : `generate_code()`, ignorer s'il existe une ligne `linking_code = '{code}'`, sinon `create_record` avec `discord_user`, `label`, `linking_code`, `linking_code_expires_at` = maintenant + 15 min au format PocketBase `%Y-%m-%d %H:%M:%S%.3fZ` (UTC), `token: ""`, `is_disabled: false`. 5 collisions → `Upstream("No free linking code")`. `device_id` = `id` du record créé.
- `exchange_code` : `!valid_code` → `Invalid("Code invalide.")` ; cherche `linking_code = '{code}' && token = '' && linking_code_expires_at > @now` ; absent → `NotFound("Code inconnu ou expiré.")` ; sinon `generate_token()`, `update_record` avec `token = hash_token`, `linking_code = ""`, `linking_code_expires_at = ""`.
- `list_devices` : `discord_user = '{account}' && token != ''`, trié par l'ordre PocketBase (pas d'exigence).
- `revoke_device` : `!valid_id(device)` → `Invalid` ; cherche `id = '{device}' && discord_user = '{account}' && token != ''` ; absent → `NotFound("Appareil introuvable.")` ; sinon `delete_record`.
- `claims_for_token` : cherche `token = '{hash}' && is_disabled = false` ; absent → `Ok(None)` ; sinon lit `discord_users` `id = '{discord_user}'` (champs `discord_id`, `username`) ; absent ou `discord_id` non `valid_id` → `Ok(None)` ; sinon `JwtClaims { sub: discord_id, username, role: Role::Normal, exp: 0, iat: 0, profile: None }`.

- [ ] **Step 1: Écrire les tests mockito** (`PocketbaseClient::for_test(server.url(), "t")`, matcher `Matcher::UrlEncoded("filter", …)`) :
  - `create_code_cleans_pending_rows_then_creates_a_fresh_code` : le GET de nettoyage avec le filtre exact ci-dessus renvoie une ligne `{"id":"old"}` → DELETE `/records/old` attendu ; GET de collision renvoie 0 ligne ; POST attendu avec `Matcher::PartialJson(json!({"discord_user":"acc","label":"PC","token":""}))` ; la réponse a `valid_code(&r.code)` et `device_id == "new"`.
  - `exchange_rejects_expired_or_used_code` : GET avec filtre `linking_code = '123456' && token = '' && linking_code_expires_at > @now` renvoie 0 ligne → `NotFound` ; aucun PATCH (mock `.expect(0)`). `exchange_code(&pb, "12a456")` → `Invalid` sans appel.
  - `exchange_stores_only_the_hash` : la ligne existe → PATCH dont le corps a `token` = 64 hex ≠ token renvoyé, `linking_code: ""` ; `hash_token(&r.token) == token_du_corps` (capturer via `Matcher::Regex` ou en relisant depuis `with_body_from_request`).
  - `revoke_only_deletes_own_device` : GET avec `discord_user = 'acc'` renvoie 0 ligne → `NotFound`, DELETE `.expect(0)`.
  - `disabled_or_unknown_tokens_have_no_claims` : GET avec `token = '{hash}' && is_disabled = false` → 0 ligne → `Ok(None)` ; avec une ligne + `discord_users` `{"discord_id":"123","username":"Loutre"}` → `sub == "123"`, `role == Role::Normal`.
- [ ] **Step 2: Lancer** `cargo test rom_auth_actions` → FAIL.
- [ ] **Step 3: Implémenter** les cinq fonctions.
- [ ] **Step 4: Lancer** `cargo test rom_auth_actions` → PASS.
- [ ] **Step 5: Commit** `feat(rom-auth): link codes, token exchange and device actions`

### Task 3: Handlers et routes `/auth/rom/*`

**Files:**
- Create: `src/handlers/rom_auth_handler.rs`, Modify: `src/handlers/mod.rs`
- Modify: `src/routes/mod.rs`, `src/main.rs` (OpenAPI `paths`, `schemas`, liste des chemins privés, test de présence des chemins)

**Interfaces:**
- Consumes: Task 2 ; `friends_actions::resolve_account(&pb, "me", &claims)` (rend `FriendError` ; mapper `Forbidden` → 403, `NotFound` → 404, `Upstream` → 502).
- Produces handlers :
  - `create_code(Query<RomCodeQuery { label: Option<String> }>, Extension<JwtClaims>) -> Result<Json<RomCodeResponse>, (StatusCode, String)>` — `POST /auth/rom/code`
  - `exchange(Json<RomTokenRequest>) -> Result<Json<RomTokenResponse>, (StatusCode, String)>` — `POST /auth/rom/token`
  - `devices(Extension<JwtClaims>) -> Result<Json<RomDeviceList>, …>` — `GET /auth/rom/devices`
  - `revoke(Path<String>, Extension<JwtClaims>) -> Result<StatusCode, …>` — `POST /auth/rom/devices/:device/revoke`, `204`

Routage : `code`, `devices`, `revoke` dans un routeur `rom_site_routes` avec `require_auth` puis `site_cors` (même ordre que `friends_routes`) ; `token` sans middleware d'auth. `RomAuthError` : `Invalid` → 400, `NotFound` → 404, `Upstream` → 502 avec `tracing::error!` et `"Liaison ROM temporairement indisponible."`. Chemins `/auth/rom/code`, `/auth/rom/devices`, `/auth/rom/devices/{device}/revoke` ajoutés à la liste de sécurité OpenAPI de `main.rs` (comme `/auth/site/me`).

- [ ] **Step 1: Écrire les tests (module `tests` de `routes/mod.rs`)** :
  - `rom_site_routes_require_a_session` : pour `POST /auth/rom/code`, `GET /auth/rom/devices`, `POST /auth/rom/devices/abc/revoke`, avec cookie `""` puis `auth_token=forged` et `origin` = site → `401`, `cache-control: no-store`, `access-control-allow-origin` = site.
  - `rom_token_exchange_rejects_malformed_codes_without_auth` : `POST /auth/rom/token` corps `{"code":"12a456"}` → `400` ; corps absent / non JSON → `4xx` (pas 5xx).
  - `service_token_cannot_create_a_link_code` (module `tests` de `rom_auth_handler.rs`) : `friends_actions::resolve_account(&PocketbaseClient::for_test("http://127.0.0.1:9".into(), "t"), "me", &JwtClaims { sub: SERVICE_SUBJECT.into(), role: Role::Admin, … })` → `Err(FriendError::Forbidden)`, que le handler mappe en `403` (aucune requête : `service:astro` échoue `valid_id`).
  - Étendre le test OpenAPI de `main.rs` avec les quatre chemins.
- [ ] **Step 2: Lancer** `cargo test` → FAIL.
- [ ] **Step 3: Implémenter** handlers (`#[utoipa::path]` tag `"ROM"`), routes, OpenAPI.
- [ ] **Step 4: Lancer** `cargo test` → PASS.
- [ ] **Step 5: Commit** `feat(rom-auth): expose link code, token exchange and device routes`

### Task 4: Token `rom_` sur la liste d'amis

**Files:**
- Modify: `src/handlers/middleware.rs`, `src/routes/mod.rs`

**Interfaces:**
- Consumes: `parse_rom_bearer`, `claims_for_token` (Task 1–2).
- Produces: `pub async fn require_friends_reader(jar: CookieJar, request: Request, next: Next) -> Result<Response, (StatusCode, String)>`.

Comportement : si l'en-tête `Authorization` est unique et `parse_rom_bearer` réussit → `PocketbaseClient::new()` + `login()` + `claims_for_token` ; `Ok(Some(claims))` → insérer dans les extensions et continuer ; `Ok(None)` → `401 "Unauthorized. Please log in."` ; erreur → `502` + log. Une valeur commençant par `Bearer rom_` mais mal formée → `401` sans PocketBase. Sinon → `verify_and_check_role(jar, request, next, &[], service_key)` inchangé. Dans `routes/mod.rs`, sortir `GET /friends/:account` de `friends_routes` vers un routeur `friends_read_routes` avec `require_friends_reader` puis `site_cors`.

- [ ] **Step 1: Écrire les tests** :
  - `rom_tokens_only_open_the_friend_list` (routes) : `Authorization: Bearer rom_<64 hex>` sur `POST /badges/me/refresh`, `GET /discord/stats/me`, `POST /friends/me/user/request`, `POST /auth/rom/code` → `401` (jamais `400`).
  - `malformed_rom_tokens_are_rejected_before_pocketbase` : `GET /friends/me` avec `Bearer rom_abc` → `401`.
  - Le test existant `friends_require_authentication_and_reject_foreign_origins` doit toujours passer.
- [ ] **Step 2: Lancer** `cargo test` → FAIL sur le nouveau test si le routeur n'est pas séparé.
- [ ] **Step 3: Implémenter** le middleware et la séparation du routeur.
- [ ] **Step 4: Lancer** `cargo test` puis `cargo clippy --all-targets -- -D warnings` → PASS, aucun warning.
- [ ] **Step 5: Commit** `feat(rom-auth): accept rom tokens on the friend list`

### Task 5: Documentation

**Files:**
- Create: `docs/rom_auth.md`
- Modify: `docs/friends.md` (§1 : ligne « Token ROM »), `docs/AUTH.md` (renvoi vers `rom_auth.md`), `docs/pocketbase.md` (collection + index)

Contenu de `rom_auth.md` : flux (site → code → jeu → token), tableau des routes et codes d'erreur, format du token, révocation, exemples `curl`/`fetch` (`credentials: "include"` côté site), configuration PocketBase requise : `token` sans Nonempty, index `CREATE UNIQUE INDEX idx_identity_token ON discord_identity_token (token) WHERE token != ''` et idem pour `linking_code`, et la limite connue : deux échanges simultanés du même code peuvent rendre le premier token inutilisable (relancer la liaison).

- [ ] **Step 1: Rédiger** les fichiers.
- [ ] **Step 2: Commit** `docs(rom-auth): document ROM linking flow`

# Skins Minecraft dans PocketBase — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Une tâche quotidienne `skins` remplit `players.head` et `players.skin` à partir de la texture Mojang, et l'image de résumé Minecraft lit `head` depuis PocketBase.

**Architecture:** Trois modules dans `src/services/` : `skin_render` (rendu pur à partir d'une texture), `mojang` (lecture du profil Mojang), `skin_service` (tâche quotidienne qui orchestre et envoie dans PocketBase). `PocketbaseClient` gagne une mise à jour `multipart`.

**Tech Stack:** Rust 2024, tokio, reqwest 0.12 (+ `multipart`), `image` 0.25, `base64` 0.22, mockito pour les tests.

**Spec:** `docs/superpowers/specs/2026-10-25-minecraft-skins-design.md`

## Global Constraints

- **Ne rien commiter** : l'utilisateur commite lui-même. Les étapes « Commit » sont remplacées par « laisser les changements non commités ».
- Branche : `feat/minecraft-skins`.
- `head` : 128×128 PNG. `skin` : 128×256 PNG (canevas 16×32 agrandi ×8). Redimensionnement `FilterType::Nearest`.
- `skin_hash` = `<hash>:<modèle>`, modèle `classic` ou `slim`.
- Délai minimal entre deux requêtes Mojang : 1,5 s (`MOJANG_DELAY` dans `skin_service.rs`).
- URL Mojang : `https://sessionserver.mojang.com/session/minecraft/profile/<uuid sans tirets>`.
- Les textures sont téléchargées en `https://textures.minecraft.net/...` (le `http` renvoyé par Mojang est réécrit).
- Compte Minecraft = `account_id` qui passe `image_service::valid_uuid`.
- Messages de log en anglais, commentaires et docs dans la langue du fichier existant.
- Vérification de chaque tâche : `cargo test` et `cargo clippy --all-targets` sans nouvel avertissement.

## Review Focus

- PocketBase renvoie `""` (pas `null`) pour un champ fichier vide : `head: ""` doit compter comme absent, sinon un joueur avec `skin_hash` mais sans image ne serait jamais régénéré (test Task 4, `empty_file_fields_force_regeneration`).
- `account_id` stocké avec tirets : l'URL Mojang doit être sans tirets (test Task 2, `profile_url_strips_dashes`).
- Mojang renvoie l'URL de texture en `http://` : elle doit être réécrite en `https://` pour passer `image_assets` (test Task 2, `texture_url_is_rewritten_to_https`).
- Texture de taille inattendue (ni 64×64 ni 64×32) : échec pour ce joueur, pas de panique (test Task 1, `rejects_unexpected_texture_size`).
- `429` au milieu du passage : résumé partiel renvoyé en `Ok`, pas d'`Err` qui masquerait les joueurs déjà traités (test Task 4, `rate_limit_stops_the_run_with_partial_summary`).

---

### Task 1: Rendu de la tête et du corps

**Files:**
- Create: `src/services/skin_render.rs`
- Modify: `src/services/mod.rs` (ajouter `pub mod skin_render;`)

**Interfaces:**
- Produces:
  - `pub enum SkinModel { Classic, Slim }` (`Clone, Copy, Debug, PartialEq`), avec `pub fn as_str(self) -> &'static str` (`"classic"` / `"slim"`).
  - `pub fn render_head(texture: &RgbaImage) -> Result<RgbaImage, String>` → 128×128.
  - `pub fn render_body(texture: &RgbaImage, model: SkinModel) -> Result<RgbaImage, String>` → 128×256.
  - `pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String>`.

- [ ] **Step 1: Écrire les tests** (module `#[cfg(test)]` du fichier). Helper `texture(w, h)` : `RgbaImage` transparent ; helper `fill(img, x, y, w, h, color)`.
  - `head_is_128_and_uses_face_then_hat` : texture 64×64, face `(8,8,8,8)` rouge, pixel `(40,8)` du chapeau bleu opaque. Sortie 128×128 ; pixel `(0,0)` bleu ; pixel `(127,127)` rouge.
  - `body_is_128x256_with_parts_in_place` : texture 64×64 classique, torse `(20,20,8,12)` vert, jambe gauche `(20,52,4,12)` jaune. Sortie 128×256 ; pixel `(4*8, 8*8)` vert ; pixel `(8*8, 20*8)` jaune ; pixel `(0,0)` transparent (alpha 0).
  - `slim_arms_are_three_pixels_wide` : texture 64×64, bras droit `(44,20,3,12)` opaque. En `Slim`, pixel `(1*8, 8*8)` opaque et `(0, 8*8)` transparent ; en `Classic`, `(0, 8*8)` opaque.
  - `legacy_texture_mirrors_right_limbs` : texture 64×32, bras droit `(44,20,4,12)` avec la colonne x=44 rouge et le reste blanc. Dans le corps, colonne canevas 0 (bras droit) rouge et colonne canevas 15 (bras gauche, miroir) rouge.
  - `legacy_opaque_hat_is_ignored` : texture 64×32, face rouge, zone chapeau `(40,8,8,8)` noire opaque. Pixel `(0,0)` de `render_head` rouge.
  - `rejects_unexpected_texture_size` : texture 128×128 → `render_head` et `render_body` renvoient `Err`.
  - `encode_png_round_trips` : `image::load_from_memory(&encode_png(&img)?)` redonne les mêmes dimensions.

- [ ] **Step 2: Lancer les tests et vérifier qu'ils échouent**

Run: `cargo test skin_render`
Expected: échec de compilation (fonctions absentes).

- [ ] **Step 3: Implémenter `skin_render.rs`**

Coordonnées (x, y, w, h) → position canevas 16×32, `L` = 4 (`Classic`) ou 3 (`Slim`) :

| Partie | Base | Extérieur | Position |
| --- | --- | --- | --- |
| Tête | (8,8,8,8) | (40,8,8,8) | (4,0) |
| Torse | (20,20,8,12) | (20,36,8,12) | (4,8) |
| Bras droit | (44,20,L,12) | (44,36,L,12) | (4−L,8) |
| Bras gauche | (36,52,L,12) | (52,52,L,12) | (12,8) |
| Jambe droite | (4,20,4,12) | (4,36,4,12) | (4,20) |
| Jambe gauche | (20,52,4,12) | (4,52,4,12) | (8,20) |

Format 64×32 : seules les parties « Base » de la tête, du torse, du bras droit et de la jambe droite existent ; bras/jambe gauches = `imageops::flip_horizontal` des droits ; pas de couche extérieure sauf le chapeau, ignoré s'il n'a aucun pixel d'alpha < 255. Les couches se posent avec `imageops::overlay` (respecte l'alpha). Agrandissement `imageops::resize(..., FilterType::Nearest)`.

- [ ] **Step 4: Lancer les tests et vérifier qu'ils passent**

Run: `cargo test skin_render`
Expected: 7 tests PASS.

- [ ] **Step 5: Laisser les changements non commités**

---

### Task 2: Lecture du profil Mojang

**Files:**
- Create: `src/services/mojang.rs`
- Modify: `src/services/mod.rs` (`pub mod mojang;`), `Cargo.toml` (`base64 = "0.22"`)

**Interfaces:**
- Consumes: `skin_render::SkinModel`.
- Produces:
  - `pub const SESSION_SERVER_URL: &str = "https://sessionserver.mojang.com";`
  - `pub struct SkinTexture { pub url: String, pub hash: String, pub model: SkinModel }` (`Debug, PartialEq, Clone`), avec `pub fn skin_hash(&self) -> String` → `"<hash>:<model>"`.
  - `pub enum ProfileLookup { Skin(SkinTexture), NoSkin, Unknown, RateLimited }` (`Debug, PartialEq`).
  - `pub fn profile_url(base_url: &str, uuid: &str) -> String`.
  - `pub async fn fetch_skin(base_url: &str, uuid: &str) -> Result<ProfileLookup, String>`.

- [ ] **Step 1: Écrire les tests** (mockito ; helper `profile_body(textures_json: Value)` qui encode en base64 `{"textures": ...}` dans `{"id":"…","name":"…","properties":[{"name":"textures","value":"<b64>"}]}`).
  - `profile_url_strips_dashes` : `profile_url("https://x", "01234567-89ab-cdef-0123-456789abcdef")` == `"https://x/session/minecraft/profile/0123456789abcdef0123456789abcdef"`.
  - `classic_skin_is_read` : `200` avec `{"SKIN":{"url":"http://textures.minecraft.net/texture/abc"}}` → `Skin(SkinTexture { url: "https://textures.minecraft.net/texture/abc", hash: "abc", model: Classic })`.
  - `slim_skin_is_read` : `metadata: {"model":"slim"}` → `model: Slim` ; `skin_hash()` == `"abc:slim"`.
  - `texture_url_is_rewritten_to_https` : couvert par `classic_skin_is_read` (assertion explicite sur le préfixe `https://`).
  - `missing_skin_is_no_skin` : `{"textures":{}}` → `NoSkin`.
  - `unknown_profile` : statut `204` puis statut `404` → `Unknown`.
  - `rate_limited` : `429` → `RateLimited`.
  - `invalid_payload_is_error` : `200` avec `{"properties":[{"name":"textures","value":"pas-du-base64"}]}` → `Err`.
  - `texture_from_other_host_is_error` : URL de texture `http://evil.example/texture/abc` → `Err`.

- [ ] **Step 2: Lancer les tests et vérifier qu'ils échouent**

Run: `cargo test mojang`
Expected: échec de compilation.

- [ ] **Step 3: Implémenter `mojang.rs`**

Client `reqwest` avec timeout 10 s, sans redirection. Décodage base64 `base64::engine::general_purpose::STANDARD`. Le hash est le dernier segment du chemin de l'URL de texture ; l'hôte doit être `textures.minecraft.net`, sinon `Err`.

- [ ] **Step 4: Lancer les tests et vérifier qu'ils passent**

Run: `cargo test mojang`
Expected: 9 tests PASS.

- [ ] **Step 5: Laisser les changements non commités**

---

### Task 3: Envoi multipart et hôte des textures

**Files:**
- Modify: `Cargo.toml` (`reqwest` : `features = ["json", "multipart"]`)
- Modify: `src/utils/pocketbase.rs` (nouvelle méthode + test dans le `mod tests` existant)
- Modify: `src/utils/image_assets.rs` (`permitted` + son test)

**Interfaces:**
- Produces:
  - `PocketbaseClient::update_record_multipart(&self, collection: &str, id: &str, fields: &[(&str, String)], files: Vec<(&str, &str, Vec<u8>)>) -> Result<serde_json::Value, String>` — `files` = `(champ, nom de fichier, octets)`, envoyés en `image/png`. Même gestion d'erreur et du jeton que `update_record`, timeout 30 s.
  - `image_assets::download` accepte `https://textures.minecraft.net/...`.

- [ ] **Step 1: Écrire les tests**
  - `update_record_multipart_sends_files_and_fields` (dans `pocketbase.rs`) : mock `PATCH /api/collections/players/records/p1`, `match_header("content-type", Matcher::Regex("^multipart/form-data".into()))`, `match_body(Matcher::AllOf(vec![Regex(r#"name="skin_hash""#), Regex("abc:classic"), Regex(r#"name="head"; filename="head.png""#), Regex(r#"name="skin"; filename="skin.png""#)]))`, statut 200 `{"id":"p1"}` ; appel avec `fields = [("skin_hash", "abc:classic")]` et deux fichiers ; `mock.assert_async()`.
  - `update_record_multipart_reports_failure` : statut 400 → `Err` contenant `"400"`.
  - Ajouter `"https://textures.minecraft.net/texture/abc"` aux URL acceptées et `"http://textures.minecraft.net/texture/abc"` aux refusées dans `asset_urls_reject_untrusted_origins_credentials_and_internal_targets`.

- [ ] **Step 2: Lancer les tests et vérifier qu'ils échouent**

Run: `cargo test -- multipart asset_urls`
Expected: échec de compilation puis du test d'URL.

- [ ] **Step 3: Implémenter** `update_record_multipart` avec `reqwest::multipart::Form` / `Part::bytes(..).file_name(..).mime_str("image/png")`, et ajouter `"textures.minecraft.net"` au `matches!` de `permitted`.

- [ ] **Step 4: Lancer les tests et vérifier qu'ils passent**

Run: `cargo test -- pocketbase image_assets`
Expected: PASS.

- [ ] **Step 5: Laisser les changements non commités**

---

### Task 4: Tâche quotidienne `skins`

**Files:**
- Create: `src/services/skin_service.rs`
- Modify: `src/services/mod.rs` (`pub mod skin_service;`)
- Modify: `src/services/scheduler.rs:55-64` (ajout `skins_task`, `DAILY_TASKS: [_; 3]`) et son test ligne 214
- Modify: `docs/daily_tasks.md`, `docs/pocketbase.md`, `docs/en/pocketbase.md`

**Interfaces:**
- Consumes: `mojang::{fetch_skin, ProfileLookup, SkinTexture, SESSION_SERVER_URL}`, `skin_render::{render_head, render_body, encode_png}`, `PocketbaseClient::update_record_multipart`, `image_assets::download`, `image_service::valid_uuid`, `scheduler::{TaskSummary, BoxFuture}` (alias maison de `scheduler.rs:16`, pas `futures`).
- Produces:
  - `pub const MOJANG_DELAY: Duration = Duration::from_millis(1500);`
  - `pub type TextureFetcher = for<'a> fn(&'a str) -> BoxFuture<'a, Option<DynamicImage>>;`
  - `pub async fn refresh_all(pb: &PocketbaseClient) -> Result<TaskSummary, String>` → appelle `refresh_all_with(pb, SESSION_SERVER_URL, MOJANG_DELAY, download_texture)` où `download_texture` enveloppe `image_assets::download`.
  - `async fn refresh_all_with(pb, mojang_url: &str, delay: Duration, fetch_texture: TextureFetcher) -> Result<TaskSummary, String>` (privée, pour les tests).
  - `scheduler::DAILY_TASKS` == `[("purge", …), ("badges", …), ("skins", …)]`.

Déroulé de `refresh_all_with` :
1. `list_all_records::<Value>("players", "")`, garder les records dont `account_id` passe `valid_uuid`.
2. Grouper par `account_id` sans tirets ni casse ; un seul `fetch_skin` par groupe, `tokio::time::sleep(delay)` avant chaque appel sauf le premier.
3. `RateLimited` → `warn!`, arrêter la boucle, renvoyer le résumé partiel en `Ok`. `Unknown` / `NoSkin` → chaque record du groupe compte en `succeeded`, sans écriture. `Err` → chaque record du groupe en `failed`.
4. `Skin(t)` : pour chaque record, si `skin_hash == t.skin_hash()` et `head`, `skin` non vides → `succeeded`. Sinon (rendu fait une seule fois par groupe, à la demande) : `fetch_texture(&t.url)` → `render_head` / `render_body` → `encode_png` → `update_record_multipart(players, id, [("skin_hash", t.skin_hash())], [("head","head.png",…), ("skin","skin.png",…)])`. Toute erreur → `failed` pour ce record.
5. `processed` = nombre de records Minecraft examinés avant l'arrêt ; un log `info!` donne en plus le nombre de records mis à jour.

- [ ] **Step 1: Écrire les tests** (mockito pour PocketBase et Mojang sur le même `Server`, `delay = Duration::ZERO`, `fetch_texture` de test renvoyant une texture 64×64 générée en mémoire ; réutiliser le helper `page(items)` du style de `badge_service`).
  - `updates_changed_skin` : un record `{"id":"p1","account_id":UUID,"skin_hash":"","head":"","skin":""}`, Mojang renvoie hash `abc` → un `PATCH` multipart sur `p1` ; résumé `{processed:1, succeeded:1, failed:[]}`.
  - `unchanged_skin_is_not_uploaded` : `skin_hash:"abc:classic"`, `head:"h.png"`, `skin:"s.png"` → aucun `PATCH` (`mock.expect(0)`), `succeeded: 1`.
  - `empty_file_fields_force_regeneration` : `skin_hash:"abc:classic"`, `head:""` → un `PATCH`.
  - `steam_accounts_are_ignored` : un record `account_id:"76561198000000000"` → aucun appel Mojang, `processed: 0`.
  - `duplicate_account_ids_call_mojang_once` : deux records même UUID (un avec tirets) → mock Mojang `expect(1)`, deux `PATCH`.
  - `unknown_profile_is_skipped` : Mojang `204` → aucun `PATCH`, `succeeded: 1`.
  - `failure_does_not_stop_others` : deux UUID, `PATCH` du premier en `500` → `failed: ["p1"]`, `succeeded: 1`.
  - `rate_limit_stops_the_run_with_partial_summary` : deux UUID, Mojang `429` sur le premier → second jamais appelé (`expect(0)`), résultat `Ok` avec `processed: 1`.
  - Dans `scheduler.rs`, mettre à jour l'assertion : `["purge", "badges", "skins"]`.

- [ ] **Step 2: Lancer les tests et vérifier qu'ils échouent**

Run: `cargo test -- skin_service scheduler`
Expected: échec de compilation.

- [ ] **Step 3: Implémenter `skin_service.rs` et brancher `skins_task` dans `scheduler.rs`**

- [ ] **Step 4: Lancer les tests et vérifier qu'ils passent**

Run: `cargo test -- skin_service scheduler`
Expected: PASS.

- [ ] **Step 5: Documenter**
  - `docs/daily_tasks.md` : « trois tâches » ; section « Skins Minecraft » : ce que fait la tâche, le délai de 1,5 s, `429` → arrêt et reprise le lendemain, joueurs inconnus de Mojang ignorés sans effacer leurs images, sens de `processed`/`succeeded`/`failed`.
  - `docs/pocketbase.md` et `docs/en/pocketbase.md` : champs `players.head` (fichier, 128×128), `players.skin` (fichier, 128×256), `players.skin_hash` (texte, `<hash>:<modèle>`, à créer à la main).

- [ ] **Step 6: Laisser les changements non commités**

---

### Task 5: Tête PocketBase dans l'image de résumé Minecraft

**Files:**
- Modify: `src/models/minecraft.rs` (`MinecraftPlayer`)
- Modify: `src/services/image_service.rs:173-175`

**Interfaces:**
- Produces:
  - `MinecraftPlayer` gagne `#[serde(default, rename = "collectionId")] pub collection_id: String` et `#[serde(default)] pub head: String`.
  - `pub fn minecraft_avatar_url(player: &MinecraftPlayer, pb_url: &str) -> String` dans `image_service.rs` : `<pb_url>/api/files/<collection_id>/<id>/<head>` si `head` et `collection_id` non vides, sinon `https://mc-heads.net/avatar/<account_id>/200`.

- [ ] **Step 1: Écrire les tests** (`image_service.rs`, module de tests existant)
  - `avatar_uses_pocketbase_head_when_present` : `head = "head_x1.png"`, `collection_id = "col"`, `id = "p1"` → `"http://pb/api/files/col/p1/head_x1.png"`.
  - `avatar_falls_back_to_mc_heads` : `head = ""` → `"https://mc-heads.net/avatar/<account_id>/200"`.

- [ ] **Step 2: Lancer les tests et vérifier qu'ils échouent**

Run: `cargo test avatar_`
Expected: échec de compilation.

- [ ] **Step 3: Implémenter** `minecraft_avatar_url` et l'utiliser à la place du `format!` ligne 174, avec `Config::global().auth.pb_url`.

- [ ] **Step 4: Lancer toute la suite**

Run: `cargo test` puis `cargo clippy --all-targets`
Expected: tous les tests PASS, aucun nouvel avertissement.

- [ ] **Step 5: Laisser les changements non commités**

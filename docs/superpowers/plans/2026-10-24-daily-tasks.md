# Tâches quotidiennes OAPI — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Une boucle de fond lancée au démarrage d'OAPI qui, chaque jour à 04:00 heure de Paris, purge les comptes Discord dont `delete_at` est dépassée puis recalcule en silence les badges de tous les comptes.

**Architecture:** `src/services/scheduler.rs` calcule le prochain réveil (`chrono-tz`, `Europe/Paris`) et exécute une liste ordonnée de tâches avec un `PocketbaseClient` connecté. La purge vit dans `src/services/purge_service.rs` ; le recalcul global des badges est une nouvelle fonction de `badge_service.rs` qui réutilise `evaluate_with_definitions`. La boucle est lancée par `tokio::spawn` dans `main`.

**Tech Stack:** Rust 2024, tokio, chrono + chrono-tz (nouvelle dépendance), PocketBase REST, mockito pour les tests.

**Spec:** `docs/superpowers/specs/2026-10-24-daily-tasks-design.md`

## Global Constraints

- Nouvelle dépendance unique : `chrono-tz` ; fuseau `Europe/Paris`.
- Config : section `scheduler` avec `enabled` (défaut `true`) et `run_at` (défaut `"04:00"`, format `HH:MM`, heure de Paris). Section absente → défauts. `run_at` invalide → erreur au démarrage.
- Ordre des tâches : **purge puis badges**. Une tâche en échec n'empêche pas la suivante.
- Sélection purge : `delete_at != '' && delete_at <= @now` sur `discord_users`.
- Ordre de purge par compte (`R` = ID record, `S` = `discord_id`) : `discord_user_stats` → `badges_earned` (champ `discord_user`) → `discord_friendships` (`user_a`/`user_b` = `R`) → `discord_identity_token` (`discord_user = R`) → `players` (mettre `discord_user` à `""`, ne jamais supprimer) → `discord_users` (supprimer `R`).
- Les filtres sur `discord_user` (stats, badges, players) couvrent `R` **et** `S`. `S` n'est utilisé que s'il passe `badge_service::valid_id` ; `R` doit passer `valid_id` sinon le compte est en échec sans aucune écriture.
- Une erreur sur une étape arrête ce compte (le record `discord_users` reste), les autres continuent.
- `players_stats` et badges Minecraft ne sont jamais touchés.
- Badges : `evaluate_with_definitions` pour chaque `discord_users` puis chaque `players`, `badges.yaml` lu une seule fois ; jamais `refresh_discord_account`. Aucune notification.
- Logs : seulement des IDs record et des compteurs, aucune donnée personnelle (pseudo, tag, snowflake).
- Traitement séquentiel. Une panique dans une exécution ne tue ni la boucle ni le serveur HTTP.
- Ne pas modifier ni commiter `deploy/config.production.yaml` (modifications locales de l'utilisateur en cours).

## Review Focus

1. `run_at` qui tombe dans le trou du passage à l'heure d'été (ex. `02:30` le dernier dimanche de mars) → exécution à la première heure valide suivante, pas de panique ni de boucle infinie (test Task 1 `next_run_skips_dst_gap`).
2. `run_at` dans l'heure dupliquée d'octobre (`02:30`) → une seule exécution ce jour-là (test Task 1 `next_run_runs_once_on_ambiguous_hour`).
3. Compte à purger dont `discord_id` est vide ou invalide → purge sur `R` seul, sans filtre `discord_user = ''` qui viserait des lignes non liées (test Task 2 `account_filter_ignores_invalid_snowflake`).
4. Compte dont une suppression échoue au milieu → `discord_users` non supprimé, compte suivant traité (test Task 2 `failed_step_keeps_account_and_continues`).
5. `badges.yaml` invalide → tâche badges en erreur loggée, la purge déjà faite n'est pas affectée (couvert par l'ordre dans Task 4 `run_tasks_continues_after_failure`).

---

### Task 1: Configuration et calcul du prochain réveil

**Files:**
- Modify: `Cargo.toml` (ajouter `chrono-tz = "0.10"`)
- Modify: `src/config.rs` (struct `SchedulerConfig`, champ `Config.scheduler`, `YamlParts`, validation)
- Modify: `default_config.yaml` (section `scheduler`)
- Create: `src/services/scheduler.rs` ; Modify: `src/services/mod.rs` (`pub mod scheduler;`)

**Interfaces:**
- Produces:
  - `config::SchedulerConfig { pub enabled: bool, pub run_at: String }`, `Default` = `{ enabled: true, run_at: "04:00" }`, `#[serde(default)]` sur le champ `Config.scheduler`.
  - `impl SchedulerConfig { pub fn run_time(&self) -> Result<chrono::NaiveTime, String> }` (format `%H:%M`).
  - `Config::validate_security` renvoie `Err` si `scheduler.run_time()` échoue.
  - `scheduler::next_run(now: DateTime<Utc>, run_at: NaiveTime) -> DateTime<Utc>` : premier instant strictement après `now` où l'heure de Paris vaut `run_at`. Trou d'heure d'été → avancer d'une heure locale et réessayer ; heure ambiguë → `earliest()`.

- [ ] **Step 1: Écrire les tests en échec**

Dans `src/config.rs` (`mod tests`) :

```rust
#[test]
fn scheduler_defaults_and_rejects_invalid_time() {
    let mut config = Config::load().unwrap();
    config.auth.jwt_secret = "test-only-secret-at-least-32-bytes-long".into();
    assert!(config.scheduler.enabled);
    assert_eq!(config.scheduler.run_time().unwrap(), NaiveTime::from_hms_opt(4, 0, 0).unwrap());
    for invalid in ["", "4h", "25:00", "04:60"] {
        config.scheduler.run_at = invalid.into();
        assert!(config.validate_security().is_err(), "{invalid}");
    }
}
```

Dans `src/services/scheduler.rs` (`mod tests`), avec `utc("2026-…")` = parse RFC 3339 :

```rust
#[test]
fn next_run_is_today_or_tomorrow_in_paris_time() {
    let four = NaiveTime::from_hms_opt(4, 0, 0).unwrap();
    // Hiver (UTC+1) : 04:00 Paris = 03:00 UTC
    assert_eq!(next_run(utc("2026-01-10T01:00:00Z"), four), utc("2026-01-10T03:00:00Z"));
    assert_eq!(next_run(utc("2026-01-10T03:00:00Z"), four), utc("2026-01-11T03:00:00Z"));
    // Été (UTC+2) : 04:00 Paris = 02:00 UTC
    assert_eq!(next_run(utc("2026-07-10T01:00:00Z"), four), utc("2026-07-10T02:00:00Z"));
}

#[test]
fn next_run_skips_dst_gap() {
    // 29 mars 2026 : 02:00 → 03:00 à Paris ; 02:30 n'existe pas
    let t = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
    assert_eq!(next_run(utc("2026-03-29T00:00:00Z"), t), utc("2026-03-29T01:30:00Z"));
}

#[test]
fn next_run_runs_once_on_ambiguous_hour() {
    // 25 octobre 2026 : 02:30 Paris existe deux fois ; seule la première compte
    let t = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
    let first = next_run(utc("2026-10-24T23:00:00Z"), t);
    assert_eq!(first, utc("2026-10-25T00:30:00Z"));
    assert_eq!(next_run(first, t), utc("2026-10-26T01:30:00Z"));
}
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test scheduler`
Expected: erreurs de compilation (`SchedulerConfig`, `next_run` absents).

- [ ] **Step 3: Implémenter**

Ajouter `chrono-tz`, `SchedulerConfig` (doc-commentaires en anglais comme le reste de `config.rs`), le champ dans `YamlParts` (`#[serde(default)]`) et dans le `Config` construit par `load_from`, la validation, puis `next_run`. Dans `default_config.yaml` :

```yaml
# Daily maintenance tasks (account purge, then badge refresh)
scheduler:
  enabled: true
  run_at: "04:00" # Europe/Paris local time, HH:MM
```

- [ ] **Step 4: Vérifier**

Run: `cargo test`
Expected: tous les tests passent (101 existants + 4 nouveaux).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/config.rs default_config.yaml src/services/mod.rs src/services/scheduler.rs
git commit -m "feat(scheduler): add daily schedule configuration and next run computation"
```

---

### Task 2: Purge des comptes Discord à supprimer

**Files:**
- Create: `src/services/purge_service.rs` ; Modify: `src/services/mod.rs` (`pub mod purge_service;`)
- Modify: `src/services/scheduler.rs` (ajouter `TaskSummary`)

**Interfaces:**
- Consumes: `PocketbaseClient::{list_all_records, update_record, delete_record}`, `badge_service::valid_id`, constantes `DISCORD_USERS_COLLECTION`, `DISCORD_USER_STATS_COLLECTION`, `DISCORD_USER_BADGES_COLLECTION`, `DISCORD_FRIENDSHIPS_COLLECTION`, `DISCORD_IDENTITY_TOKEN_COLLECTION`, `MINECRAFT_PLAYERS_COLLECTION`.
- Produces:
  - `scheduler::TaskSummary { pub processed: usize, pub succeeded: usize, pub failed: Vec<String> }` (`#[derive(Debug, Default, PartialEq)]`), `failed` = IDs record.
  - `purge_service::purge_deleted_accounts(pb: &PocketbaseClient) -> Result<TaskSummary, String>` : `Err` seulement si la liste des comptes à purger ne peut pas être lue.
  - privées : `account_filter(field: &str, record: &str, snowflake: Option<&str>) -> String`, `purge_account(pb, record: &str, snowflake: Option<&str>) -> Result<(), String>`, `delete_matching(pb, collection: &str, filter: &str) -> Result<(), String>`.

- [ ] **Step 1: Écrire les tests en échec**

Tests purs :

```rust
#[test]
fn account_filter_matches_record_and_snowflake() {
    assert_eq!(account_filter("discord_user", "rec1", Some("123")),
               "discord_user = 'rec1' || discord_user = '123'");
}

#[test]
fn account_filter_ignores_invalid_snowflake() {
    // Appelant : snowflake = Some(s).filter(|s| valid_id(s))
    assert_eq!(account_filter("discord_user", "rec1", None), "discord_user = 'rec1'");
}
```

Tests mockito (même helpers `page` / `list_mock` que `rom_auth_actions.rs`, `PocketbaseClient::for_test`) :

- `purges_linked_data_in_order_then_deletes_account` : `discord_users` renvoie `[{"id":"rec1","discord_id":"123"}]` pour le filtre `delete_at != '' && delete_at <= @now` ; chaque collection liée renvoie une ligne (`s1`, `b1`, `f1`, `t1`, `p1`) pour son filtre exact ; mocks `DELETE` sur `s1`, `b1`, `f1`, `t1`, `discord_users/rec1` avec `.expect(1)` ; `PATCH players/p1` avec `Matcher::Json(json!({"discord_user": ""}))` `.expect(1)` ; **aucun** mock `DELETE players/*` ni requête `players_stats`. Résultat : `TaskSummary { processed: 1, succeeded: 1, failed: vec![] }`. Filtres attendus :
  - stats / badges / players : `discord_user = 'rec1' || discord_user = '123'`
  - friendships : `user_a = 'rec1' || user_b = 'rec1'`
  - identity token : `discord_user = 'rec1'`
- `failed_step_keeps_account_and_continues` : deux comptes `rec1`, `rec2` ; `DELETE` de la ligne de stats de `rec1` renvoie `500` ; `DELETE discord_users/rec1` `.expect(0)` ; `DELETE discord_users/rec2` `.expect(1)`. Résultat : `processed: 2, succeeded: 1, failed: vec!["rec1"]`.
- `invalid_record_id_is_failed_without_writes` : compte `{"id":"bad id"}` → `failed: vec!["bad id"]`, aucun autre appel.

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test purge`
Expected: échec de compilation (`purge_service` absent).

- [ ] **Step 3: Implémenter `purge_deleted_accounts`**

Liste des comptes avec `list_all_records::<Value>` ; `?` sur chaque étape de `purge_account` pour s'arrêter au premier échec. Les erreurs par compte sont loggées en `tracing::warn!` avec l'ID record seulement.

- [ ] **Step 4: Vérifier**

Run: `cargo test`
Expected: tous les tests passent.

- [ ] **Step 5: Commit**

```bash
git add src/services/mod.rs src/services/purge_service.rs src/services/scheduler.rs
git commit -m "feat(scheduler): purge Discord accounts past their deletion date"
```

---

### Task 3: Recalcul global des badges

**Files:**
- Modify: `src/services/badge_service.rs`

**Interfaces:**
- Consumes: `scheduler::TaskSummary` (Task 2), `catalogue()`, `evaluate_with_definitions` (privés au module).
- Produces: `badge_service::refresh_all(pb: &PocketbaseClient) -> Result<TaskSummary, String>` : `Err` si `badges.yaml` est invalide ou si une des deux listes ne peut pas être lue ; sinon évalue chaque `discord_users` (kind `DiscordUser`) puis chaque `players` (kind `Player`). `succeeded` = comptes évalués sans erreur ; le nombre de badges créés (`created == true`) est loggé en `info!`. Privée testable : `refresh_all_with_definitions(pb, definitions: Vec<BadgeDefinition>) -> Result<TaskSummary, String>` (`BadgeDefinition` doit dériver `Clone`).

- [ ] **Step 1: Écrire le test en échec**

`refresh_all_continues_after_account_failure` (mockito, `refresh_all_with_definitions(&pb, vec![])`) :
- `GET /api/collections/discord_users/records` (sans filtre) → `[{"id":"d1"}]` ;
- `GET /api/collections/players/records` (sans filtre) → `[{"id":"p1"}]` ;
- `GET /api/collections/discord_users/records/d1` → `500`, `.expect(1)` (premier appel de `evaluate_with_definitions` : `find`) ;
- `GET /api/collections/players/records/p1` → `{"id":"p1","account_id":"0123456789abcdef0123456789abcdef"}`, `.expect(1)` ;
- `GET /api/collections/badges_earned/records` (filtre `player = 'p1' || player = '0123456789abcdef0123456789abcdef'`) → `[]`.

Avec des définitions vides, l'évaluation de `p1` s'arrête après `badges_earned` et réussit.
Attendu : `TaskSummary { processed: 2, succeeded: 1, failed: vec!["d1".into()] }`, et l'ordre Discord puis Minecraft respecté.

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test refresh_all`
Expected: échec de compilation.

- [ ] **Step 3: Implémenter `refresh_all` et `refresh_all_with_definitions`**

`catalogue()` une seule fois, puis boucle séquentielle ; erreurs par compte en `warn!` avec l'ID record.

- [ ] **Step 4: Vérifier**

Run: `cargo test`
Expected: tous les tests passent.

- [ ] **Step 5: Commit**

```bash
git add src/services/badge_service.rs
git commit -m "feat(badges): refresh badges of every account"
```

---

### Task 4: Boucle quotidienne, branchement dans `main` et documentation

**Files:**
- Modify: `src/services/scheduler.rs`
- Modify: `src/main.rs` (appel après `config::init()` / validation, avant `axum::serve`)
- Create: `docs/daily_tasks.md` ; Modify: `docs/configuration.md` (section `scheduler`), `README.md` (lien vers `docs/daily_tasks.md` s'il liste les docs)

**Interfaces:**
- Consumes: `next_run` (Task 1), `purge_service::purge_deleted_accounts` (Task 2), `badge_service::refresh_all` (Task 3), `TaskSummary`.
- Produces:
  - `scheduler::run_tasks(pb: &PocketbaseClient, tasks: &[(&str, TaskFn)]) -> Vec<(String, Result<TaskSummary, String>)>` où `type TaskFn = for<'a> fn(&'a PocketbaseClient) -> BoxFuture<'a, Result<TaskSummary, String>>` (`std::pin::Pin<Box<dyn Future<…> + Send + 'a>>`, pas de crate `futures`). Exécute chaque tâche dans l'ordre, logge chaque résultat (`info!` succès avec compteurs, `error!` échec), continue après une erreur.
  - `scheduler::DAILY_TASKS` : `[("purge", …purge_deleted_accounts), ("badges", …refresh_all)]` dans cet ordre.
  - `scheduler::spawn(config: &SchedulerConfig)` : ne fait rien si `!enabled` (log `info!`) ; sinon `tokio::spawn` d'une boucle : `sleep` jusqu'à `next_run(Utc::now(), run_at)`, puis exécution d'un passage (`PocketbaseClient::new()`, `login`, `run_tasks(&pb, &DAILY_TASKS)`) dans un `tokio::spawn` interne attendu via son `JoinHandle`, pour qu'une panique soit loggée sans arrêter la boucle. Échec de `login` → `error!` et attente du lendemain.

- [ ] **Step 1: Écrire le test en échec**

```rust
#[tokio::test]
async fn run_tasks_continues_after_failure() {
    let pb = PocketbaseClient::for_test("http://127.0.0.1:9".into(), "t");
    fn fail(_: &PocketbaseClient) -> BoxFuture<'_, Result<TaskSummary, String>> {
        Box::pin(async { Err("boom".into()) })
    }
    fn ok(_: &PocketbaseClient) -> BoxFuture<'_, Result<TaskSummary, String>> {
        Box::pin(async { Ok(TaskSummary { processed: 1, succeeded: 1, failed: vec![] }) })
    }
    let results = run_tasks(&pb, &[("purge", fail), ("badges", ok)]).await;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0], ("purge".into(), Err("boom".into())));
    assert!(results[1].1.is_ok());
}

#[test]
fn daily_tasks_run_purge_before_badges() {
    assert_eq!(DAILY_TASKS.map(|(name, _)| name), ["purge", "badges"]);
}
```

- [ ] **Step 2: Vérifier l'échec**

Run: `cargo test run_tasks`
Expected: échec de compilation.

- [ ] **Step 3: Implémenter `run_tasks`, `DAILY_TASKS`, `spawn`, puis appeler `services::scheduler::spawn(&config.scheduler)` dans `main`**

- [ ] **Step 4: Documenter**

`docs/daily_tasks.md` en français, dans le style de `docs/badges.md` : rôle des tâches, horaire et config, périmètre et ordre exact de la purge (tableau du spec), conservation des comptes Minecraft, reprise le lendemain en cas d'échec, absence de rattrapage si OAPI est arrêté à l'heure prévue, badges silencieux. Ajouter la section `scheduler` à `docs/configuration.md`.

- [ ] **Step 5: Vérifier**

Run: `cargo test` puis `cargo build --release`
Expected: tous les tests passent, build sans warning nouveau. Lancer `cargo run` en local et vérifier la ligne de log indiquant la prochaine exécution (ex. `Next daily tasks run at …`).

- [ ] **Step 6: Commit**

```bash
git add src/services/scheduler.rs src/main.rs docs/daily_tasks.md docs/configuration.md README.md
git commit -m "feat(scheduler): run daily maintenance tasks at startup-configured time"
```

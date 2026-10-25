# Utilisation de PocketBase (Client API)

Le projet utilise PocketBase comme source de données centrale. L'utilitaire `src/utils/pocketbase.rs` fournit un client robuste pour interagir avec l'API REST de PocketBase.

---

## Fonctionnalités du Client

### 1. Authentification Admin
Le client s'authentifie automatiquement comme administrateur lors de l'appel à `login()`. Cela permet de passer outre les règles de visibilité publiques et de lire toutes les collections nécessaires à la génération des stats.

### 2. Gestion de la Pagination (Exhaustivité)
Contrairement aux appels classiques qui sont limités à 100 résultats par PocketBase, la méthode `list_all_records` :
- Boucle sur toutes les pages disponibles.
- Utilise une taille de page de 500 pour optimiser les performances.
- Fusionne tous les résultats avant de les retourner.

C'est ce qui permet de calculer un temps de jeu total exact sur des milliers d'enregistrements.

---

## Exemple d'utilisation dans une Action

```rust
let mut pb = PocketbaseClient::new();
pb.login().await?;

// Récupérer TOUTES les stats d'un joueur (même s'il y a 5000 lignes)
let stats: Vec<MinecraftStats> = pb.list_all_records("players_stats", &filter).await?;
```

---

## Collection `discord_identity_token`

Tokens de liaison ROM / Discord (voir [rom_auth.md](rom_auth.md)). Champs : `discord_user`, `label`, `linking_code`, `linking_code_expires_at`, `token` (hash SHA-256, **sans « Nonempty »**), `is_disabled`. Index uniques partiels requis :

```sql
CREATE UNIQUE INDEX idx_identity_token ON discord_identity_token (token) WHERE token != ''
CREATE UNIQUE INDEX idx_identity_linking_code ON discord_identity_token (linking_code) WHERE linking_code != ''
```

---

## Collection `players` : champs de skin

Remplis chaque jour par la tâche `skins` (voir [daily_tasks.md](daily_tasks.md)) :

- `head` : fichier (un seul, PNG 128×128), tête du joueur ;
- `skin` : fichier (un seul, PNG 128×256), skin en pied ;
- `skin_hash` : texte, de la forme `<hash>:<modèle>` (par exemple `abc123:classic` ou `abc123:slim`). Ce champ est à créer à la main dans PocketBase.

---

## Sécurité
- Le jeton d'authentification (`token`) est stocké localement dans l'instance du client.
- Les erreurs de requête sont logguées via `tracing` avec le corps de la réponse brute pour faciliter le diagnostic (ex: erreurs de filtre 400).
